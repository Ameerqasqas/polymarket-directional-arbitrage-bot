# Trading Logic

This directory contains context-aware trade qualification logic.

Design intent:
- `signals/` generates directional/indicator outputs,
- `trading_logic/` applies higher-order filters before execution.

Current module:
- `decision_filter.py`: minimal decision filter with rationale output for audit logs.

Extension ideas:
- session-aware filters (Asia/Europe/US),
- volatility regime filters,
- event blackout windows,
- portfolio-level conviction throttling.
