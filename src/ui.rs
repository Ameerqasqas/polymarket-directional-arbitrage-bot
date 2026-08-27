//! Terminal dashboard: live asks vs 60s TWAP, plan, and clip progress.

use std::collections::VecDeque;
use std::io::{self, stdout, IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table};
use ratatui::{backend::CrosstermBackend, Terminal};
use rust_decimal::Decimal;
use tracing_subscriber::fmt::MakeWriter;

use crate::strategy::{FavoredOutcome, SizePlan};

pub const LOG_CAP: usize = 80;

#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<String>>>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAP))),
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BufferWriter {
    buf: LogBuffer,
}

impl Write for BufferWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(bytes);
        if let Ok(mut g) = self.buf.inner.lock() {
            for line in text.split('\n') {
                let line = line.trim_end();
                if line.is_empty() {
                    continue;
                }
                if g.len() >= LOG_CAP {
                    g.pop_front();
                }
                g.push_back(line.to_string());
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogBuffer {
    type Writer = BufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        BufferWriter { buf: self.clone() }
    }
}

#[derive(Debug, Clone)]
pub struct DashboardState {
    pub dry_run: bool,
    pub label_a: String,
    pub label_b: String,
    pub fair_a: Decimal,
    pub min_locked_edge: Decimal,
    pub live_ask_a: Option<Decimal>,
    pub live_ask_b: Option<Decimal>,
    pub twap_ask_a: Option<Decimal>,
    pub twap_ask_b: Option<Decimal>,
    pub twap_span_secs: f64,
    pub twap_window_secs: u64,
    pub twap_samples: usize,
    pub twap_ready: bool,
    pub plan: Option<SizePlan>,
    pub px_a: Option<Decimal>,
    pub px_b: Option<Decimal>,
    pub slice_idx: u32,
    pub slices: u32,
    pub executing: bool,
    pub submitted_a: Decimal,
    pub submitted_b: Decimal,
    pub last_order_a: Option<String>,
    pub last_order_b: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

impl DashboardState {
    pub fn notion(&self) -> Option<Decimal> {
        let plan = self.plan.as_ref()?;
        Some(plan.qty_a * self.px_a? + plan.qty_b * self.px_b?)
    }
}

fn dec_s(x: Option<Decimal>) -> String {
    x.map(|d| format!("{d:.4}"))
        .unwrap_or_else(|| "—".to_string())
}

fn favored_label(plan: Option<&SizePlan>, a: &str, b: &str) -> String {
    match plan.map(|p| p.favored) {
        Some(FavoredOutcome::A) => format!("{a} (tilt)"),
        Some(FavoredOutcome::B) => format!("{b} (tilt)"),
        Some(FavoredOutcome::Neutral) => "paired / neutral".into(),
        None => "—".into(),
    }
}

/// Single-shot / non-TTY panel (no raw mode).
pub fn format_panel(s: &DashboardState) -> String {
    let mode = if s.dry_run { "DRY-RUN" } else { "LIVE" };
    let live_bundle = match (s.live_ask_a, s.live_ask_b) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    };
    let twap_bundle = match (s.twap_ask_a, s.twap_ask_b) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    };
    let (qa, qb, pair) = s
        .plan
        .as_ref()
        .map(|p| (format!("{:.2}", p.qty_a), format!("{:.2}", p.qty_b), format!("{:.2}", p.paired_shares)))
        .unwrap_or_else(|| ("—".into(), "—".into(), "—".into()));

    format!(
        "\
╔══════════════════════════════════════════════════════════════════╗
║  Directional Arb   TWAP {win:>3}s   {mode:<8}                       ║
╠══════════════════════════════════════════════════════════════════╣
║  {a:<10}  live {la:<10}  twap {ta:<10}                   ║
║  {b:<10}  live {lb:<10}  twap {tb:<10}                   ║
║  bundle      live {lbund:<10}  twap {tbund:<10}                   ║
║  fair P({a_short})={fair}   min locked edge={edge}   samples={samp:<3}     ║
║  favored={fav:<22}  ready={ready}                        ║
║  plan pair={pair:<8} {a_short}={qa:<8} {b_short}={qb:<8}  ~{notion} USDC     ║
║  limits {a_short}@{pxa:<8} {b_short}@{pxb:<8}  clip {clip}/{slices}            ║
║  submitted {a_short}={sa:<8} {b_short}={sb:<8}                           ║
║  {status:<62} ║
╚══════════════════════════════════════════════════════════════════╝",
        win = s.twap_window_secs,
        mode = mode,
        a = s.label_a,
        b = s.label_b,
        a_short = truncate(&s.label_a, 4),
        b_short = truncate(&s.label_b, 4),
        la = dec_s(s.live_ask_a),
        lb = dec_s(s.live_ask_b),
        ta = dec_s(s.twap_ask_a),
        tb = dec_s(s.twap_ask_b),
        lbund = dec_s(live_bundle),
        tbund = dec_s(twap_bundle),
        fair = format!("{:.3}", s.fair_a),
        edge = format!("{:.3}", s.min_locked_edge),
        samp = s.twap_samples,
        fav = favored_label(s.plan.as_ref(), &s.label_a, &s.label_b),
        ready = if s.twap_ready { "yes" } else { "warming" },
        pair = pair,
        qa = qa,
        qb = qb,
        notion = s
            .notion()
            .map(|n| format!("{n:.2}"))
            .unwrap_or_else(|| "—".into()),
        pxa = dec_s(s.px_a),
        pxb = dec_s(s.px_b),
        clip = if s.executing { s.slice_idx } else { 0 },
        slices = s.slices,
        sa = format!("{:.2}", s.submitted_a),
        sb = format!("{:.2}", s.submitted_b),
        status = truncate(&s.error.clone().unwrap_or_else(|| s.status.clone()), 62),
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

pub fn stdout_is_tty() -> bool {
    io::stdout().is_terminal()
}

pub fn print_panel(s: &DashboardState) {
    println!("{}", format_panel(s));
}

pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

/// Enter alternate screen + raw mode. Restore on [`restore_tui`].
pub fn enter_tui() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

pub fn restore_tui(terminal: &mut Tui) {
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}

pub fn draw_dashboard(terminal: &mut Tui, s: &DashboardState, logs: &LogBuffer) -> Result<()> {
    terminal.draw(|frame| draw(frame, s, logs))?;
    Ok(())
}

pub fn quit_pressed() -> Result<bool> {
    while event::poll(Duration::ZERO)? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press
                && matches!(
                    key.code,
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                )
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub async fn wait_or_quit(interval: Duration) -> Result<bool> {
    let sleep_until = tokio::time::Instant::now() + interval;
    loop {
        let remaining = sleep_until.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(true),
            _ = tokio::time::sleep(remaining.min(Duration::from_millis(50))) => {
                if quit_pressed()? {
                    return Ok(true);
                }
            }
        }
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, s: &DashboardState, logs: &LogBuffer) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Min(6),
        ])
        .split(frame.area());

    let mode = if s.dry_run { "DRY-RUN" } else { "LIVE" };
    let title = format!(
        " Directional Arb  ·  TWAP {}s  ·  {mode}  ·  q to quit ",
        s.twap_window_secs
    );
    let header = Paragraph::new(Line::from(vec![
        Span::styled(title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(header, chunks[0]);

    let live_bundle = match (s.live_ask_a, s.live_ask_b) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    };
    let twap_bundle = match (s.twap_ask_a, s.twap_ask_b) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    };

    let rows = vec![
        Row::new(vec![
            Cell::from("leg"),
            Cell::from("live ask"),
            Cell::from("60s TWAP"),
            Cell::from("limit"),
            Cell::from("plan qty"),
            Cell::from("submitted"),
        ])
        .style(Style::default().fg(Color::Gray)),
        quote_row(
            &s.label_a,
            s.live_ask_a,
            s.twap_ask_a,
            s.px_a,
            s.plan.as_ref().map(|p| p.qty_a),
            s.submitted_a,
        ),
        quote_row(
            &s.label_b,
            s.live_ask_b,
            s.twap_ask_b,
            s.px_b,
            s.plan.as_ref().map(|p| p.qty_b),
            s.submitted_b,
        ),
        quote_row(
            "bundle",
            live_bundle,
            twap_bundle,
            None,
            s.plan.as_ref().map(|p| p.paired_shares),
            Decimal::ZERO,
        ),
    ];
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
        ],
    )
    .block(
        Block::default()
            .title(" quotes (strategy sizes off 60s TWAP, limits off live ask) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(table, chunks[1]);

    let notion = s
        .notion()
        .map(|n| format!("{n:.2} USDC"))
        .unwrap_or_else(|| "—".into());
    let meta = Paragraph::new(vec![
        Line::from(format!(
            "fair P({}) = {:.3}    min locked edge = {:.3}    favored = {}",
            s.label_a,
            s.fair_a,
            s.min_locked_edge,
            favored_label(s.plan.as_ref(), &s.label_a, &s.label_b)
        )),
        Line::from(format!(
            "TWAP samples = {}    span = {:.1}s / {}s    ready = {}",
            s.twap_samples,
            s.twap_span_secs,
            s.twap_window_secs,
            if s.twap_ready { "yes" } else { "warming" }
        )),
        Line::from(format!(
            "approx notional = {notion}    last orders  {} / {}",
            s.last_order_a.as_deref().unwrap_or("—"),
            s.last_order_b.as_deref().unwrap_or("—")
        )),
        Line::from(
            s.error
                .clone()
                .unwrap_or_else(|| s.status.clone()),
        )
        .style(if s.error.is_some() {
            Style::default().fg(Color::Red)
        } else if s.plan.is_some() {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Yellow)
        }),
    ])
    .block(
        Block::default()
            .title(" plan ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(meta, chunks[2]);

    let frac = if s.slices == 0 {
        0.0
    } else if s.executing {
        (s.slice_idx as f64 / s.slices as f64).clamp(0.0, 1.0)
    } else {
        (s.twap_span_secs / s.twap_window_secs.max(1) as f64).clamp(0.0, 1.0)
    };
    let gauge_label = if s.executing {
        format!("TWAP clip {} / {}", s.slice_idx, s.slices)
    } else {
        format!("TWAP window {:.0}%", frac * 100.0)
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(" execution ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .ratio(frac)
        .label(gauge_label);
    frame.render_widget(gauge, chunks[3]);

    let log_text: String = logs.lines().into_iter().rev().take(12).rev().collect::<Vec<_>>().join("\n");
    let log = Paragraph::new(log_text).block(
        Block::default()
            .title(" log ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(log, chunks[4]);
}

fn quote_row(
    label: &str,
    live: Option<Decimal>,
    twap: Option<Decimal>,
    limit: Option<Decimal>,
    qty: Option<Decimal>,
    submitted: Decimal,
) -> Row<'static> {
    Row::new(vec![
        Cell::from(label.to_string()),
        Cell::from(dec_s(live)),
        Cell::from(dec_s(twap)),
        Cell::from(dec_s(limit)),
        Cell::from(qty.map(|q| format!("{q:.2}")).unwrap_or_else(|| "—".into())),
        Cell::from(format!("{submitted:.2}")),
    ])
}
