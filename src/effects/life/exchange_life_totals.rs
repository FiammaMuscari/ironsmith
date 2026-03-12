//! Exchange life totals effect implementation.

use crate::effect::EffectOutcome;
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_player_filter;
use crate::event_processor::process_life_gain_with_event;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::target::PlayerFilter;
use crate::triggers::TriggerEvent;

/// Effect that exchanges life totals between two players.
///
/// Used by cards like "Exchange life totals with target player."
/// Both players' life totals are simultaneously set to what
/// the other player's life total was.
///
/// # Fields
///
/// * `player1` - First player in the exchange (usually the controller)
/// * `player2` - Second player in the exchange (usually target opponent)
///
/// # Example
///
/// ```ignore
/// // Exchange life totals with target player
/// let effect = ExchangeLifeTotalsEffect::with_target();
///
/// // Or with a specific opponent
/// let effect = ExchangeLifeTotalsEffect::with_opponent();
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeLifeTotalsEffect {
    /// First player in the exchange.
    pub player1: PlayerFilter,
    /// Second player in the exchange.
    pub player2: PlayerFilter,
}

impl ExchangeLifeTotalsEffect {
    /// Create a new exchange life totals effect.
    pub fn new(player1: PlayerFilter, player2: PlayerFilter) -> Self {
        Self { player1, player2 }
    }

    /// Create an effect that exchanges life totals with target opponent.
    pub fn with_opponent() -> Self {
        Self::new(PlayerFilter::You, PlayerFilter::Opponent)
    }

    /// Create an effect that exchanges life totals with target player.
    pub fn with_target() -> Self {
        Self::new(
            PlayerFilter::You,
            PlayerFilter::Target(Box::new(PlayerFilter::Any)),
        )
    }
}

impl EffectExecutor for ExchangeLifeTotalsEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let player1_id = resolve_player_filter(game, &self.player1, ctx)?;
        let player2_id = resolve_player_filter(game, &self.player2, ctx)?;

        let life1 = game.player(player1_id).map(|p| p.life).unwrap_or(0);
        let life2 = game.player(player2_id).map(|p| p.life).unwrap_or(0);

        // Check if life totals can change
        if !game.can_change_life_total(player1_id) || !game.can_change_life_total(player2_id) {
            return Ok(EffectOutcome::prevented());
        }

        let mut outcome = EffectOutcome::resolved();

        if life2 > life1 {
            let gained = process_life_gain_with_event(game, player1_id, (life2 - life1) as u32);
            if gained > 0
                && let Some(player) = game.player_mut(player1_id)
            {
                player.gain_life(gained);
            }
            if gained > 0 {
                outcome = outcome.with_event(TriggerEvent::new_with_provenance(
                    crate::events::LifeGainEvent::new(player1_id, gained),
                    ctx.provenance,
                ));
            }
        } else if life1 > life2 {
            let lost = (life1 - life2) as u32;
            if let Some(player) = game.player_mut(player1_id) {
                player.lose_life(lost);
            }
            if lost > 0 {
                outcome = outcome.with_event(TriggerEvent::new_with_provenance(
                    crate::events::LifeLossEvent::from_effect(player1_id, lost),
                    ctx.provenance,
                ));
            }
        }

        if life1 > life2 {
            let gained = process_life_gain_with_event(game, player2_id, (life1 - life2) as u32);
            if gained > 0
                && let Some(player) = game.player_mut(player2_id)
            {
                player.gain_life(gained);
            }
            if gained > 0 {
                outcome = outcome.with_event(TriggerEvent::new_with_provenance(
                    crate::events::LifeGainEvent::new(player2_id, gained),
                    ctx.provenance,
                ));
            }
        } else if life2 > life1 {
            let lost = (life2 - life1) as u32;
            if let Some(player) = game.player_mut(player2_id) {
                player.lose_life(lost);
            }
            if lost > 0 {
                outcome = outcome.with_event(TriggerEvent::new_with_provenance(
                    crate::events::LifeLossEvent::from_effect(player2_id, lost),
                    ctx.provenance,
                ));
            }
        }

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventKind;
    use crate::executor::ResolvedTarget;
    use crate::ids::PlayerId;

    #[test]
    fn exchange_life_totals_emits_gain_and_loss_events() {
        let mut game = crate::tests::test_helpers::setup_two_player_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);
        game.player_mut(alice).expect("alice exists").life = 10;
        game.player_mut(bob).expect("bob exists").life = 20;

        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice)
            .with_targets(vec![ResolvedTarget::Player(bob)]);
        let outcome = ExchangeLifeTotalsEffect::with_target()
            .execute(&mut game, &mut ctx)
            .expect("exchange should resolve");

        assert_eq!(game.player(alice).expect("alice exists").life, 20);
        assert_eq!(game.player(bob).expect("bob exists").life, 10);
        assert!(
            outcome
                .events
                .iter()
                .any(|event| event.kind() == EventKind::LifeGain),
            "exchanging life totals should emit at least one LifeGainEvent"
        );
        assert!(
            outcome
                .events
                .iter()
                .any(|event| event.kind() == EventKind::LifeLoss),
            "exchanging life totals should emit at least one LifeLossEvent"
        );
    }
}
