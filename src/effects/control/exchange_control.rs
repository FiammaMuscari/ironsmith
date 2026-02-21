//! Exchange control effect implementation.

use crate::continuous::{EffectTarget, Modification};
use crate::effect::{Effect, EffectOutcome, EffectResult, Until};
use crate::effects::{ApplyContinuousEffect, EffectExecutor};
use crate::executor::{ExecutionContext, ExecutionError, execute_effect};
use crate::game_state::GameState;
use crate::target::ChooseSpec;
use crate::types::CardType;
use std::collections::HashSet;

/// Effect that exchanges control of two permanents.
///
/// Creates continuous effects that swap the controllers of two permanents.
///
/// # Fields
///
/// * `permanent1` - First permanent
/// * `permanent2` - Second permanent
///
/// # Example
///
/// ```ignore
/// // Exchange control of two target creatures
/// let effect = ExchangeControlEffect::new(
///     ChooseSpec::creature(),
///     ChooseSpec::creature(),
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeControlEffect {
    /// First permanent to exchange.
    pub permanent1: ChooseSpec,
    /// Second permanent to exchange.
    pub permanent2: ChooseSpec,
    /// Optional targeting constraint that requires the two permanents to share a type.
    pub shared_type: Option<SharedTypeConstraint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedTypeConstraint {
    CardType,
    PermanentType,
}

impl ExchangeControlEffect {
    /// Create a new exchange control effect.
    pub fn new(permanent1: ChooseSpec, permanent2: ChooseSpec) -> Self {
        Self {
            permanent1,
            permanent2,
            shared_type: None,
        }
    }

    pub fn with_shared_type(mut self, constraint: SharedTypeConstraint) -> Self {
        self.shared_type = Some(constraint);
        self
    }

    /// Exchange control of two creatures.
    pub fn creatures() -> Self {
        Self::new(ChooseSpec::creature(), ChooseSpec::creature())
    }

    /// Exchange control of two permanents.
    pub fn permanents() -> Self {
        Self::new(ChooseSpec::permanent(), ChooseSpec::permanent())
    }
}

impl EffectExecutor for ExchangeControlEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let Some((perm1_id, perm2_id)) = ctx.resolve_two_object_targets() else {
            return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
        };

        if let Some(constraint) = self.shared_type {
            let Some(obj1) = game.object(perm1_id) else {
                return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
            };
            let Some(obj2) = game.object(perm2_id) else {
                return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
            };

            let relevant = |ty: CardType| -> bool {
                match constraint {
                    SharedTypeConstraint::CardType => true,
                    SharedTypeConstraint::PermanentType => matches!(
                        ty,
                        CardType::Artifact
                            | CardType::Creature
                            | CardType::Enchantment
                            | CardType::Land
                            | CardType::Planeswalker
                            | CardType::Battle
                    ),
                }
            };

            let types1: HashSet<CardType> = obj1
                .card_types
                .iter()
                .copied()
                .filter(|ty| relevant(*ty))
                .collect();
            let shares_type = obj2
                .card_types
                .iter()
                .copied()
                .filter(|ty| relevant(*ty))
                .any(|ty| types1.contains(&ty));

            if !shares_type {
                return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
            }
        }

        // Get current controllers
        let controller1 = game.object(perm1_id).map(|o| o.controller);
        let controller2 = game.object(perm2_id).map(|o| o.controller);

        if let (Some(c1), Some(c2)) = (controller1, controller2) {
            let effect1 = ApplyContinuousEffect::new(
                EffectTarget::Specific(perm1_id),
                Modification::ChangeController(c2),
                Until::Forever,
            );

            let effect2 = ApplyContinuousEffect::new(
                EffectTarget::Specific(perm2_id),
                Modification::ChangeController(c1),
                Until::Forever,
            );

            let outcomes = vec![
                execute_effect(game, &Effect::new(effect1), ctx)?,
                execute_effect(game, &Effect::new(effect2), ctx)?,
            ];

            Ok(EffectOutcome::aggregate(outcomes))
        } else {
            Ok(EffectOutcome::from_result(EffectResult::TargetInvalid))
        }
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        if self.permanent1.is_target() {
            Some(&self.permanent1)
        } else if self.permanent2.is_target() {
            Some(&self.permanent2)
        } else {
            None
        }
    }

    fn get_target_count(&self) -> Option<crate::effect::ChoiceCount> {
        self.get_target_spec().map(|spec| spec.count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::executor::ResolvedTarget;
    use crate::ids::{CardId, ObjectId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::object::Object;
    use crate::types::CardType;
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn make_creature_card(card_id: u32, name: &str) -> crate::card::Card {
        CardBuilder::new(CardId::from_raw(card_id), name)
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(2)]]))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(2, 2))
            .build()
    }

    fn create_creature(game: &mut GameState, name: &str, controller: PlayerId) -> ObjectId {
        let id = game.new_object_id();
        let card = make_creature_card(id.0 as u32, name);
        let obj = Object::from_card(id, &card, controller, Zone::Battlefield);
        game.add_object(obj);
        id
    }

    #[test]
    fn test_exchange_control() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let creature1 = create_creature(&mut game, "Alice's Creature", alice);
        let creature2 = create_creature(&mut game, "Bob's Creature", bob);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Object(creature1),
            ResolvedTarget::Object(creature2),
        ]);

        let effect = ExchangeControlEffect::creatures();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Resolved);
        // Two continuous effects should be created
        assert_eq!(game.continuous_effects.effects_sorted().len(), 2);
    }

    #[test]
    fn test_exchange_control_insufficient_targets() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        let creature1 = create_creature(&mut game, "Creature", alice);
        let source = game.new_object_id();

        // Only one target
        let mut ctx = ExecutionContext::new_default(source, alice)
            .with_targets(vec![ResolvedTarget::Object(creature1)]);

        let effect = ExchangeControlEffect::creatures();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::TargetInvalid);
    }

    #[test]
    fn test_exchange_control_invalid_first_target() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let creature2 = create_creature(&mut game, "Creature", bob);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Player(alice), // Invalid - should be object
            ResolvedTarget::Object(creature2),
        ]);

        let effect = ExchangeControlEffect::creatures();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::TargetInvalid);
    }

    #[test]
    fn test_exchange_control_invalid_second_target() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let creature1 = create_creature(&mut game, "Creature", alice);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Object(creature1),
            ResolvedTarget::Player(bob), // Invalid - should be object
        ]);

        let effect = ExchangeControlEffect::creatures();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::TargetInvalid);
    }

    #[test]
    fn test_exchange_control_permanents() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let creature1 = create_creature(&mut game, "Permanent 1", alice);
        let creature2 = create_creature(&mut game, "Permanent 2", bob);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Object(creature1),
            ResolvedTarget::Object(creature2),
        ]);

        let effect = ExchangeControlEffect::permanents();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Resolved);
    }

    #[test]
    fn test_exchange_control_clone_box() {
        let effect = ExchangeControlEffect::creatures();
        let cloned = effect.clone_box();
        assert!(format!("{:?}", cloned).contains("ExchangeControlEffect"));
    }
}
