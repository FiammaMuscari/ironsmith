//! "Whenever [player] draws their Nth card each turn" trigger.

use crate::events::EventKind;
use crate::events::other::CardsDrawnEvent;
use crate::target::PlayerFilter;
use crate::triggers::TriggerEvent;
use crate::triggers::matcher_trait::{TriggerContext, TriggerMatcher};

/// Trigger for "Whenever [player] draws their Nth card each turn".
///
/// This fires once when the draw event includes the configured draw number.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerDrawsNthCardEachTurnTrigger {
    pub player: PlayerFilter,
    pub card_number: u32,
}

impl PlayerDrawsNthCardEachTurnTrigger {
    pub fn new(player: PlayerFilter, card_number: u32) -> Self {
        Self {
            player,
            card_number,
        }
    }
}

impl TriggerMatcher for PlayerDrawsNthCardEachTurnTrigger {
    fn matches(&self, event: &TriggerEvent, ctx: &TriggerContext) -> bool {
        if event.kind() != EventKind::CardsDrawn {
            return false;
        }
        let Some(e) = event.downcast::<CardsDrawnEvent>() else {
            return false;
        };

        let player_matches = match &self.player {
            PlayerFilter::You => e.player == ctx.controller,
            PlayerFilter::Opponent => e.player != ctx.controller,
            PlayerFilter::Any => true,
            PlayerFilter::Specific(id) => e.player == *id,
            _ => true,
        };
        if !player_matches {
            return false;
        }

        if self.card_number == 0 {
            return false;
        }

        let total_after = ctx
            .game
            .cards_drawn_this_turn
            .get(&e.player)
            .copied()
            .unwrap_or(0);
        let drawn_now = e.amount();
        let total_before = total_after.saturating_sub(drawn_now);

        total_before < self.card_number && self.card_number <= total_after
    }

    fn display(&self) -> String {
        let ordinal = ordinal_word(self.card_number);
        match &self.player {
            PlayerFilter::You => format!("Whenever you draw your {ordinal} card each turn"),
            PlayerFilter::Any => format!("Whenever a player draws their {ordinal} card each turn"),
            PlayerFilter::Opponent => {
                format!("Whenever an opponent draws their {ordinal} card each turn")
            }
            PlayerFilter::Specific(_) | PlayerFilter::IteratedPlayer => {
                format!("Whenever that player draws their {ordinal} card each turn")
            }
            _ => format!("Whenever {:?} draws their {ordinal} card each turn", self.player),
        }
    }

    fn clone_box(&self) -> Box<dyn TriggerMatcher> {
        Box::new(self.clone())
    }
}

fn ordinal_word(n: u32) -> &'static str {
    match n {
        1 => "first",
        2 => "second",
        3 => "third",
        4 => "fourth",
        5 => "fifth",
        6 => "sixth",
        7 => "seventh",
        8 => "eighth",
        9 => "ninth",
        10 => "tenth",
        _ => "nth",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;
    use crate::ids::{ObjectId, PlayerId};

    #[test]
    fn test_display() {
        let trigger = PlayerDrawsNthCardEachTurnTrigger::new(PlayerFilter::You, 2);
        assert!(trigger.display().contains("second card each turn"));
    }

    #[test]
    fn test_matches_second_draw() {
        let mut game = GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20);
        let alice = PlayerId::from_index(0);
        let source_id = ObjectId::from_raw(1);

        game.cards_drawn_this_turn.insert(alice, 2);
        let event =
            TriggerEvent::new(CardsDrawnEvent::single(alice, ObjectId::from_raw(2), false));
        let ctx = TriggerContext::for_source(source_id, alice, &game);

        let trigger = PlayerDrawsNthCardEachTurnTrigger::new(PlayerFilter::You, 2);
        assert!(trigger.matches(&event, &ctx));
    }

    #[test]
    fn test_matches_second_draw_in_two_card_draw_event() {
        let mut game = GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20);
        let alice = PlayerId::from_index(0);
        let source_id = ObjectId::from_raw(1);

        game.cards_drawn_this_turn.insert(alice, 2);
        let event = TriggerEvent::new(CardsDrawnEvent::new(
            alice,
            vec![ObjectId::from_raw(2), ObjectId::from_raw(3)],
            true,
        ));
        let ctx = TriggerContext::for_source(source_id, alice, &game);

        let trigger = PlayerDrawsNthCardEachTurnTrigger::new(PlayerFilter::You, 2);
        assert!(trigger.matches(&event, &ctx));
    }

    #[test]
    fn test_does_not_match_wrong_number() {
        let mut game = GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20);
        let alice = PlayerId::from_index(0);
        let source_id = ObjectId::from_raw(1);

        game.cards_drawn_this_turn.insert(alice, 3);
        let event =
            TriggerEvent::new(CardsDrawnEvent::single(alice, ObjectId::from_raw(2), false));
        let ctx = TriggerContext::for_source(source_id, alice, &game);

        let trigger = PlayerDrawsNthCardEachTurnTrigger::new(PlayerFilter::You, 2);
        assert!(!trigger.matches(&event, &ctx));
    }
}
