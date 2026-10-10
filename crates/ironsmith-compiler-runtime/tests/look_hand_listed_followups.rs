//! Spy Network: one look over a target player's hand, the top card of their
//! library and their face-down creatures. Source-authored, deliberately UNRUN.
#[path = "cf8_p10_support/mod.rs"]
mod support;

const SPY_NETWORK: &str = "Mana cost: {U}\nType: Instant\nLook at target player's hand, the top card of that player's library, and any face-down creatures they control. Look at the top four cards of your library, then put them back in any order.";

#[test]
fn spy_network_looks_at_hand_top_card_and_face_down_creatures() {
    for definition in support::definitions("Spy Network", SPY_NETWORK) {
        assert_eq!(support::find_all::<ironsmith::effects::LookAtHandEffect>(&definition).len(), 1);
        let objects = support::find_all::<ironsmith::effects::LookAtObjectsEffect>(&definition);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].filter.face_down, Some(true));
        assert!(objects[0].filter.card_types.contains(&ironsmith::CardType::Creature));
        assert!(!support::find_all::<ironsmith::effects::LookAtTopCardsEffect>(&definition).is_empty());
    }
}
