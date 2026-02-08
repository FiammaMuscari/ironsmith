//! "Unless [player does action]" effect implementation.

use crate::decision::FallbackStrategy;
use crate::decisions::make_boolean_decision;
use crate::effect::{Effect, EffectOutcome, EffectResult};
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_player_filter;
use crate::executor::{ExecutionContext, ExecutionError, execute_effect};
use crate::game_state::GameState;
use crate::target::PlayerFilter;

/// Effect that executes main effects unless a player performs an alternative action.
///
/// "Sacrifice this creature unless you sacrifice another creature" — the player
/// can choose to perform the alternative action to prevent the main effects.
///
/// # Fields
///
/// * `effects` - The effects to execute if the player does NOT perform the alternative
/// * `alternative` - The alternative action the player can choose to perform
/// * `player` - Which player chooses whether to perform the alternative
///
/// # Result
///
/// - If player performs alternative: result of alternative effects
/// - If player declines: result of executing main effects
#[derive(Debug, Clone, PartialEq)]
pub struct UnlessActionEffect {
    /// The effects to execute if the player does not perform the alternative.
    pub effects: Vec<Effect>,
    /// The alternative action to prevent the main effects.
    pub alternative: Vec<Effect>,
    /// Which player chooses.
    pub player: PlayerFilter,
}

impl UnlessActionEffect {
    /// Create a new "unless action" effect.
    pub fn new(effects: Vec<Effect>, alternative: Vec<Effect>, player: PlayerFilter) -> Self {
        Self {
            effects,
            alternative,
            player,
        }
    }
}

impl EffectExecutor for UnlessActionEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let deciding_player = resolve_player_filter(game, &self.player, ctx)?;

        // Ask the player if they want to perform the alternative action
        let wants_alternative = make_boolean_decision(
            game,
            &mut ctx.decision_maker,
            deciding_player,
            ctx.source,
            "Perform alternative action to prevent effect?".to_string(),
            FallbackStrategy::Accept,
        );

        if wants_alternative {
            // Execute alternative effects
            let mut outcomes = Vec::new();
            for effect in &self.alternative {
                outcomes.push(execute_effect(game, effect, ctx)?);
            }
            Ok(EffectOutcome::aggregate(outcomes))
        } else {
            // Execute main effects (the "penalty")
            let mut outcomes = Vec::new();
            for effect in &self.effects {
                outcomes.push(execute_effect(game, effect, ctx)?);
            }
            Ok(EffectOutcome::aggregate(outcomes))
        }
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }
}
