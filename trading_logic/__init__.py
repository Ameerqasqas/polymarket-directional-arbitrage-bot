"""
Trading logic package.

This layer is intended for context-aware filters that sit on top of raw
indicator signals before final execution decisions are made.
"""

from .decision_filter import DecisionFilter, DecisionContext

__all__ = ["DecisionFilter", "DecisionContext"]
