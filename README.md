# Hyperliquid Trading Bot

Automated and semi-discretionary trading framework for Hyperliquid with:
- multi-timeframe signal engines,
- live panel monitoring,
- risk controls,
- and repeatable backtest output capture.

---

## What This Project Is

This is not a "set and forget" money printer.
It is a practical trading operations framework:
- generate signals,
- validate execution rules,
- monitor positions in real time,
- and learn from iterative testing.

If you trade actively, this repo is designed to support disciplined iteration:
test hypothesis -> run controlled backtest -> inspect metrics -> adjust only one variable -> rerun.

---

## Current Strategy Stack

The bot currently includes signal modules for:
- RSI: `1m`, `5m`, `1h`, `4h`
- SMA crossover: `5m`
- MACD crossover: `15m`
- Range bias: `24h low`, `7d low`
- Scalping: `1m`
- Support/Resistance: `1h`
- Bollinger Bands: `15m`, `30m`, `1h`

Signals are modular and can be toggled in configuration.

---

## Experimental Analysis Notes (Trader Format)

The repository already includes stored backtest artifacts under `results/` (multiple symbols and strategies across October 2025 runs).
Based on the current layout and naming of those outputs, this project is already suitable for workflow-level experimentation:

1. Cross-asset replication
   Same logic tested on `BTC`, `ETH`, `ENA`, `PAXG`, `XRP`, `HBAR`, etc.

2. Regime sensitivity checks
   High-frequency (`1m`) and slower (`1h`, `4h`) logic can be compared side by side.

3. Parameter sanity review
   Indicator defaults are centralized and visible in code/config, so drift is easier to control.

4. Post-run auditability
   JSON output naming and logs support reviewing what ran and when.

Practical trader guidance:
- Do not compare strategies only by raw PnL; compare drawdown shape and signal consistency.
- Review false-positive clusters by market regime (trend, chop, volatility expansion).
- Keep one-variable changes between runs to avoid "result storytelling."

---

## Installation

### Requirements
- Python `3.8+`
- Hyperliquid account and API credentials
- Funded account (if executing live orders)

### Setup

```bash
git clone https://github.com/gamma-trade-lab/Hyperliquid-Trading-Bot.git
cd Hyperliquid-Trading-Bot
pip install -r requirements.txt
```

If needed:

```bash
pip3 install -r requirements.txt
```

---

## Running

Launch panel:

```bash
python trading_panel.py
```

or

```bash
python3 trading_panel.py
```

Default behavior can be configured for:
- signal-only mode (paper workflow),
- or order execution mode (live workflow).

---

## Refactor: Trading Logic Directory

The codebase now includes a dedicated directory for discretionary-aware or context-aware logic:

- `trading_logic/`

Purpose:
- keep non-mechanical decision rules separate from pure indicator generation,
- isolate trade filters and conviction scoring,
- make future "human-intent + system-execution" workflows cleaner.

This keeps your `signals/` folder focused on indicator output, while `trading_logic/` handles higher-order trade qualification.

---

## Project Structure

```text
Hyperliquid-Trading-Bot/
|- trading_panel.py
|- requirements.txt
|- core/
|- managers/
|- signals/
|- trading_logic/                # New: discretionary/context-aware decision layer
|- panel_modules/
|- config/
|- utils/
|- results/
|- logs/
`- README.md
```

---

## Configuration

Main files:
- `config/trading_settings.py`
- `config/signal_settings.py`
- `config/system_settings.py`
- `config/backtest_settings.py`
- `config/debug_settings.py`
- `config/api_config.json`

Recommended workflow:
1. Configure API + risk defaults.
2. Enable only 1-2 strategies first.
3. Run in signal mode and validate logs.
4. Move to controlled live size only after repeated stable behavior.

---

## Risk Controls Included

- position count limits,
- stop-loss and take-profit controls,
- trailing stop behavior,
- signal validation before execution,
- timed checks to reduce noisy over-triggering.

You should still treat all automation as supervised.

---

## Suggested Trader Workflow

1. Pick one market and one strategy.
2. Run backtest and save outputs.
3. Record drawdown, trade frequency, and expectancy.
4. Add one trading filter in `trading_logic/`.
5. Re-run and compare deltas only.
6. Promote only if behavior improves across more than one symbol.

---

## Safety Notice

Crypto derivatives are high risk.
Use this software at your own risk.
No guarantee of profitability is expressed or implied.

Minimum discipline:
- start with small size,
- monitor live positions,
- define max daily loss before session starts,
- stop running a strategy when behavior deviates from its tested profile.

---

## Contribution Notes

If you add a new strategy:
1. place indicator logic in `signals/`,
2. place discretionary filters in `trading_logic/`,
3. register strategy/config entries,
4. produce backtest evidence in `results/`,
5. update this README with findings, not marketing claims.

---

Trade with process, not impulse.
