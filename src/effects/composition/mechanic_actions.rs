//! Explicit mechanic effects used by parser/rendering for supported wording.
//!
//! These mechanics are represented as first-class effects so parser output does
//! not depend on raw oracle text passthrough for rendering.

use crate::decisions::make_decision;
use crate::decisions::specs::ChooseObjectsSpec;
use crate::effect::{EffectOutcome, ExecutionFact, Until};
use crate::effects::EffectExecutor;
use crate::effects::helpers::normalize_object_selection;
use crate::effects::player::CastTaggedEffect;
use crate::effects::zones::apply_zone_change;
use crate::event_processor::EventOutcome;
use crate::events::permanents::SacrificeEvent;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::ids::{ObjectId, StableId};
use crate::object::CounterType;
use crate::snapshot::ObjectSnapshot;
use crate::target::ChooseSpec;
use crate::triggers::TriggerEvent;
use crate::zone::Zone;

#[derive(Debug, Clone, PartialEq)]
pub struct BackupEffect {
    pub amount: u32,
    pub granted_abilities: Vec<crate::ability::Ability>,
}

impl BackupEffect {
    pub fn new(amount: u32, granted_abilities: Vec<crate::ability::Ability>) -> Self {
        Self {
            amount,
            granted_abilities,
        }
    }
}

impl EffectExecutor for BackupEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let target = crate::effects::helpers::resolve_single_object_from_spec(
            game,
            &ChooseSpec::target_creature(),
            ctx,
        )?;

        let mut outcomes = vec![
            crate::effects::PutCountersEffect::new(
                CounterType::PlusOnePlusOne,
                self.amount,
                ChooseSpec::SpecificObject(target),
            )
            .execute(game, ctx)?,
        ];

        if target != ctx.source {
            for ability in &self.granted_abilities {
                let granted = match &ability.kind {
                    crate::ability::AbilityKind::Static(static_ability) => static_ability.clone(),
                    _ => crate::static_abilities::StaticAbility::grant_object_ability_for_filter(
                        crate::target::ObjectFilter::source(),
                        ability.clone(),
                        ability.text.clone().unwrap_or_default(),
                    ),
                };
                outcomes.push(
                    crate::effects::ApplyContinuousEffect::new(
                        crate::continuous::EffectTarget::Specific(target),
                        crate::continuous::Modification::AddAbility(granted),
                        Until::EndOfTurn,
                    )
                    .execute(game, ctx)?,
                );
            }
        }

        Ok(EffectOutcome::aggregate(outcomes))
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        static TARGET: std::sync::OnceLock<ChooseSpec> = std::sync::OnceLock::new();
        Some(TARGET.get_or_init(ChooseSpec::target_creature))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExploreEffect {
    pub target: ChooseSpec,
}

impl ExploreEffect {
    pub fn new(target: ChooseSpec) -> Self {
        Self { target }
    }
}

impl EffectExecutor for ExploreEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        _game: &mut GameState,
        _ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        // Runtime explore behavior is handled separately; this preserves
        // parser/render semantics without oracle-text fallback.
        Ok(EffectOutcome::resolved())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAttractionEffect;

impl OpenAttractionEffect {
    pub fn new() -> Self {
        Self
    }
}

impl EffectExecutor for OpenAttractionEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        _game: &mut GameState,
        _ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        Ok(EffectOutcome::resolved())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManifestDreadEffect;

impl ManifestDreadEffect {
    pub fn new() -> Self {
        Self
    }
}

impl EffectExecutor for ManifestDreadEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        _game: &mut GameState,
        _ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        Ok(EffectOutcome::resolved())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BolsterEffect {
    pub amount: u32,
}

impl BolsterEffect {
    pub fn new(amount: u32) -> Self {
        Self { amount }
    }
}

impl EffectExecutor for BolsterEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let mut candidates = game
            .battlefield
            .iter()
            .copied()
            .filter(|&id| {
                game.object(id).is_some_and(|obj| {
                    obj.controller == ctx.controller
                        && game.object_has_card_type(id, crate::types::CardType::Creature)
                })
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(EffectOutcome::resolved());
        }

        let least_toughness = candidates
            .iter()
            .filter_map(|&id| {
                game.calculated_toughness(id)
                    .or_else(|| game.object(id).and_then(|obj| obj.toughness()))
            })
            .min()
            .unwrap_or(0);
        candidates.retain(|&id| {
            game.calculated_toughness(id)
                .or_else(|| game.object(id).and_then(|obj| obj.toughness()))
                == Some(least_toughness)
        });
        if candidates.is_empty() {
            return Ok(EffectOutcome::resolved());
        }

        let chosen = if candidates.len() == 1 {
            candidates[0]
        } else {
            let spec = ChooseObjectsSpec::new(
                ctx.source,
                "Choose a creature with the least toughness you control for bolster",
                candidates.clone(),
                1,
                Some(1),
            );
            let selection: Vec<ObjectId> = make_decision(
                game,
                ctx.decision_maker,
                ctx.controller,
                Some(ctx.source),
                spec,
            );
            let normalized = normalize_object_selection(selection, &candidates, 1);
            normalized.first().copied().unwrap_or(candidates[0])
        };

        crate::effects::PutCountersEffect::new(
            CounterType::PlusOnePlusOne,
            self.amount,
            ChooseSpec::SpecificObject(chosen),
        )
        .execute(game, ctx)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CipherEffect;

impl CipherEffect {
    pub fn new() -> Self {
        Self
    }
}

impl EffectExecutor for CipherEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let Some(source_obj) = game.object(ctx.source).cloned() else {
            return Ok(EffectOutcome::target_invalid());
        };
        if source_obj.zone != Zone::Stack || source_obj.card.is_none() {
            return Ok(EffectOutcome::resolved());
        }

        let candidates = game
            .battlefield
            .iter()
            .copied()
            .filter(|&id| {
                game.object(id).is_some_and(|obj| {
                    obj.controller == ctx.controller
                        && game.object_has_card_type(id, crate::types::CardType::Creature)
                })
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(EffectOutcome::resolved());
        }

        let choice_ctx = crate::decisions::context::BooleanContext::new(
            ctx.controller,
            Some(ctx.source),
            format!(
                "Exile {} encoded on a creature you control?",
                source_obj.name
            ),
        );
        if !ctx.decision_maker.decide_boolean(game, &choice_ctx) {
            return Ok(EffectOutcome::declined());
        }

        let chosen_creature = if candidates.len() == 1 {
            candidates[0]
        } else {
            let spec = ChooseObjectsSpec::new(
                ctx.source,
                "Choose a creature you control to encode",
                candidates.clone(),
                1,
                Some(1),
            );
            let selection: Vec<ObjectId> = make_decision(
                game,
                ctx.decision_maker,
                ctx.controller,
                Some(ctx.source),
                spec,
            );
            let normalized = normalize_object_selection(selection, &candidates, 1);
            let Some(chosen) = normalized.first().copied() else {
                return Ok(EffectOutcome::declined());
            };
            chosen
        };

        let exiled_id = match apply_zone_change(
            game,
            ctx.source,
            source_obj.zone,
            Zone::Exile,
            ctx.cause.clone(),
            &mut *ctx.decision_maker,
        ) {
            EventOutcome::Proceed(result) => {
                let Some(new_id) = result.new_object_id else {
                    return Ok(EffectOutcome::resolved());
                };
                if result.final_zone != Zone::Exile {
                    return Ok(EffectOutcome::resolved());
                }
                new_id
            }
            EventOutcome::Prevented => return Ok(EffectOutcome::prevented()),
            EventOutcome::Replaced => return Ok(EffectOutcome::replaced()),
            EventOutcome::NotApplicable => return Ok(EffectOutcome::target_invalid()),
        };

        let Some(exiled_stable_id) = game.object(exiled_id).map(|obj| obj.stable_id) else {
            return Ok(EffectOutcome::target_invalid());
        };

        game.imprint_card(chosen_creature, exiled_id);
        let trigger_text = "Whenever this creature deals combat damage to a player, its controller may cast a copy of the encoded card without paying its mana cost.";
        let ability = crate::ability::Ability::triggered(
            crate::triggers::Trigger::this_deals_combat_damage_to_player(),
            vec![crate::effect::Effect::cast_encoded_card_copy(
                exiled_stable_id,
            )],
        )
        .with_text(trigger_text);
        if let Some(creature) = game.object_mut(chosen_creature) {
            creature.abilities.push(ability);
        }

        Ok(
            EffectOutcome::with_objects(vec![exiled_id, chosen_creature])
                .with_execution_fact(ExecutionFact::ChosenObjects(vec![chosen_creature]))
                .with_execution_fact(ExecutionFact::AffectedObjects(vec![exiled_id])),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CastEncodedCardCopyEffect {
    pub encoded_card: StableId,
}

impl CastEncodedCardCopyEffect {
    pub fn new(encoded_card: StableId) -> Self {
        Self { encoded_card }
    }
}

impl EffectExecutor for CastEncodedCardCopyEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let Some(encoded_id) = game.find_object_by_stable_id(self.encoded_card) else {
            return Ok(EffectOutcome::target_invalid());
        };
        let Some(encoded_obj) = game.object(encoded_id).cloned() else {
            return Ok(EffectOutcome::target_invalid());
        };
        if encoded_obj.zone != Zone::Exile {
            return Ok(EffectOutcome::target_invalid());
        }

        let choice_ctx = crate::decisions::context::BooleanContext::new(
            ctx.controller,
            Some(ctx.source),
            format!(
                "Cast a copy of {} without paying its mana cost?",
                encoded_obj.name
            ),
        );
        if !ctx.decision_maker.decide_boolean(game, &choice_ctx) {
            return Ok(EffectOutcome::declined());
        }

        let snapshot = ObjectSnapshot::from_object(&encoded_obj, game);
        let prior = ctx.clear_object_tag("cipher_encoded");
        ctx.set_tagged_objects("cipher_encoded", vec![snapshot]);
        let result = CastTaggedEffect::new("cipher_encoded")
            .as_copy()
            .without_paying_mana_cost()
            .execute(game, ctx);
        if let Some(previous) = prior {
            ctx.set_tagged_objects("cipher_encoded", previous);
        } else {
            ctx.clear_object_tag("cipher_encoded");
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DevourEffect {
    pub multiplier: u32,
}

impl DevourEffect {
    pub fn new(multiplier: u32) -> Self {
        Self { multiplier }
    }
}

impl EffectExecutor for DevourEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        if !game
            .object(ctx.source)
            .is_some_and(|obj| obj.zone == Zone::Battlefield)
        {
            return Ok(EffectOutcome::resolved());
        }

        let candidates = game
            .battlefield
            .iter()
            .copied()
            .filter(|&id| id != ctx.source)
            .filter(|&id| {
                game.object(id).is_some_and(|obj| {
                    obj.controller == ctx.controller
                        && game.object_has_card_type(id, crate::types::CardType::Creature)
                        && game.can_be_sacrificed(id)
                })
            })
            .collect::<Vec<_>>();

        let chosen = if candidates.is_empty() {
            Vec::new()
        } else {
            let spec = ChooseObjectsSpec::new(
                ctx.source,
                "Choose any number of other creatures you control to sacrifice for devour",
                candidates.clone(),
                0,
                Some(candidates.len()),
            );
            let selection: Vec<ObjectId> = make_decision(
                game,
                ctx.decision_maker,
                ctx.controller,
                Some(ctx.source),
                spec,
            );
            selection
                .into_iter()
                .filter(|id| candidates.contains(id))
                .fold(Vec::new(), |mut chosen, id| {
                    if !chosen.contains(&id) {
                        chosen.push(id);
                    }
                    chosen
                })
        };

        let mut sacrificed_count: i32 = 0;
        let mut sacrifice_events = Vec::new();
        for id in chosen {
            let pre_snapshot = game
                .object(id)
                .map(|obj| ObjectSnapshot::from_object(obj, game));
            let sacrificing_player = pre_snapshot.as_ref().map(|snapshot| snapshot.controller);

            match apply_zone_change(
                game,
                id,
                Zone::Battlefield,
                Zone::Graveyard,
                ctx.cause.clone(),
                &mut *ctx.decision_maker,
            ) {
                EventOutcome::Prevented | EventOutcome::NotApplicable => {}
                EventOutcome::Proceed(result) => {
                    sacrificed_count += 1;
                    if result.final_zone == Zone::Graveyard {
                        sacrifice_events.push(TriggerEvent::new_with_provenance(
                            SacrificeEvent::new(id, Some(ctx.source))
                                .with_snapshot(pre_snapshot, sacrificing_player),
                            ctx.provenance,
                        ));
                    }
                }
                EventOutcome::Replaced => {
                    sacrificed_count += 1;
                }
            }
        }

        if sacrificed_count == 0 {
            return Ok(EffectOutcome::count(0).with_events(sacrifice_events));
        }

        let mut counters = crate::effects::PutCountersEffect::new(
            CounterType::PlusOnePlusOne,
            sacrificed_count.saturating_mul(self.multiplier as i32),
            ChooseSpec::Source,
        )
        .execute(game, ctx)?;
        counters.events.extend(sacrifice_events);
        Ok(counters)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportEffect {
    pub amount: u32,
}

impl SupportEffect {
    pub fn new(amount: u32) -> Self {
        Self { amount }
    }
}

impl EffectExecutor for SupportEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        _game: &mut GameState,
        _ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        Ok(EffectOutcome::resolved())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdaptEffect {
    pub amount: u32,
}

impl AdaptEffect {
    pub fn new(amount: u32) -> Self {
        Self { amount }
    }
}

impl EffectExecutor for AdaptEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        _game: &mut GameState,
        _ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        Ok(EffectOutcome::resolved())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CounterAbilityEffect;

impl CounterAbilityEffect {
    pub fn new() -> Self {
        Self
    }
}

impl EffectExecutor for CounterAbilityEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        _game: &mut GameState,
        _ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        Ok(EffectOutcome::resolved())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::Ability;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::decision::DecisionMaker;
    use crate::decisions::context::SelectObjectsContext;
    use crate::executor::ExecutionContext;
    use crate::ids::{CardId, PlayerId};
    use crate::static_abilities::StaticAbility;
    use crate::static_abilities::StaticAbilityId;
    use crate::types::CardType;

    fn setup_game() -> GameState {
        crate::tests::test_helpers::setup_two_player_game()
    }

    fn create_creature(
        game: &mut GameState,
        controller: PlayerId,
        card_id: u32,
        name: &str,
        power: i32,
        toughness: i32,
    ) -> ObjectId {
        let card = CardBuilder::new(CardId::from_raw(card_id), name)
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(power, toughness))
            .build();
        game.create_object_from_card(&card, controller, Zone::Battlefield)
    }

    struct SelectIdsDecisionMaker {
        chosen: Vec<ObjectId>,
    }

    impl DecisionMaker for SelectIdsDecisionMaker {
        fn decide_objects(
            &mut self,
            _game: &GameState,
            ctx: &SelectObjectsContext,
        ) -> Vec<ObjectId> {
            self.chosen
                .iter()
                .copied()
                .filter(|id| {
                    ctx.candidates
                        .iter()
                        .any(|candidate| candidate.legal && candidate.id == *id)
                })
                .collect()
        }
    }

    #[test]
    fn bolster_chooses_among_least_toughness_creatures() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let first = create_creature(&mut game, alice, 1, "First", 1, 1);
        let second = create_creature(&mut game, alice, 2, "Second", 1, 1);
        let _largest = create_creature(&mut game, alice, 3, "Largest", 4, 4);
        let mut dm = SelectIdsDecisionMaker {
            chosen: vec![second],
        };
        let mut ctx = ExecutionContext::new_default(source, alice).with_decision_maker(&mut dm);

        let outcome = BolsterEffect::new(2)
            .execute(&mut game, &mut ctx)
            .expect("execute bolster");

        assert_eq!(outcome.value, crate::effect::OutcomeValue::Count(2));
        assert_eq!(game.counter_count(first, CounterType::PlusOnePlusOne), 0);
        assert_eq!(game.counter_count(second, CounterType::PlusOnePlusOne), 2);
    }

    #[test]
    fn devour_sacrifices_exactly_the_chosen_creatures_and_emits_sacrifice_events() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = create_creature(&mut game, alice, 10, "Devourer", 2, 2);
        let first = create_creature(&mut game, alice, 11, "First Food", 1, 1);
        let second = create_creature(&mut game, alice, 12, "Second Food", 1, 1);
        let keep = create_creature(&mut game, alice, 13, "Keep", 3, 3);
        let mut dm = SelectIdsDecisionMaker {
            chosen: vec![second],
        };
        let mut ctx = ExecutionContext::new_default(source, alice).with_decision_maker(&mut dm);

        let outcome = DevourEffect::new(2)
            .execute(&mut game, &mut ctx)
            .expect("execute devour");

        assert!(game.battlefield.contains(&source));
        assert!(game.battlefield.contains(&first));
        assert!(!game.battlefield.contains(&second));
        assert!(game.battlefield.contains(&keep));
        assert_eq!(game.players[0].graveyard.len(), 1);
        assert_eq!(game.counter_count(source, CounterType::PlusOnePlusOne), 2);
        assert!(
            outcome
                .events_of_type::<crate::events::permanents::SacrificeEvent>()
                .count()
                == 1,
            "expected devour to emit one sacrifice event"
        );
    }

    #[test]
    fn backup_puts_counter_on_target_and_grants_following_ability_to_another_creature() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = create_creature(&mut game, alice, 20, "Backup Source", 2, 2);
        let target = create_creature(&mut game, alice, 21, "Backup Target", 1, 1);
        let granted = Ability::static_ability(StaticAbility::flying()).with_text("Flying");
        let mut ctx = ExecutionContext::new_default(source, alice)
            .with_targets(vec![crate::executor::ResolvedTarget::Object(target)]);

        let outcome = BackupEffect::new(1, vec![granted])
            .execute(&mut game, &mut ctx)
            .expect("execute backup");

        assert!(outcome.something_happened());
        assert_eq!(game.counter_count(target, CounterType::PlusOnePlusOne), 1);
        assert!(
            game.object_has_static_ability_id(target, StaticAbilityId::Flying),
            "backup target should gain the granted ability until end of turn"
        );
    }
}
