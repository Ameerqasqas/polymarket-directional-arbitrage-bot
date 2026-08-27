## Polymarket directional arbitrage bot

Rust bot that targets **binary Polymarket markets** (two complementary outcomes such as Up / Down). It behaves like a classic locked arb when both asks are cheap enough together, then **tilts size toward whichever outcome your model believes is most mispriced**, keeping the opposite leg as a **partial hedge**.

### Operational workflow

1. **Discover tokens** — Each outcome is its own ERC-1155-style outcome token on Polymarket’s CLOB. You need both decimal token IDs (from Gamma / the UI / API). Put them in `bot.toml`.
2. **Pull books** — For each outcome token, fetch the consolidated book via `GET /book` (wrapped by [`polymarket-client-sdk`](https://github.com/Polymarket/rs-clob-client)).
3. **Check arb shell** — Let \(a\) be the **60s TWAP** of the best ask for outcome A and \(b\) the TWAP ask for outcome B. If \(a + b \le 1 - \varepsilon\) with your configured **`min_locked_edge`** \(\varepsilon\), there is a classical paired discount versus \$1 payoff at resolution (before fees / latency risk). Live asks still drive the **limit price**; TWAP drives **whether** to quote and **how much**.
4. **Score directional edge** — Compare your **`fair_probability_a`** (modeled chance A wins) to the TWAP asks:
   - \(\text{edge}_A = p^\* - a_{\text{TWAP}}\)
   - \(\text{edge}_B = (1 - p^\*) - b_{\text{TWAP}}\)
5. **Size**
   - Start from a **paired hedge** \(q\) shares on **both** sides (capped by **`base_pair_shares`** and **`max_usdc_notional`**).
   - If \(\max(\text{edge}_A,\text{edge}_B) - \min(\dots) \ge\) **`tilt_edge_gap`**, add up to **`max_tilt_extra_shares`** on the favored outcome (still respecting **`max_usdc_notional`**).
6. **TWAP execution (daemon)** — After the 60s window is full, the bot splits the plan into **`twap.slices`** child GTC BUY clips (default 6, one every 10s) across that window. If the TWAP bundle stops locking, remaining clips are aborted.
7. **Quote limits only** — The bot never sends market-style IOC/FOK paths from this workflow; it builds **`OrderType::GTC`** BUY limits. Prices default to **live best ask**, optionally **`price_improve_ticks`** behind the touch for maker preference.
8. **Sign & POST** — With `POLYMARKET_PRIVATE_KEY` set, authenticate against the configured **`clob_host`**, EIP-712-sign each clip, and `POST /order`.

### Strategy intuition

- **Arb core**: \(q\) matched shares behave like a classical arb sleeve — bounded downside versus \$1 collateral logic at settlement (subject to venue rules and fees).
- **Tilt sleeve**: Additional shares on the high-edge outcome inject convex payout if your probability estimate leads the market’s slow repricing (common in fast crypto scenarios).
- **60s TWAP**: Daemon mode averages asks over `[twap] window_secs` (default 60) so a single print does not resize the whole book, then slices that plan across the window instead of dumping full size every poll.

### Configure & run

Requirements:

- Rust toolchain matching the SDK (**MSRV is currently aggressive — see `Cargo.toml`**).
- Polygon POL + allowance setup per Polymarket docs if trading live.

```powershell
copy config.example.toml bot.toml
# edit bot.toml + export POLYMARKET_PRIVATE_KEY for live mode

$env:RUST_LOG="info"
cargo run --release -- --config bot.toml --dry-run   # one-shot panel, no orders
cargo run --release -- --config bot.toml             # one-shot live (full size, 1 clip)
cargo run --release -- --config bot.toml --daemon    # live TUI, 60s TWAP + sliced clips
cargo run --release -- --config bot.toml --daemon --plain  # same engine, ASCII panel
```

`--daemon` on a TTY opens the dashboard (quotes, 60s TWAP, clip gauge, log). Press `q` or Ctrl+C to exit. Use `--plain` to print the panel on stdout instead.

Environment:

| Variable | Purpose |
|----------|---------|
| `POLYMARKET_PRIVATE_KEY` | Required for authenticated cycles (`--dry-run` skips trading). |
| `RUST_LOG` | Standard tracing filter (`info`, `debug`, …). |

### Wallet modes

Map `[wallet.signature_type]` to Polymarket’s signing modes:

| Config value | When |
|--------------|------|
| `eoa` | Private key controls funds directly (MetaMask-style EOAs after allowances). |
| `proxy` | Magic/email proxy wallets (SDK derives proxy funder via CREATE2). |
| `gnosis_safe` | Browser-wallet Safe deployment Polymarket provisions per EOA. |

### Limitations & extension hooks

- **`fair_probability_a` is static in config** — plug your spot/perp/model feed here (Rust trait / external IPC) without changing exchange plumbing.
- **No inventory-aware unwind** — This skeleton sizes **entries** only; production stacks usually layer cooldowns, venue bans checks (`/geoblock`), merge/redeem tooling, and inventory reconciliation.
- **Protocol URLs** — Use `https://clob.polymarket.com` for legacy USDC.e collateral or `https://clob-v2.polymarket.com` when your account runs on the newer pUSD stack (Polymarket docs outline both).

### Safety

Prediction-market automation can lose money quickly (model error, stale quotes, adverse selection). Run `--dry-run`, inspect logged notionals, and keep notional caps tiny until behavior matches expectations.
