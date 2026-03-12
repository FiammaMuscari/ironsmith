//! Exile cards from the top of a library until one matches a filter, then
//! grant temporary play permission for that exiled card until end of turn.

use crate::effect::EffectOutcome;
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_player_filter;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::grant::Grantable;
use crate::grant_registry::GrantSource;
use crate::target::{ObjectFilter, PlayerFilter};
use crate::zone::Zone;

#[derive(Debug, Clone, PartialEq)]
pub struct ExileUntilMatchGrantPlayEffect {
    pub player: PlayerFilter,
    pub filter: ObjectFilter,
    pub caster: PlayerFilter,
}

impl ExileUntilMatchGrantPlayEffect {
    pub fn new(player: PlayerFilter, filter: ObjectFilter, caster: PlayerFilter) -> Self {
        Self {
            player,
            filter,
            caster,
        }
    }
}

impl EffectExecutor for ExileUntilMatchGrantPlayEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let player_id = resolve_player_filter(game, &self.player, ctx)?;
        let caster_id = resolve_player_filter(game, &self.caster, ctx)?;

        let mut candidate = None;

        loop {
            let top_card = game
                .player(player_id)
                .and_then(|player| player.library.last().copied());
            let Some(top_card_id) = top_card else {
                break;
            };

            let Some(exiled_id) = game.move_object(top_card_id, Zone::Exile) else {
                break;
            };

            let filter_ctx = ctx.filter_context(game);
            let Some(card) = game.object(exiled_id) else {
                continue;
            };
            if self.filter.matches(card, &filter_ctx, game) {
                candidate = Some(exiled_id);
                break;
            }
        }

        let Some(candidate_id) = candidate else {
            return Ok(EffectOutcome::count(0));
        };

        game.grant_registry.grant_to_card(
            candidate_id,
            Zone::Exile,
            caster_id,
            Grantable::PlayFrom,
            GrantSource::Effect {
                source_id: ctx.source,
                expires_end_of_turn: game.turn.turn_number,
            },
        );

        Ok(EffectOutcome::with_objects(vec![candidate_id]))
    }
}
