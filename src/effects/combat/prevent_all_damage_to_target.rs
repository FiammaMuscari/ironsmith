//! Prevent all damage to a specific target effect implementation.

use super::prevention_helpers::{
    PreventionTargetResolveMode, register_prevention_shield, resolve_prevention_target_from_spec,
};
use crate::effect::{EffectOutcome, EffectResult, Until};
use crate::effects::EffectExecutor;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::prevention::DamageFilter;
use crate::target::ChooseSpec;

/// Effect that prevents all damage to a chosen target for a duration.
#[derive(Debug, Clone, PartialEq)]
pub struct PreventAllDamageToTargetEffect {
    /// What to protect.
    pub target: ChooseSpec,
    /// Duration for the prevention shield.
    pub duration: Until,
    /// Filter for what damage this shield applies to.
    pub damage_filter: DamageFilter,
}

impl PreventAllDamageToTargetEffect {
    /// Create a new "prevent all damage to target" effect.
    pub fn new(target: ChooseSpec, duration: Until) -> Self {
        Self {
            target,
            duration,
            damage_filter: DamageFilter::all(),
        }
    }

    /// Set a damage filter for this prevention effect.
    pub fn with_filter(mut self, filter: DamageFilter) -> Self {
        self.damage_filter = filter;
        self
    }
}

impl EffectExecutor for PreventAllDamageToTargetEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        if !game.can_prevent_damage() {
            return Ok(EffectOutcome::from_result(EffectResult::Prevented));
        }

        let protected = resolve_prevention_target_from_spec(
            game,
            &self.target,
            ctx,
            PreventionTargetResolveMode::StrictSelection,
        )?;
        register_prevention_shield(
            game,
            ctx,
            protected,
            None,
            self.duration.clone(),
            self.damage_filter.clone(),
        );

        Ok(EffectOutcome::resolved())
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        Some(&self.target)
    }

    fn target_description(&self) -> &'static str {
        "target to protect from all damage"
    }
}
