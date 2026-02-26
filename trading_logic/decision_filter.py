"""
Context-aware decision filters for final trade qualification.

This module intentionally remains lightweight until strategy-specific
decision rules are migrated from ad-hoc logic into a single location.
"""

from dataclasses import dataclass
from typing import Dict, Tuple


@dataclass
class DecisionContext:
    """Minimal context payload for trading qualification."""

    market_regime: str = "unknown"
    volatility_score: float = 0.5
    confidence_floor: float = 0.55
    metadata: Dict[str, float] | None = None


class DecisionFilter:
    """
    Qualify raw signal actions with contextual constraints.

    The current implementation is intentionally conservative and returns a
    final action together with rationale so it can be audited in logs.
    """

    def qualify(self, action: str, signal_strength: float, context: DecisionContext) -> Tuple[str, str]:
        normalized_action = (action or "HOLD").upper()

        if normalized_action not in {"BUY", "SELL", "HOLD"}:
            return "HOLD", f"Unknown action '{action}'"

        if normalized_action == "HOLD":
            return "HOLD", "No trade action from signal"

        if signal_strength < context.confidence_floor:
            return "HOLD", (
                f"Signal strength {signal_strength:.2f} below confidence floor "
                f"{context.confidence_floor:.2f}"
            )

        if context.market_regime.lower() == "chop" and context.volatility_score > 0.7:
            return "HOLD", "Filtered due to high-volatility chop regime"

        return normalized_action, "Qualified by trading filter"
