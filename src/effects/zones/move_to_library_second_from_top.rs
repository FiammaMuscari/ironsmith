//! Move an object to second from top of its owner's library.

use crate::effect::{EffectOutcome, EffectResult};
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_objects_from_spec;
use crate::event_processor::{EventOutcome, process_zone_change};
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::target::ChooseSpec;
use crate::zone::Zone;

/// "Put target [object] into its owner's library second from the top."
#[derive(Debug, Clone, PartialEq)]
pub struct MoveToLibrarySecondFromTopEffect {
    pub target: ChooseSpec,
}

impl MoveToLibrarySecondFromTopEffect {
    pub fn new(target: ChooseSpec) -> Self {
        Self { target }
    }
}

impl EffectExecutor for MoveToLibrarySecondFromTopEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let object_ids = resolve_objects_from_spec(game, &self.target, ctx)?;
        if object_ids.is_empty() {
            return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
        }

        let mut moved_ids = Vec::new();
        let mut any_replaced = false;

        for object_id in object_ids {
            let Some(obj) = game.object(object_id) else {
                continue;
            };
            let from_zone = obj.zone;

            let result = process_zone_change(
                game,
                object_id,
                from_zone,
                Zone::Library,
                &mut ctx.decision_maker,
            );

            match result {
                EventOutcome::Prevented => {
                    return Ok(EffectOutcome::from_result(EffectResult::Prevented));
                }
                EventOutcome::Proceed(final_zone) => {
                    if let Some(new_id) = game.move_object(object_id, final_zone) {
                        if final_zone == Zone::Exile {
                            game.add_exiled_with_source_link(ctx.source, new_id);
                        } else if final_zone == Zone::Library
                            && let Some(owner) = game.object(new_id).map(|o| o.owner)
                            && let Some(player) = game.player_mut(owner)
                            && let Some(pos) = player.library.iter().position(|id| *id == new_id)
                        {
                            player.library.remove(pos);
                            let insert_pos = player.library.len().saturating_sub(1);
                            player.library.insert(insert_pos, new_id);
                        }
                        moved_ids.push(new_id);
                    }
                }
                EventOutcome::Replaced => {
                    any_replaced = true;
                }
                EventOutcome::NotApplicable => {}
            }
        }

        if !moved_ids.is_empty() {
            return Ok(EffectOutcome::from_result(EffectResult::Objects(moved_ids)));
        }
        if any_replaced {
            return Ok(EffectOutcome::from_result(EffectResult::Replaced));
        }
        Ok(EffectOutcome::from_result(EffectResult::TargetInvalid))
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        Some(&self.target)
    }

    fn target_description(&self) -> &'static str {
        "target to move second from top"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::executor::ResolvedTarget;
    use crate::ids::{CardId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::types::CardType;

    #[test]
    fn moves_target_to_second_from_top_of_library() {
        let mut game = GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20);
        let alice = PlayerId::from_index(0);

        let below_top_card = CardBuilder::new(CardId::from_raw(2), "Below Top Card")
            .card_types(vec![CardType::Instant])
            .build();
        game.create_object_from_card(&below_top_card, alice, Zone::Library);

        let top_card = CardBuilder::new(CardId::from_raw(1), "Top Card")
            .card_types(vec![CardType::Sorcery])
            .build();
        let top_id = game.create_object_from_card(&top_card, alice, Zone::Library);

        let creature = CardBuilder::new(CardId::from_raw(3), "Test Creature")
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Green]]))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(2, 2))
            .build();
        let creature_id = game.create_object_from_card(&creature, alice, Zone::Battlefield);

        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice)
            .with_targets(vec![ResolvedTarget::Object(creature_id)]);
        let effect = MoveToLibrarySecondFromTopEffect::new(ChooseSpec::Object(
            crate::target::ObjectFilter::specific(creature_id),
        ));

        let outcome = effect.execute(&mut game, &mut ctx).expect("execute effect");
        assert!(
            matches!(outcome.result, EffectResult::Objects(_)),
            "expected moved object result, got {:?}",
            outcome.result
        );

        let library = &game.player(alice).expect("alice exists").library;
        assert!(
            library.len() >= 3,
            "expected three cards in library after move, got {library:?}"
        );
        let top = *library.last().expect("library has top card");
        let second_from_top = library[library.len() - 2];

        assert_eq!(top, top_id, "original top card should remain on top");
        assert!(
            game.object(second_from_top)
                .is_some_and(|obj| obj.name == "Test Creature"),
            "moved creature should be second from top, got {second_from_top:?}"
        );
    }
}
