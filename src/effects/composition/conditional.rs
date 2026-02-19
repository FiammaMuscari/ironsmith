//! Conditional effect implementation.

use crate::effect::{Condition, Effect, EffectOutcome};
use crate::effects::{EffectExecutor, ModalSpec};
use crate::executor::{ExecutionContext, ExecutionError, execute_effect};
use crate::game_state::GameState;
use crate::ids::{ObjectId, PlayerId};

/// Effect that branches based on game state conditions.
///
/// Unlike `If` which checks the result of a prior effect, `Conditional`
/// evaluates game state conditions like "if you control a creature" or
/// "if your life total is 10 or less".
///
/// # Fields
///
/// * `condition` - The game state condition to check
/// * `if_true` - Effects to execute if condition is true
/// * `if_false` - Effects to execute if condition is false
///
/// # Example
///
/// ```ignore
/// // If you control a creature, draw a card. Otherwise, gain 2 life.
/// let effect = ConditionalEffect::new(
///     Condition::YouControl(ObjectFilter::creature()),
///     vec![Effect::draw(1)],
///     vec![Effect::gain_life(2)],
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalEffect {
    /// The game state condition to check.
    pub condition: Condition,
    /// Effects to execute if condition is true.
    pub if_true: Vec<Effect>,
    /// Effects to execute if condition is false.
    pub if_false: Vec<Effect>,
}

impl ConditionalEffect {
    /// Create a new Conditional effect.
    pub fn new(condition: Condition, if_true: Vec<Effect>, if_false: Vec<Effect>) -> Self {
        Self {
            condition,
            if_true,
            if_false,
        }
    }

    /// Create a conditional with no else clause.
    pub fn if_only(condition: Condition, if_true: Vec<Effect>) -> Self {
        Self::new(condition, if_true, vec![])
    }
}

impl EffectExecutor for ConditionalEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let result = evaluate_condition(game, &self.condition, ctx)?;

        let effects_to_execute = if result {
            &self.if_true
        } else {
            &self.if_false
        };

        let mut outcomes = Vec::new();
        for effect in effects_to_execute {
            outcomes.push(execute_effect(game, effect, ctx)?);
        }

        Ok(EffectOutcome::aggregate(outcomes))
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_modal_spec_with_context(
        &self,
        game: &GameState,
        controller: PlayerId,
        source: ObjectId,
    ) -> Option<ModalSpec> {
        // Evaluate the condition at cast time to determine which branch to use
        let condition_result = evaluate_condition_simple(game, &self.condition, controller, source);

        // Search the appropriate branch for modal specs
        let effects_to_search = if condition_result {
            &self.if_true
        } else {
            &self.if_false
        };

        // Recursively search through the effects in this branch
        for effect in effects_to_search {
            // First try the context-aware version
            if let Some(spec) = effect
                .0
                .get_modal_spec_with_context(game, controller, source)
            {
                return Some(spec);
            }
            // Fall back to the simple version
            if let Some(spec) = effect.0.get_modal_spec() {
                return Some(spec);
            }
        }

        None
    }
}

fn evaluate_condition_simple(
    game: &GameState,
    condition: &Condition,
    controller: PlayerId,
    source: ObjectId,
) -> bool {
    super::condition_eval::evaluate_condition_cast_time(game, condition, controller, source)
}

fn evaluate_condition(
    game: &GameState,
    condition: &Condition,
    ctx: &ExecutionContext,
) -> Result<bool, ExecutionError> {
    super::condition_eval::evaluate_condition_resolution(game, condition, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::ids::{CardId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::object::Object;
    use crate::target::ObjectFilter;
    use crate::types::CardType;
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn create_creature(game: &mut GameState, name: &str, controller: PlayerId) {
        let card = CardBuilder::new(CardId::new(), name)
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(2)]]))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(2, 2))
            .build();
        let id = game.new_object_id();
        let obj = Object::from_card(id, &card, controller, Zone::Battlefield);
        game.add_object(obj);
    }

    #[test]
    fn test_conditional_you_control_true() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        create_creature(&mut game, "Bear", alice);

        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let initial_life = game.player(alice).unwrap().life;

        let effect = ConditionalEffect::new(
            Condition::YouControl(ObjectFilter::creature()),
            vec![Effect::gain_life(5)],
            vec![Effect::gain_life(1)],
        );
        effect.execute(&mut game, &mut ctx).unwrap();

        // Should have gained 5 (condition true)
        assert_eq!(game.player(alice).unwrap().life, initial_life + 5);
    }

    #[test]
    fn test_conditional_you_control_false() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        // No creatures
        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let initial_life = game.player(alice).unwrap().life;

        let effect = ConditionalEffect::new(
            Condition::YouControl(ObjectFilter::creature()),
            vec![Effect::gain_life(5)],
            vec![Effect::gain_life(1)],
        );
        effect.execute(&mut game, &mut ctx).unwrap();

        // Should have gained 1 (condition false)
        assert_eq!(game.player(alice).unwrap().life, initial_life + 1);
    }

    #[test]
    fn test_conditional_life_total() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        // Set Alice's life to 5
        if let Some(p) = game.player_mut(alice) {
            p.life = 5;
        }

        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let effect =
            ConditionalEffect::if_only(Condition::LifeTotalOrLess(10), vec![Effect::gain_life(3)]);
        effect.execute(&mut game, &mut ctx).unwrap();

        // Should have gained 3 (life <= 10)
        assert_eq!(game.player(alice).unwrap().life, 8);
    }

    #[test]
    fn test_conditional_if_only_false() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let initial_life = game.player(alice).unwrap().life; // 20

        // Life > 10, so condition is false
        let effect =
            ConditionalEffect::if_only(Condition::LifeTotalOrLess(10), vec![Effect::gain_life(3)]);
        effect.execute(&mut game, &mut ctx).unwrap();

        // Should not have gained anything
        assert_eq!(game.player(alice).unwrap().life, initial_life);
    }

    #[test]
    fn test_conditional_clone_box() {
        let effect =
            ConditionalEffect::if_only(Condition::LifeTotalOrLess(10), vec![Effect::gain_life(1)]);
        let cloned = effect.clone_box();
        assert!(format!("{:?}", cloned).contains("ConditionalEffect"));
    }

    #[test]
    fn test_conditional_mana_spent_to_cast_this_spell() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        let card = CardBuilder::new(CardId::new(), "Adamant Probe")
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(4)]]))
            .card_types(vec![CardType::Instant])
            .build();
        let source = game.new_object_id();
        let mut source_obj = Object::from_card(source, &card, alice, Zone::Stack);
        source_obj.mana_spent_to_cast.blue = 3;
        source_obj.mana_spent_to_cast.white = 1;
        game.add_object(source_obj);

        let mut ctx = ExecutionContext::new_default(source, alice);
        let initial_life = game.player(alice).unwrap().life;

        let effect = ConditionalEffect::new(
            Condition::ManaSpentToCastThisSpellAtLeast {
                amount: 3,
                symbol: Some(ManaSymbol::Blue),
            },
            vec![Effect::gain_life(5)],
            vec![Effect::gain_life(1)],
        );
        effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(game.player(alice).unwrap().life, initial_life + 5);
    }
}
