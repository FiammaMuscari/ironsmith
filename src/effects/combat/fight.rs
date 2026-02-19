//! Fight effect implementation.

use crate::effect::{Effect, EffectOutcome, EffectResult};
use crate::effects::EffectExecutor;
use crate::executor::{ExecutionContext, ExecutionError, ResolvedTarget, execute_effect};
use crate::game_state::GameState;
use crate::target::ChooseSpec;

/// Effect that makes two creatures fight.
///
/// Each creature deals damage equal to its power to the other.
///
/// # Fields
///
/// * `creature1` - First creature (often "target creature you control")
/// * `creature2` - Second creature (often "target creature you don't control")
///
/// # Example
///
/// ```ignore
/// // Target creature you control fights target creature you don't control
/// let effect = FightEffect::new(
///     ChooseSpec::creature().you_control(),
///     ChooseSpec::creature().opponent_controls(),
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct FightEffect {
    /// First creature specification.
    pub creature1: ChooseSpec,
    /// Second creature specification.
    pub creature2: ChooseSpec,
}

impl FightEffect {
    /// Create a new fight effect.
    pub fn new(creature1: ChooseSpec, creature2: ChooseSpec) -> Self {
        Self {
            creature1,
            creature2,
        }
    }

    /// Create a fight between a creature you control and one you don't.
    pub fn you_vs_opponent() -> Self {
        Self::new(ChooseSpec::creature(), ChooseSpec::creature())
    }
}

impl EffectExecutor for FightEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        // Get both targets from resolved targets
        let Some((creature1_id, creature2_id)) = ctx.resolve_two_object_targets() else {
            return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
        };

        // Use calculated power so continuous effects (pumps/shrinks) are respected.
        let power1 = game.calculated_power(creature1_id).unwrap_or(0).max(0) as u32;
        let power2 = game.calculated_power(creature2_id).unwrap_or(0).max(0) as u32;

        // Each creature deals damage equal to its power to the other.
        // Decompose into two DealDamage effects and aggregate outcomes.
        let mut outcomes = Vec::new();

        if power1 > 0 {
            let outcome =
                ctx.with_temp_targets(vec![ResolvedTarget::Object(creature2_id)], |ctx| {
                    let effect = Effect::deal_damage(power1 as i32, ChooseSpec::AnyTarget);
                    execute_effect(game, &effect, ctx)
                })?;
            outcomes.push(outcome);
        }

        if power2 > 0 {
            let outcome =
                ctx.with_temp_targets(vec![ResolvedTarget::Object(creature1_id)], |ctx| {
                    let effect = Effect::deal_damage(power2 as i32, ChooseSpec::AnyTarget);
                    execute_effect(game, &effect, ctx)
                })?;
            outcomes.push(outcome);
        }

        Ok(EffectOutcome::aggregate(outcomes))
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        Some(&self.creature1)
    }

    fn target_description(&self) -> &'static str {
        "creature to fight"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::continuous::ContinuousEffect;
    use crate::effect::Until;
    use crate::ids::{CardId, ObjectId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::object::Object;
    use crate::types::CardType;
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn make_creature_card(
        card_id: u32,
        name: &str,
        power: i32,
        toughness: i32,
    ) -> crate::card::Card {
        CardBuilder::new(CardId::from_raw(card_id), name)
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(2)]]))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(power, toughness))
            .build()
    }

    fn create_creature(
        game: &mut GameState,
        name: &str,
        power: i32,
        toughness: i32,
        controller: PlayerId,
    ) -> ObjectId {
        let id = game.new_object_id();
        let card = make_creature_card(id.0 as u32, name, power, toughness);
        let obj = Object::from_card(id, &card, controller, Zone::Battlefield);
        game.add_object(obj);
        id
    }

    #[test]
    fn test_fight_basic() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let bear = create_creature(&mut game, "Grizzly Bears", 2, 2, alice);
        let goblin = create_creature(&mut game, "Goblin Piker", 2, 1, bob);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Object(bear),
            ResolvedTarget::Object(goblin),
        ]);

        let effect = FightEffect::you_vs_opponent();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(4));

        // Bear (2/2) takes 2 damage from Goblin
        assert_eq!(game.damage_on(bear), 2);

        // Goblin (2/1) takes 2 damage from Bear (lethal)
        assert_eq!(game.damage_on(goblin), 2);
    }

    #[test]
    fn test_fight_asymmetric_power() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let big = create_creature(&mut game, "Big Creature", 5, 5, alice);
        let small = create_creature(&mut game, "Small Creature", 1, 1, bob);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Object(big),
            ResolvedTarget::Object(small),
        ]);

        let effect = FightEffect::you_vs_opponent();
        effect.execute(&mut game, &mut ctx).unwrap();

        // Big takes 1 damage
        assert_eq!(game.damage_on(big), 1);
        // Small takes 5 damage
        assert_eq!(game.damage_on(small), 5);
    }

    #[test]
    fn test_fight_zero_power() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let wall = create_creature(&mut game, "Wall", 0, 4, alice);
        let attacker = create_creature(&mut game, "Attacker", 3, 3, bob);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Object(wall),
            ResolvedTarget::Object(attacker),
        ]);

        let effect = FightEffect::you_vs_opponent();
        effect.execute(&mut game, &mut ctx).unwrap();

        // Wall deals 0 damage (0 power)
        assert_eq!(game.damage_on(attacker), 0);
        // Attacker deals 3 damage to wall
        assert_eq!(game.damage_on(wall), 3);
    }

    #[test]
    fn test_fight_insufficient_targets() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        let bear = create_creature(&mut game, "Bear", 2, 2, alice);
        let source = game.new_object_id();

        // Only one target
        let mut ctx = ExecutionContext::new_default(source, alice)
            .with_targets(vec![ResolvedTarget::Object(bear)]);

        let effect = FightEffect::you_vs_opponent();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::TargetInvalid);
    }

    #[test]
    fn test_fight_no_targets() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();

        let mut ctx = ExecutionContext::new_default(source, alice);

        let effect = FightEffect::you_vs_opponent();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::TargetInvalid);
    }

    #[test]
    fn test_fight_clone_box() {
        let effect = FightEffect::you_vs_opponent();
        let cloned = effect.clone_box();
        assert!(format!("{:?}", cloned).contains("FightEffect"));
    }

    #[test]
    fn test_fight_uses_calculated_power_with_continuous_effects() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);

        let bear = create_creature(&mut game, "Bear", 2, 2, alice);
        let ogre = create_creature(&mut game, "Ogre", 2, 2, bob);
        let source = game.new_object_id();

        // +2/+0 pump should increase fight damage dealt by Bear.
        game.continuous_effects.add_effect(ContinuousEffect::pump(
            source,
            alice,
            bear,
            2,
            0,
            Until::EndOfTurn,
        ));

        let mut ctx = ExecutionContext::new_default(source, alice).with_targets(vec![
            ResolvedTarget::Object(bear),
            ResolvedTarget::Object(ogre),
        ]);

        let effect = FightEffect::you_vs_opponent();
        effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(game.damage_on(ogre), 4);
        assert_eq!(game.damage_on(bear), 2);
    }
}
