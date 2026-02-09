//! Connive effect implementation.

use crate::effect::{EffectOutcome, EffectResult};
use crate::effects::DrawCardsEffect;
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_objects_from_spec;
use crate::events::cause::EventCause;
use crate::events::{KeywordActionEvent, KeywordActionKind};
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::ids::ObjectId;
use crate::target::{ChooseSpec, PlayerFilter};
use crate::triggers::TriggerEvent;
use crate::types::CardType;

/// Effect that makes target creature(s) connive.
///
/// Connive: Draw a card, then discard a card.
/// If a nonland card was discarded this way, put a +1/+1 counter on that creature.
#[derive(Debug, Clone, PartialEq)]
pub struct ConniveEffect {
    pub target: ChooseSpec,
}

impl ConniveEffect {
    pub fn new(target: ChooseSpec) -> Self {
        Self { target }
    }
}

impl EffectExecutor for ConniveEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let target_ids = resolve_objects_from_spec(game, &self.target, ctx)?;
        if target_ids.is_empty() {
            return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
        }

        let mut outcomes = Vec::new();
        for target_id in target_ids {
            let Some(target_obj) = game.object(target_id) else {
                outcomes.push(EffectOutcome::from_result(EffectResult::TargetInvalid));
                continue;
            };
            if !target_obj.has_card_type(CardType::Creature) {
                outcomes.push(EffectOutcome::from_result(EffectResult::TargetInvalid));
                continue;
            }

            let controller = target_obj.controller;
            let mut events = Vec::new();
            events.push(TriggerEvent::new(KeywordActionEvent::new(
                KeywordActionKind::Connive,
                controller,
                ctx.source,
                1,
            )));

            // Draw one card first.
            let draw_outcome =
                DrawCardsEffect::new(1, PlayerFilter::Specific(controller)).execute(game, ctx)?;
            events.extend(draw_outcome.events);

            // Then discard one card if possible.
            let hand_cards: Vec<ObjectId> = game
                .player(controller)
                .map(|p| p.hand.iter().copied().collect())
                .unwrap_or_default();

            if !hand_cards.is_empty() {
                use crate::decisions::make_decision;
                use crate::decisions::specs::ChooseObjectsSpec;
                use crate::event_processor::execute_discard;

                let spec = ChooseObjectsSpec::new(
                    ctx.source,
                    "Choose a card to discard for connive".to_string(),
                    hand_cards.clone(),
                    1,
                    Some(1),
                );
                let chosen: Vec<_> =
                    make_decision(game, ctx.decision_maker, controller, Some(ctx.source), spec);
                let selected = normalize_selection(chosen, &hand_cards, 1);
                if let Some(card_to_discard) = selected.first().copied() {
                    let discarded_nonland = game
                        .object(card_to_discard)
                        .map(|obj| !obj.has_card_type(CardType::Land))
                        .unwrap_or(false);
                    let discard_result = execute_discard(
                        game,
                        card_to_discard,
                        controller,
                        EventCause::from_effect(ctx.source, ctx.controller),
                        false,
                        &mut *ctx.decision_maker,
                    );

                    if !discard_result.prevented && discarded_nonland {
                        if let Some(event) = game.add_counters_with_source(
                            target_id,
                            crate::object::CounterType::PlusOnePlusOne,
                            1,
                            Some(ctx.source),
                            Some(ctx.controller),
                        ) {
                            events.push(event);
                        }
                    }
                }
            }

            outcomes.push(EffectOutcome::resolved().with_events(events));
        }

        Ok(EffectOutcome::aggregate(outcomes))
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        Some(&self.target)
    }

    fn target_description(&self) -> &'static str {
        "creature to connive"
    }
}

fn normalize_selection(
    chosen: Vec<ObjectId>,
    candidates: &[ObjectId],
    required: usize,
) -> Vec<ObjectId> {
    let mut selected = Vec::with_capacity(required);

    for id in chosen {
        if selected.len() == required {
            break;
        }
        if candidates.contains(&id) && !selected.contains(&id) {
            selected.push(id);
        }
    }

    if selected.len() < required {
        for &id in candidates {
            if selected.len() == required {
                break;
            }
            if !selected.contains(&id) {
                selected.push(id);
            }
        }
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::ids::{CardId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn add_card_to_hand(
        game: &mut GameState,
        owner: PlayerId,
        card_types: Vec<CardType>,
    ) -> ObjectId {
        let card = CardBuilder::new(CardId::new(), "Hand Card")
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(1)]]))
            .card_types(card_types)
            .build();
        game.create_object_from_card(&card, owner, Zone::Hand)
    }

    fn create_creature(game: &mut GameState, owner: PlayerId) -> ObjectId {
        let card = CardBuilder::new(CardId::new(), "Conniver")
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(2)]]))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(2, 2))
            .build();
        game.create_object_from_card(&card, owner, Zone::Battlefield)
    }

    #[test]
    fn connive_puts_counter_when_nonland_discarded() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let creature = create_creature(&mut game, alice);
        add_card_to_hand(&mut game, alice, vec![CardType::Instant]);

        let mut ctx = ExecutionContext::new_default(source, alice);
        let effect = ConniveEffect::new(ChooseSpec::SpecificObject(creature));
        let result = effect.execute(&mut game, &mut ctx).unwrap();
        assert!(result.result.is_success());
        assert_eq!(
            game.object(creature)
                .and_then(|obj| obj
                    .counters
                    .get(&crate::object::CounterType::PlusOnePlusOne))
                .copied()
                .unwrap_or(0),
            1
        );
    }

    #[test]
    fn connive_does_not_put_counter_when_land_discarded() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let creature = create_creature(&mut game, alice);
        add_card_to_hand(&mut game, alice, vec![CardType::Land]);

        let mut ctx = ExecutionContext::new_default(source, alice);
        let effect = ConniveEffect::new(ChooseSpec::SpecificObject(creature));
        let result = effect.execute(&mut game, &mut ctx).unwrap();
        assert!(result.result.is_success());
        assert_eq!(
            game.object(creature)
                .and_then(|obj| obj
                    .counters
                    .get(&crate::object::CounterType::PlusOnePlusOne))
                .copied()
                .unwrap_or(0),
            0
        );
    }
}
