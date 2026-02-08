//! Pay mana effect implementation.

use crate::effect::{EffectOutcome, EffectResult};
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_player_from_spec;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::mana::ManaCost;
use crate::target::ChooseSpec;

/// Effect that asks a player to pay a mana cost.
///
/// Returns `Count(1)` when paid, `Impossible` when the player can't pay.
#[derive(Debug, Clone, PartialEq)]
pub struct PayManaEffect {
    /// Mana cost to pay.
    pub cost: ManaCost,
    /// Which player pays it.
    pub player: ChooseSpec,
}

impl PayManaEffect {
    /// Create a new pay-mana effect.
    pub fn new(cost: ManaCost, player: ChooseSpec) -> Self {
        Self { cost, player }
    }
}

impl EffectExecutor for PayManaEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let player_id = resolve_player_from_spec(game, &self.player, ctx)?;
        if game.try_pay_mana_cost(player_id, None, &self.cost, 0) {
            Ok(EffectOutcome::count(1))
        } else {
            Ok(EffectOutcome::from_result(EffectResult::Impossible))
        }
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        if self.player.is_target() {
            Some(&self.player)
        } else {
            None
        }
    }

    fn target_description(&self) -> &'static str {
        "player to pay mana"
    }
}
