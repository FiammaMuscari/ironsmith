//! "Whenever [player] loses life" trigger.

use crate::events::EventKind;
use crate::events::life::LifeLossEvent;
use crate::target::PlayerFilter;
use crate::triggers::TriggerEvent;
use crate::triggers::matcher_trait::{TriggerContext, TriggerMatcher};

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerLosesLifeTrigger {
    pub player: PlayerFilter,
    pub during_turn: Option<PlayerFilter>,
}

impl PlayerLosesLifeTrigger {
    pub fn new(player: PlayerFilter) -> Self {
        Self {
            player,
            during_turn: None,
        }
    }

    pub fn during_turn(player: PlayerFilter, during_turn: PlayerFilter) -> Self {
        Self {
            player,
            during_turn: Some(during_turn),
        }
    }
}

impl TriggerMatcher for PlayerLosesLifeTrigger {
    fn matches(&self, event: &TriggerEvent, ctx: &TriggerContext) -> bool {
        if event.kind() != EventKind::LifeLoss {
            return false;
        }
        let Some(e) = event.downcast::<LifeLossEvent>() else {
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
        if let Some(during_turn) = &self.during_turn {
            let active_player = ctx.game.turn.active_player;
            return match during_turn {
                PlayerFilter::You => active_player == ctx.controller,
                PlayerFilter::Opponent => active_player != ctx.controller,
                PlayerFilter::Any | PlayerFilter::Active => true,
                PlayerFilter::Specific(id) => active_player == *id,
                _ => true,
            };
        }
        true
    }

    fn display(&self) -> String {
        let base = match &self.player {
            PlayerFilter::You => "Whenever you lose life".to_string(),
            PlayerFilter::Opponent => "Whenever an opponent loses life".to_string(),
            PlayerFilter::Any => "Whenever a player loses life".to_string(),
            _ => format!("Whenever {:?} loses life", self.player),
        };
        if let Some(during_turn) = &self.during_turn {
            let suffix = match during_turn {
                PlayerFilter::You => " during your turn",
                PlayerFilter::Opponent => " during an opponent's turn",
                PlayerFilter::Specific(_) => " during that player's turn",
                _ => "",
            };
            format!("{base}{suffix}")
        } else {
            base
        }
    }

    fn clone_box(&self) -> Box<dyn TriggerMatcher> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display() {
        let trigger = PlayerLosesLifeTrigger::new(PlayerFilter::Any);
        assert!(trigger.display().contains("loses life"));
    }
}
