//! Exile cards from the top of a library until one matches a filter, then offer
//! that card to be cast and put the rest on the bottom in random order.

use crate::alternative_cast::CastingMethod;
use crate::cost::OptionalCostsPaid;
use crate::effect::EffectOutcome;
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_player_filter;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::{GameState, StackEntry};
use crate::target::{ObjectFilter, PlayerFilter};
use crate::zone::Zone;

use super::runtime_helpers::with_spell_cast_event;

#[derive(Debug, Clone, PartialEq)]
pub struct ExileUntilMatchCastEffect {
    pub player: PlayerFilter,
    pub filter: ObjectFilter,
    pub caster: PlayerFilter,
    pub without_paying_mana_cost: bool,
}

impl ExileUntilMatchCastEffect {
    pub fn new(
        player: PlayerFilter,
        filter: ObjectFilter,
        caster: PlayerFilter,
        without_paying_mana_cost: bool,
    ) -> Self {
        Self {
            player,
            filter,
            caster,
            without_paying_mana_cost,
        }
    }
}

impl EffectExecutor for ExileUntilMatchCastEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let player_id = resolve_player_filter(game, &self.player, ctx)?;
        let caster_id = resolve_player_filter(game, &self.caster, ctx)?;

        let mut exiled = Vec::new();
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
            exiled.push(exiled_id);

            let filter_ctx = ctx.filter_context(game);
            let Some(card) = game.object(exiled_id) else {
                continue;
            };
            if self.filter.matches(card, &filter_ctx, game) {
                candidate = Some(exiled_id);
                break;
            }
        }

        let mut casted_card = None;
        if let Some(candidate_id) = candidate {
            let Some(candidate_obj) = game.object(candidate_id) else {
                return Ok(EffectOutcome::count(0));
            };

            let candidate_name = candidate_obj.name.clone();
            let prompt = if self.without_paying_mana_cost {
                format!("Cast {candidate_name} without paying its mana cost?")
            } else {
                format!("Cast {candidate_name}?")
            };
            let choice_ctx = crate::decisions::context::BooleanContext::new(
                caster_id,
                Some(candidate_id),
                prompt,
            );
            let should_cast = ctx.decision_maker.decide_boolean(game, &choice_ctx);

            if should_cast {
                let from_zone = candidate_obj.zone;
                let mana_cost = candidate_obj.mana_cost.clone();
                let stable_id = candidate_obj.stable_id;
                let x_value = mana_cost
                    .as_ref()
                    .and_then(|cost| if cost.has_x() { Some(0u32) } else { None });

                if let Some(new_id) = game.move_object(candidate_id, Zone::Stack) {
                    if let Some(obj) = game.object_mut(new_id) {
                        obj.x_value = x_value;
                    }

                    let stack_entry = StackEntry {
                        object_id: new_id,
                        controller: caster_id,
                        targets: vec![],
                        target_assignments: vec![],
                        x_value,
                        ability_effects: None,
                        is_ability: false,
                        casting_method: CastingMethod::PlayFrom {
                            source: ctx.source,
                            zone: from_zone,
                            use_alternative: None,
                        },
                        optional_costs_paid: OptionalCostsPaid::default(),
                        defending_player: None,
                        saga_final_chapter_source: None,
                        source_stable_id: Some(stable_id),
                        source_snapshot: None,
                        source_name: Some(candidate_name),
                        triggering_event: None,
                        intervening_if: None,
                        keyword_payment_contributions: vec![],
                        crew_contributors: vec![],
                        saddle_contributors: vec![],
                        chosen_modes: None,
                        tagged_objects: std::collections::HashMap::new(),
                    };
                    game.push_to_stack(stack_entry);
                    casted_card = Some((candidate_id, new_id, from_zone));
                }
            }
        }

        let mut to_bottom = exiled;
        if let Some((casted_from_exile, _, _)) = casted_card {
            to_bottom.retain(|id| *id != casted_from_exile);
        }
        game.shuffle_slice(&mut to_bottom);

        for exiled_id in to_bottom {
            if let Some((new_id, final_zone)) = game.move_object_with_commander_options(
                exiled_id,
                Zone::Library,
                &mut *ctx.decision_maker,
            ) {
                if final_zone != Zone::Library {
                    continue;
                }
                let owner = game.object(new_id).map(|obj| obj.owner);
                if let Some(owner) = owner
                    && let Some(player) = game.player_mut(owner)
                    && let Some(pos) = player.library.iter().position(|id| *id == new_id)
                {
                    player.library.remove(pos);
                    player.library.insert(0, new_id);
                }
            }
        }

        if let Some((_, casted_id, from_zone)) = casted_card {
            Ok(with_spell_cast_event(
                EffectOutcome::with_objects(vec![casted_id]),
                game,
                casted_id,
                caster_id,
                from_zone,
                ctx.provenance,
            ))
        } else {
            Ok(EffectOutcome::count(0))
        }
    }
}
