use crate::decision::FallbackStrategy;
use crate::decisions::{SearchSpec, make_decision_with_fallback};
use crate::effect::EffectOutcome;
use crate::effects::EffectExecutor;
use crate::events::SearchLibraryEvent;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::ids::ObjectId;
use crate::triggers::TriggerEvent;
use crate::types::{CardType, Subtype, Supertype};

#[derive(Debug, Clone, PartialEq)]
pub struct YasharnImplacableEarthEffect;

impl YasharnImplacableEarthEffect {
    pub fn new() -> Self {
        Self
    }

    fn is_matching_basic_land(object: &crate::object::Object, subtype: Subtype) -> bool {
        object.card_types.contains(&CardType::Land)
            && object.supertypes.contains(&Supertype::Basic)
            && object.subtypes.contains(&subtype)
    }

    fn choose_matching_card(
        &self,
        game: &GameState,
        ctx: &mut ExecutionContext,
        subtype: Subtype,
    ) -> Option<ObjectId> {
        let matching_cards: Vec<ObjectId> = game
            .player(ctx.controller)
            .map(|player| {
                player
                    .library
                    .iter()
                    .filter_map(|&id| game.object(id).map(|object| (id, object)))
                    .filter(|(_, object)| Self::is_matching_basic_land(object, subtype))
                    .map(|(id, _)| id)
                    .collect()
            })
            .unwrap_or_default();

        if matching_cards.is_empty() {
            return None;
        }

        let spec = SearchSpec::new(ctx.source, matching_cards, true);
        make_decision_with_fallback(
            game,
            &mut ctx.decision_maker,
            ctx.controller,
            Some(ctx.source),
            spec,
            FallbackStrategy::FirstOption,
        )
    }
}

impl EffectExecutor for YasharnImplacableEarthEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        if !game.can_search_library(ctx.controller) {
            return Ok(EffectOutcome::prevented());
        }

        let search_event = TriggerEvent::new_with_provenance(
            SearchLibraryEvent::new(ctx.controller, Some(ctx.controller)),
            ctx.provenance,
        );

        let mut moved = Vec::new();
        for subtype in [Subtype::Forest, Subtype::Plains] {
            let Some(card_id) = self.choose_matching_card(game, ctx, subtype) else {
                continue;
            };

            let still_in_library = game
                .player(ctx.controller)
                .is_some_and(|player| player.library.contains(&card_id));
            if !still_in_library {
                continue;
            }

            if let Some(new_id) = game.move_object_by_effect(card_id, crate::zone::Zone::Hand) {
                moved.push(new_id);
            }
        }

        game.shuffle_player_library(ctx.controller);

        let outcome = if moved.is_empty() {
            EffectOutcome::count(0)
        } else {
            EffectOutcome::with_objects(moved)
        };
        Ok(outcome.with_event(search_event))
    }
}
