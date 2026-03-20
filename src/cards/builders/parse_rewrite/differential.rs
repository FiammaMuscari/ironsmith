use crate::cards::builders::{
    CardTextError, TotalCost, parse_activation_cost, parse_scryfall_mana_cost, tokenize_line,
};
use crate::mana::ManaCost;

use super::leaf::{display_activation_cost_segments, lower_activation_cost_cst};
use super::{ActivationCostCst, parse_activation_cost_rewrite, parse_mana_cost_rewrite};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActivationCostDiff {
    pub(crate) legacy: TotalCost,
    pub(crate) rewrite: TotalCost,
    pub(crate) rewrite_cst: ActivationCostCst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManaCostDiff {
    pub(crate) legacy: ManaCost,
    pub(crate) rewrite: ManaCost,
}

pub(crate) fn diff_activation_cost(raw: &str) -> Result<ActivationCostDiff, CardTextError> {
    let legacy = parse_activation_cost(&tokenize_line(raw, 0))?;
    let rewrite_cst = parse_activation_cost_rewrite(raw)?;
    let rewrite = lower_activation_cost_cst(&rewrite_cst)?;
    Ok(ActivationCostDiff {
        legacy,
        rewrite,
        rewrite_cst,
    })
}

pub(crate) fn diff_mana_cost(raw: &str) -> Result<ManaCostDiff, CardTextError> {
    let legacy = parse_scryfall_mana_cost(raw)?;
    let rewrite = parse_mana_cost_rewrite(raw)?;
    Ok(ManaCostDiff { legacy, rewrite })
}

pub(crate) fn assert_activation_cost_parity(raw: &str) -> Result<(), CardTextError> {
    let diff = diff_activation_cost(raw)?;
    if diff.legacy != diff.rewrite {
        return Err(CardTextError::InvariantViolation(format!(
            "rewrite activation-cost mismatch for '{raw}': legacy='{}' rewrite='{}' cst='{}'",
            diff.legacy.display(),
            diff.rewrite.display(),
            display_activation_cost_segments(&diff.rewrite_cst),
        )));
    }
    Ok(())
}

pub(crate) fn assert_mana_cost_parity(raw: &str) -> Result<(), CardTextError> {
    let diff = diff_mana_cost(raw)?;
    if diff.legacy != diff.rewrite {
        return Err(CardTextError::InvariantViolation(format!(
            "rewrite mana-cost mismatch for '{raw}': legacy='{}' rewrite='{}'",
            diff.legacy.to_oracle(),
            diff.rewrite.to_oracle(),
        )));
    }
    Ok(())
}
