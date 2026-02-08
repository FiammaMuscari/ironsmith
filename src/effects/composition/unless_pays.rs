//! "Unless pays" effect implementation.

use crate::decision::FallbackStrategy;
use crate::decisions::make_boolean_decision;
use crate::effect::{Effect, EffectOutcome, EffectResult};
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_player_filter;
use crate::executor::{ExecutionContext, ExecutionError, execute_effect};
use crate::game_state::GameState;
use crate::mana::{ManaCost, ManaSymbol};
use crate::target::PlayerFilter;

/// Effect that executes inner effects unless a player pays a mana cost.
///
/// "Sacrifice this creature unless you pay {U}" - the player can choose to pay
/// the mana to prevent the inner effects from happening.
///
/// # Fields
///
/// * `effects` - The effects to execute if the player does NOT pay
/// * `player` - Which player is asked to pay
/// * `mana` - The mana cost that must be paid to prevent the effects
///
/// # Result
///
/// - If player pays: `EffectResult::Declined` (effects prevented)
/// - If player doesn't pay: the result of executing inner effects
#[derive(Debug, Clone, PartialEq)]
pub struct UnlessPaysEffect {
    /// The effects to execute if the player does not pay.
    pub effects: Vec<Effect>,
    /// Which player is asked to pay.
    pub player: PlayerFilter,
    /// The mana cost required to prevent the effects.
    pub mana: Vec<ManaSymbol>,
}

impl UnlessPaysEffect {
    /// Create a new "unless pays" effect.
    pub fn new(effects: Vec<Effect>, player: PlayerFilter, mana: Vec<ManaSymbol>) -> Self {
        Self {
            effects,
            player,
            mana,
        }
    }
}

impl EffectExecutor for UnlessPaysEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        // Resolve who pays
        let paying_player = resolve_player_filter(game, &self.player, ctx)?;

        // Format the mana cost for display
        let mana_display: String = self
            .mana
            .iter()
            .map(|s| format!("{:?}", s))
            .collect::<Vec<_>>()
            .join(", ");

        // Check if player can afford to pay
        let can_afford = {
            let cost = ManaCost::from_symbols(self.mana.clone());
            game.can_pay_mana_cost(paying_player, None, &cost, 0)
        };

        // Ask the player if they want to pay
        let wants_to_pay = if can_afford {
            make_boolean_decision(
                game,
                &mut ctx.decision_maker,
                paying_player,
                ctx.source,
                format!("Pay {} to prevent effect?", mana_display),
                FallbackStrategy::Accept,
            )
        } else {
            false
        };

        if wants_to_pay {
            // Pay the mana cost
            let cost = ManaCost::from_symbols(self.mana.clone());
            if game.try_pay_mana_cost(paying_player, None, &cost, 0) {
                // Payment successful, effects are prevented
                return Ok(EffectOutcome::from_result(EffectResult::Declined));
            }
        }

        // Player didn't pay (or couldn't), execute the inner effects
        let mut outcomes = Vec::new();
        for effect in &self.effects {
            outcomes.push(execute_effect(game, effect, ctx)?);
        }
        Ok(EffectOutcome::aggregate(outcomes))
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }
}
