//! Pure sizing logic: arb shell + directional tilt from modeled edge.

use rust_decimal::Decimal;

/// Maximum outcome-share decimals enforced by Polymarket (`LOT_SIZE_SCALE` in their SDK).
pub const SHARE_DECIMALS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoredOutcome {
    /// Outcome A token is favored (more modeled edge vs ask).
    A,
    /// Outcome B token is favored.
    B,
    /// Edge gap too small — remain symmetric paired hedge only.
    Neutral,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrategyParams {
    pub min_locked_edge: Decimal,
    pub max_usdc_notional: Decimal,
    pub base_pair_shares: Decimal,
    pub max_tilt_extra_shares: Decimal,
    pub tilt_edge_gap: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SizePlan {
    pub favored: FavoredOutcome,
    pub paired_shares: Decimal,
    pub qty_a: Decimal,
    pub qty_b: Decimal,
}

#[inline]
pub fn trunc_shares(x: Decimal) -> Decimal {
    x.max(Decimal::ZERO).trunc_with_scale(SHARE_DECIMALS)
}

/// False when asks, model probability, or caps cannot form a locked pair.
pub fn inputs_are_tradable(
    ask_a: Decimal,
    ask_b: Decimal,
    fair_prob_a: Decimal,
    params: &StrategyParams,
) -> bool {
    if ask_a <= Decimal::ZERO || ask_b <= Decimal::ZERO {
        return false;
    }
    if fair_prob_a < Decimal::ZERO || fair_prob_a > Decimal::ONE {
        return false;
    }
    if params.min_locked_edge <= Decimal::ZERO || params.min_locked_edge >= Decimal::ONE {
        return false;
    }
    if params.max_usdc_notional <= Decimal::ZERO || params.base_pair_shares <= Decimal::ZERO {
        return false;
    }
    if params.tilt_edge_gap < Decimal::ZERO || params.max_tilt_extra_shares < Decimal::ZERO {
        return false;
    }
    true
}

/// Decide quantities only (limit prices are derived later from the book).
pub fn plan_sizes(
    ask_a: Decimal,
    ask_b: Decimal,
    fair_prob_a: Decimal,
    params: &StrategyParams,
    min_order_size: Decimal,
) -> Option<SizePlan> {
    if !inputs_are_tradable(ask_a, ask_b, fair_prob_a, params) {
        return None;
    }

    let one = Decimal::ONE;
    let bundle = ask_a + ask_b;

    let max_bundle = one.checked_sub(params.min_locked_edge)?;
    if bundle > max_bundle {
        return None;
    }

    let edge_a = fair_prob_a - ask_a;
    let edge_b = (one - fair_prob_a) - ask_b;

    let favored = if edge_a - edge_b >= params.tilt_edge_gap {
        FavoredOutcome::A
    } else if edge_b - edge_a >= params.tilt_edge_gap {
        FavoredOutcome::B
    } else {
        FavoredOutcome::Neutral
    };

    let pair_cost = bundle;
    let max_q_usdc = params.max_usdc_notional.checked_div(pair_cost)?;
    let mut q_pair = max_q_usdc.min(params.base_pair_shares);
    q_pair = trunc_shares(q_pair);

    if q_pair < min_order_size {
        return None;
    }

    let mut extra = Decimal::ZERO;
    match favored {
        FavoredOutcome::A => {
            extra = params.max_tilt_extra_shares;
            let spent_pair = q_pair * pair_cost;
            let remaining = (params.max_usdc_notional - spent_pair).max(Decimal::ZERO);
            if remaining > Decimal::ZERO && ask_a > Decimal::ZERO {
                let cap_extra = trunc_shares(remaining.checked_div(ask_a)?);
                extra = extra.min(cap_extra);
            }
            extra = trunc_shares(extra);
            if extra > Decimal::ZERO && extra < min_order_size {
                extra = Decimal::ZERO;
            }
        }
        FavoredOutcome::B => {
            extra = params.max_tilt_extra_shares;
            let spent_pair = q_pair * pair_cost;
            let remaining = (params.max_usdc_notional - spent_pair).max(Decimal::ZERO);
            if remaining > Decimal::ZERO && ask_b > Decimal::ZERO {
                let cap_extra = trunc_shares(remaining.checked_div(ask_b)?);
                extra = extra.min(cap_extra);
            }
            extra = trunc_shares(extra);
            if extra > Decimal::ZERO && extra < min_order_size {
                extra = Decimal::ZERO;
            }
        }
        FavoredOutcome::Neutral => {}
    }

    let (qty_a, qty_b) = match favored {
        FavoredOutcome::A => (q_pair + extra, q_pair),
        FavoredOutcome::B => (q_pair, q_pair + extra),
        FavoredOutcome::Neutral => (q_pair, q_pair),
    };

    if qty_a < min_order_size || qty_b < min_order_size {
        return None;
    }

    Some(SizePlan {
        favored,
        paired_shares: q_pair,
        qty_a,
        qty_b,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn params() -> StrategyParams {
        StrategyParams {
            min_locked_edge: dec!(0.02),
            max_usdc_notional: dec!(100),
            base_pair_shares: dec!(50),
            max_tilt_extra_shares: dec!(25),
            tilt_edge_gap: dec!(0.02),
        }
    }

    #[test]
    fn rejects_when_bundle_too_wide() {
        let p = params();
        assert!(
            plan_sizes(dec!(0.55), dec!(0.55), dec!(0.5), &p, dec!(1)).is_none()
        );
    }

    #[test]
    fn symmetric_when_edge_gap_small() {
        let p = params();
        let plan = plan_sizes(dec!(0.48), dec!(0.49), dec!(0.50), &p, dec!(1)).unwrap();
        assert_eq!(plan.favored, FavoredOutcome::Neutral);
        assert_eq!(plan.qty_a, plan.qty_b);
    }

    #[test]
    fn tilts_to_a_when_model_prefers_a() {
        let p = params();
        let plan = plan_sizes(dec!(0.45), dec!(0.49), dec!(0.62), &p, dec!(1)).unwrap();
        assert_eq!(plan.favored, FavoredOutcome::A);
        assert!(plan.qty_a > plan.qty_b);
        assert_eq!(plan.qty_b, plan.paired_shares);
    }

    #[test]
    fn rejects_zero_or_negative_asks() {
        let p = params();
        assert!(plan_sizes(dec!(0), dec!(0.40), dec!(0.50), &p, dec!(1)).is_none());
        assert!(plan_sizes(dec!(-0.10), dec!(0.40), dec!(0.50), &p, dec!(1)).is_none());
        assert!(plan_sizes(dec!(0.40), dec!(0), dec!(0.50), &p, dec!(1)).is_none());
    }

    #[test]
    fn rejects_fair_probability_outside_unit_interval() {
        let p = params();
        assert!(plan_sizes(dec!(0.45), dec!(0.45), dec!(-0.01), &p, dec!(1)).is_none());
        assert!(plan_sizes(dec!(0.45), dec!(0.45), dec!(1.01), &p, dec!(1)).is_none());
    }

    #[test]
    fn rejects_locked_edge_outside_open_unit_interval() {
        let mut p = params();
        p.min_locked_edge = dec!(0);
        assert!(plan_sizes(dec!(0.40), dec!(0.40), dec!(0.50), &p, dec!(1)).is_none());
        p.min_locked_edge = dec!(1);
        assert!(plan_sizes(dec!(0.40), dec!(0.40), dec!(0.50), &p, dec!(1)).is_none());
    }
}
