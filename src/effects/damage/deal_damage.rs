//! Deal damage effect implementation.
//!
//! This module implements the `DealDamage` effect, which deals damage to a target
//! creature, planeswalker, or player.

use crate::effect::{EffectOutcome, EffectResult, Value};
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_value;
use crate::event_processor::process_damage_with_event_with_source_snapshot;
use crate::events::DamageEvent;
use crate::events::LifeLossEvent;
use crate::executor::{ExecutionContext, ExecutionError, ResolvedTarget};
use crate::game_event::DamageTarget;
use crate::game_state::GameState;
use crate::target::{ChooseSpec, PlayerFilter};
use crate::triggers::TriggerEvent;
use crate::types::CardType;

/// Effect that deals damage to a target creature, planeswalker, or player.
///
/// # Fields
///
/// * `amount` - The amount of damage to deal (can be fixed or variable)
/// * `target` - The target specification (creature, player, or "any target")
/// * `source_is_combat` - Whether this damage is combat damage
///
/// # Example
///
/// ```ignore
/// // Deal 3 damage to any target (Lightning Bolt)
/// let effect = DealDamageEffect {
///     amount: Value::Fixed(3),
///     target: ChooseSpec::AnyTarget,
///     source_is_combat: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct DealDamageEffect {
    /// The amount of damage to deal.
    pub amount: Value,
    /// The target specification.
    pub target: ChooseSpec,
    /// Whether this damage is combat damage.
    pub source_is_combat: bool,
}

impl DealDamageEffect {
    /// Create a new deal damage effect.
    pub fn new(amount: impl Into<Value>, target: ChooseSpec) -> Self {
        Self {
            amount: amount.into(),
            target,
            source_is_combat: false,
        }
    }

    /// Set whether this is combat damage.
    pub fn with_combat(mut self, is_combat: bool) -> Self {
        self.source_is_combat = is_combat;
        self
    }
}

impl EffectExecutor for DealDamageEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let amount = resolve_value(game, &self.amount, ctx)?.max(0) as u32;
        let make_outcome = |dealt_damage: u32, target: DamageTarget, life_loss_from_damage: u32| {
            let mut outcome = EffectOutcome::count(dealt_damage as i32);
            if dealt_damage > 0 {
                outcome = outcome.with_event(TriggerEvent::new(DamageEvent::new(
                    ctx.source,
                    target,
                    dealt_damage,
                    self.source_is_combat,
                )));
            }

            if life_loss_from_damage > 0
                && let DamageTarget::Player(player_id) = target
            {
                outcome = outcome.with_event(TriggerEvent::new(LifeLossEvent::new(
                    player_id,
                    life_loss_from_damage,
                    true,
                )));
            }

            outcome
        };

        let apply_player_life_change = |game: &mut GameState, player_id, dealt_damage| {
            if dealt_damage == 0 || !game.can_change_life_total(player_id) {
                return 0;
            }

            let Some(player) = game.player_mut(player_id) else {
                return 0;
            };

            player.deal_damage(dealt_damage)
        };

        // Check if this is targeting IteratedPlayer (used in ForEachOpponent)
        // If so, resolve the target from the context's iterated_player
        if let ChooseSpec::Player(PlayerFilter::IteratedPlayer) = &self.target {
            if let Some(player_id) = ctx.iterated_player {
                // Process through replacement/prevention effects
                let (final_damage, was_prevented) = process_damage_with_event_with_source_snapshot(
                    game,
                    ctx.source,
                    DamageTarget::Player(player_id),
                    amount,
                    self.source_is_combat,
                    ctx.source_snapshot.as_ref(),
                );

                if was_prevented {
                    return Ok(EffectOutcome::from_result(EffectResult::Prevented));
                }

                let life_loss = apply_player_life_change(game, player_id, final_damage);
                return Ok(make_outcome(
                    final_damage,
                    DamageTarget::Player(player_id),
                    life_loss,
                ));
            }
            return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
        }

        if let ChooseSpec::Iterated = &self.target {
            if let Some(object_id) = ctx.iterated_object {
                if let Some(obj) = game.object(object_id)
                    && (obj.has_card_type(CardType::Creature)
                        || obj.has_card_type(CardType::Planeswalker))
                {
                    let (final_damage, was_prevented) =
                        process_damage_with_event_with_source_snapshot(
                            game,
                            ctx.source,
                            DamageTarget::Object(object_id),
                            amount,
                            self.source_is_combat,
                            ctx.source_snapshot.as_ref(),
                        );

                    if was_prevented {
                        return Ok(EffectOutcome::from_result(EffectResult::Prevented));
                    }

                    if final_damage > 0 {
                        game.mark_damage(object_id, final_damage);
                    }
                    return Ok(make_outcome(
                        final_damage,
                        DamageTarget::Object(object_id),
                        0,
                    ));
                }
                return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
            }
            return Ok(EffectOutcome::from_result(EffectResult::TargetInvalid));
        }

        // Handle SourceController - deal damage to the controller of the source (e.g., Ancient Tomb)
        if let ChooseSpec::SourceController = &self.target {
            let controller = ctx.controller;
            // Process through replacement/prevention effects
            let (final_damage, was_prevented) = process_damage_with_event_with_source_snapshot(
                game,
                ctx.source,
                DamageTarget::Player(controller),
                amount,
                self.source_is_combat,
                ctx.source_snapshot.as_ref(),
            );

            if was_prevented {
                return Ok(EffectOutcome::from_result(EffectResult::Prevented));
            }

            let life_loss = apply_player_life_change(game, controller, final_damage);
            return Ok(make_outcome(
                final_damage,
                DamageTarget::Player(controller),
                life_loss,
            ));
        }

        // Otherwise, use pre-resolved targets from ctx.targets
        for target in &ctx.targets {
            match target {
                ResolvedTarget::Player(player_id) => {
                    // Process through replacement/prevention effects
                    let (final_damage, was_prevented) =
                        process_damage_with_event_with_source_snapshot(
                            game,
                            ctx.source,
                            DamageTarget::Player(*player_id),
                            amount,
                            self.source_is_combat,
                            ctx.source_snapshot.as_ref(),
                        );

                    if was_prevented {
                        return Ok(EffectOutcome::from_result(EffectResult::Prevented));
                    }

                    let life_loss = apply_player_life_change(game, *player_id, final_damage);
                    return Ok(make_outcome(
                        final_damage,
                        DamageTarget::Player(*player_id),
                        life_loss,
                    ));
                }
                ResolvedTarget::Object(object_id) => {
                    if let Some(obj) = game.object(*object_id)
                        && (obj.has_card_type(CardType::Creature)
                            || obj.has_card_type(CardType::Planeswalker))
                    {
                        // Process through replacement/prevention effects
                        let (final_damage, was_prevented) =
                            process_damage_with_event_with_source_snapshot(
                                game,
                                ctx.source,
                                DamageTarget::Object(*object_id),
                                amount,
                                self.source_is_combat,
                                ctx.source_snapshot.as_ref(),
                            );

                        if was_prevented {
                            return Ok(EffectOutcome::from_result(EffectResult::Prevented));
                        }

                        if final_damage > 0 {
                            game.mark_damage(*object_id, final_damage);
                        }
                        return Ok(make_outcome(
                            final_damage,
                            DamageTarget::Object(*object_id),
                            0,
                        ));
                    }
                }
            }
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
        "target for damage"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::events::EventKind;
    use crate::ids::{CardId, ObjectId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::object::Object;
    use crate::types::CardType;
    use crate::zone::Zone;

    fn new_test_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    /// Create a simple source card (like a spell)
    fn make_source_card() -> crate::card::Card {
        CardBuilder::new(CardId::from_raw(1), "Test Source")
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Red]]))
            .card_types(vec![CardType::Instant])
            .build()
    }

    /// Create a simple creature card
    fn make_creature_card(
        card_id: u32,
        name: &str,
        power: i32,
        toughness: i32,
    ) -> crate::card::Card {
        CardBuilder::new(CardId::from_raw(card_id), name)
            .mana_cost(ManaCost::from_pips(vec![
                vec![ManaSymbol::Generic(1)],
                vec![ManaSymbol::Green],
            ]))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(power, toughness))
            .build()
    }

    /// Create a test game with two players and a creature on the battlefield.
    fn setup_game_with_creature() -> (GameState, ObjectId, ObjectId) {
        let mut game = new_test_game();
        let player_id = game.players[0].id;

        // Create a source object (the spell dealing damage)
        let source_id = game.new_object_id();
        let source_card = make_source_card();
        let source_obj = Object::from_card(source_id, &source_card, player_id, Zone::Stack);
        game.add_object(source_obj);

        // Create a creature to damage
        let creature_id = game.new_object_id();
        let creature_card = make_creature_card(2, "Grizzly Bears", 2, 2);
        let creature_obj =
            Object::from_card(creature_id, &creature_card, player_id, Zone::Battlefield);
        game.add_object(creature_obj);
        game.battlefield.push(creature_id);

        (game, source_id, creature_id)
    }

    #[test]
    fn test_deal_damage_to_creature() {
        let (mut game, source_id, creature_id) = setup_game_with_creature();
        let player_id = game.players[0].id;

        let effect = DealDamageEffect::new(3, ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player_id)
            .with_targets(vec![ResolvedTarget::Object(creature_id)]);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(3));
        assert_eq!(game.damage_on(creature_id), 3);
    }

    #[test]
    fn test_deal_damage_to_player() {
        let mut game = new_test_game();
        let player1_id = game.players[0].id;
        let player2_id = game.players[1].id;

        let source_id = game.new_object_id();
        let source_card = make_source_card();
        let source_obj = Object::from_card(source_id, &source_card, player1_id, Zone::Stack);
        game.add_object(source_obj);

        let initial_life = game.player(player2_id).unwrap().life;

        let effect = DealDamageEffect::new(3, ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player1_id)
            .with_targets(vec![ResolvedTarget::Player(player2_id)]);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(3));
        assert_eq!(game.player(player2_id).unwrap().life, initial_life - 3);
    }

    #[test]
    fn test_deal_damage_to_player_emits_life_loss_event() {
        let mut game = new_test_game();
        let player1_id = game.players[0].id;
        let player2_id = game.players[1].id;

        let source_id = game.new_object_id();
        let source_card = make_source_card();
        let source_obj = Object::from_card(source_id, &source_card, player1_id, Zone::Stack);
        game.add_object(source_obj);

        let effect = DealDamageEffect::new(3, ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player1_id)
            .with_targets(vec![ResolvedTarget::Player(player2_id)]);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.events.len(), 2);
        assert_eq!(result.events[0].kind(), EventKind::Damage);
        assert_eq!(result.events[1].kind(), EventKind::LifeLoss);

        let life_loss = result.events[1].downcast::<LifeLossEvent>().unwrap();
        assert_eq!(life_loss.player, player2_id);
        assert_eq!(life_loss.amount, 3);
        assert!(life_loss.from_damage);
    }

    #[test]
    fn test_deal_damage_to_player_life_locked_emits_no_life_loss_event() {
        let mut game = new_test_game();
        let player1_id = game.players[0].id;
        let player2_id = game.players[1].id;

        game.cant_effects.life_total_cant_change.insert(player2_id);

        let source_id = game.new_object_id();
        let source_card = make_source_card();
        let source_obj = Object::from_card(source_id, &source_card, player1_id, Zone::Stack);
        game.add_object(source_obj);

        let initial_life = game.player(player2_id).unwrap().life;

        let effect = DealDamageEffect::new(3, ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player1_id)
            .with_targets(vec![ResolvedTarget::Player(player2_id)]);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].kind(), EventKind::Damage);
        assert_eq!(game.player(player2_id).unwrap().life, initial_life);
    }

    #[test]
    fn test_deal_variable_damage() {
        let (mut game, source_id, creature_id) = setup_game_with_creature();
        let player_id = game.players[0].id;

        // Deal X damage where X = 5
        let effect = DealDamageEffect::new(Value::X, ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player_id)
            .with_x(5)
            .with_targets(vec![ResolvedTarget::Object(creature_id)]);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(5));
        assert_eq!(game.damage_on(creature_id), 5);
    }

    #[test]
    fn test_deal_damage_no_target() {
        let mut game = new_test_game();
        let player_id = game.players[0].id;

        let source_id = game.new_object_id();
        let source_card = make_source_card();
        let source_obj = Object::from_card(source_id, &source_card, player_id, Zone::Stack);
        game.add_object(source_obj);

        let effect = DealDamageEffect::new(3, ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player_id);
        // No targets set

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::TargetInvalid);
    }

    #[test]
    fn test_deal_zero_damage() {
        let (mut game, source_id, creature_id) = setup_game_with_creature();
        let player_id = game.players[0].id;

        let effect = DealDamageEffect::new(0, ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player_id)
            .with_targets(vec![ResolvedTarget::Object(creature_id)]);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        // 0 damage is still counted as the effect executing
        assert_eq!(result.result, EffectResult::Count(0));
        assert_eq!(game.damage_on(creature_id), 0);
    }

    #[test]
    fn test_deal_damage_to_iterated_player() {
        let mut game = new_test_game();
        let player1_id = game.players[0].id;
        let player2_id = game.players[1].id;

        let source_id = game.new_object_id();
        let source_card = make_source_card();
        let source_obj = Object::from_card(source_id, &source_card, player1_id, Zone::Stack);
        game.add_object(source_obj);

        let initial_life = game.player(player2_id).unwrap().life;

        // Use IteratedPlayer target (as in ForEachOpponent)
        let effect = DealDamageEffect::new(2, ChooseSpec::Player(PlayerFilter::IteratedPlayer));
        let mut ctx = ExecutionContext::new_default(source_id, player1_id);
        ctx.iterated_player = Some(player2_id);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(2));
        assert_eq!(game.player(player2_id).unwrap().life, initial_life - 2);
    }

    #[test]
    fn test_deal_damage_negative_becomes_zero() {
        let (mut game, source_id, creature_id) = setup_game_with_creature();
        let player_id = game.players[0].id;

        // Negative damage should be treated as 0
        let effect = DealDamageEffect::new(Value::Fixed(-5), ChooseSpec::AnyTarget);
        let mut ctx = ExecutionContext::new_default(source_id, player_id)
            .with_targets(vec![ResolvedTarget::Object(creature_id)]);

        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(0));
        assert_eq!(game.damage_on(creature_id), 0);
    }

    #[test]
    fn test_deal_damage_is_debug() {
        let effect = DealDamageEffect::new(3, ChooseSpec::AnyTarget);
        let debug_str = format!("{:?}", effect);
        assert!(debug_str.contains("DealDamageEffect"));
        assert!(debug_str.contains("amount"));
    }
}
