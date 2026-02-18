//! "Whenever [filter] deals combat damage to a player" trigger.

use crate::events::DamageEvent;
use crate::events::EventKind;
use crate::game_event::DamageTarget;
use crate::target::ObjectFilter;
use crate::triggers::TriggerEvent;
use crate::triggers::matcher_trait::{TriggerContext, TriggerMatcher};

#[derive(Debug, Clone, PartialEq)]
pub struct DealsCombatDamageToPlayerTrigger {
    pub filter: ObjectFilter,
    pub one_or_more: bool,
}

impl DealsCombatDamageToPlayerTrigger {
    pub fn new(filter: ObjectFilter) -> Self {
        Self {
            filter,
            one_or_more: false,
        }
    }

    pub fn one_or_more(filter: ObjectFilter) -> Self {
        Self {
            filter,
            one_or_more: true,
        }
    }

    fn first_matching_hit_to_player_in_batch(
        &self,
        player: crate::ids::PlayerId,
        ctx: &TriggerContext,
    ) -> bool {
        for (source, damaged_player) in ctx.game.combat_damage_player_batch_hits() {
            if *damaged_player != player {
                continue;
            }
            let Some(source_obj) = ctx.game.object(*source) else {
                continue;
            };
            if self.filter.matches(source_obj, &ctx.filter_ctx, ctx.game) {
                return false;
            }
        }
        true
    }
}

impl TriggerMatcher for DealsCombatDamageToPlayerTrigger {
    fn matches(&self, event: &TriggerEvent, ctx: &TriggerContext) -> bool {
        if event.kind() != EventKind::Damage {
            return false;
        }
        let Some(e) = event.downcast::<DamageEvent>() else {
            return false;
        };
        // Must be combat damage to a player
        if !e.is_combat || !matches!(e.target, DamageTarget::Player(_)) {
            return false;
        }
        let Some(obj) = ctx.game.object(e.source) else {
            return false;
        };
        if !self.filter.matches(obj, &ctx.filter_ctx, ctx.game) {
            return false;
        }
        if !self.one_or_more {
            return true;
        }
        let DamageTarget::Player(player) = e.target else {
            return false;
        };
        self.first_matching_hit_to_player_in_batch(player, ctx)
    }

    fn display(&self) -> String {
        if self.one_or_more {
            let mut subject = self.filter.description();
            if let Some(stripped) = subject.strip_prefix("a ") {
                subject = stripped.to_string();
            } else if let Some(stripped) = subject.strip_prefix("an ") {
                subject = stripped.to_string();
            }
            return format!("Whenever one or more {subject} deal combat damage to a player");
        }
        format!(
            "Whenever {} deals combat damage to a player",
            self.filter.description()
        )
    }

    fn clone_box(&self) -> Box<dyn TriggerMatcher> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::game_state::GameState;
    use crate::ids::{CardId, ObjectId, PlayerId};
    use crate::types::CardType;
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn create_creature(game: &mut GameState, name: &str, controller: PlayerId) -> ObjectId {
        let card = CardBuilder::new(CardId::from_raw(game.new_object_id().0 as u32), name)
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(2, 2))
            .build();
        game.create_object_from_card(&card, controller, Zone::Battlefield)
    }

    #[test]
    fn test_display() {
        let trigger = DealsCombatDamageToPlayerTrigger::new(ObjectFilter::creature());
        assert!(trigger.display().contains("deals combat damage"));
    }

    #[test]
    fn test_one_or_more_matches_only_first_matching_hit_per_player_in_batch() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);
        let source_id = ObjectId::from_raw(100);
        let attacker_one = create_creature(&mut game, "A", alice);
        let attacker_two = create_creature(&mut game, "B", alice);

        let trigger = DealsCombatDamageToPlayerTrigger::one_or_more(ObjectFilter::creature());
        let ctx = TriggerContext::for_source(source_id, alice, &game);
        let first_event = TriggerEvent::new(DamageEvent::new(
            attacker_one,
            DamageTarget::Player(bob),
            2,
            true,
        ));
        assert!(trigger.matches(&first_event, &ctx));

        game.record_combat_damage_player_batch_hit(attacker_one, bob);
        let ctx = TriggerContext::for_source(source_id, alice, &game);
        let second_event = TriggerEvent::new(DamageEvent::new(
            attacker_two,
            DamageTarget::Player(bob),
            2,
            true,
        ));
        assert!(!trigger.matches(&second_event, &ctx));
    }
}
