//! Shared delayed-trigger queue primitives.

use crate::effect::Effect;
use crate::game_state::GameState;
use crate::ids::{ObjectId, PlayerId};
use crate::triggers::{DelayedTrigger, Trigger};

/// Config used to enqueue a delayed trigger.
#[derive(Debug, Clone)]
pub(crate) struct DelayedTriggerConfig {
    pub trigger: Trigger,
    pub effects: Vec<Effect>,
    pub one_shot: bool,
    pub not_before_turn: Option<u32>,
    pub expires_at_turn: Option<u32>,
    pub target_objects: Vec<ObjectId>,
    pub ability_source: Option<ObjectId>,
    pub controller: PlayerId,
}

impl DelayedTriggerConfig {
    pub fn new(
        trigger: Trigger,
        effects: Vec<Effect>,
        one_shot: bool,
        target_objects: Vec<ObjectId>,
        controller: PlayerId,
    ) -> Self {
        Self {
            trigger,
            effects,
            one_shot,
            not_before_turn: None,
            expires_at_turn: None,
            target_objects,
            ability_source: None,
            controller,
        }
    }

    pub fn with_not_before_turn(mut self, not_before_turn: Option<u32>) -> Self {
        self.not_before_turn = not_before_turn;
        self
    }

    pub fn with_expires_at_turn(mut self, expires_at_turn: Option<u32>) -> Self {
        self.expires_at_turn = expires_at_turn;
        self
    }

    pub fn with_ability_source(mut self, ability_source: Option<ObjectId>) -> Self {
        self.ability_source = ability_source;
        self
    }
}

/// Push a delayed trigger onto the game queue.
pub(crate) fn queue_delayed_trigger(game: &mut GameState, config: DelayedTriggerConfig) {
    game.delayed_triggers.push(DelayedTrigger {
        trigger: config.trigger,
        effects: config.effects,
        one_shot: config.one_shot,
        not_before_turn: config.not_before_turn,
        expires_at_turn: config.expires_at_turn,
        target_objects: config.target_objects,
        ability_source: config.ability_source,
        controller: config.controller,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Effect;
    use crate::ids::PlayerId;
    use crate::target::ChooseSpec;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    #[test]
    fn test_queue_delayed_trigger_defaults() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let watched = game.new_object_id();

        let config = DelayedTriggerConfig::new(
            Trigger::this_leaves_battlefield(),
            vec![Effect::sacrifice_source()],
            true,
            vec![watched],
            alice,
        );
        queue_delayed_trigger(&mut game, config);

        assert_eq!(game.delayed_triggers.len(), 1);
        let delayed = &game.delayed_triggers[0];
        assert!(delayed.one_shot);
        assert_eq!(delayed.target_objects, vec![watched]);
        assert_eq!(delayed.controller, alice);
        assert_eq!(delayed.not_before_turn, None);
        assert_eq!(delayed.expires_at_turn, None);
        assert_eq!(delayed.ability_source, None);
    }

    #[test]
    fn test_queue_delayed_trigger_with_optional_fields() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let watched = game.new_object_id();
        let turn = game.turn.turn_number;

        let config = DelayedTriggerConfig::new(
            Trigger::end_of_combat(),
            vec![Effect::exile(ChooseSpec::SpecificObject(watched))],
            false,
            vec![watched],
            alice,
        )
        .with_not_before_turn(Some(turn + 1))
        .with_expires_at_turn(Some(turn))
        .with_ability_source(Some(source));
        queue_delayed_trigger(&mut game, config);

        assert_eq!(game.delayed_triggers.len(), 1);
        let delayed = &game.delayed_triggers[0];
        assert!(!delayed.one_shot);
        assert_eq!(delayed.not_before_turn, Some(turn + 1));
        assert_eq!(delayed.expires_at_turn, Some(turn));
        assert_eq!(delayed.ability_source, Some(source));
    }
}
