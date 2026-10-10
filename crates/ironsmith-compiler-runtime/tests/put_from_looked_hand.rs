//! "Look at defending player's hand. You may put a creature card from it onto
//! the battlefield under your control tapped and attacking that player or a
//! planeswalker they control." (Zara): the put's source is the looked-at hand.
//! Source-authored, deliberately UNRUN.
#[path = "cf8_p10_support/mod.rs"]
mod support;

use ironsmith::effects::{MoveToZoneAttackTargetMode, MoveToZoneEffect, ReturnToHandEffect};
use ironsmith::target::PlayerFilter;
use ironsmith::Zone;

const ZARA: &str = "Mana cost: {3}{U}{R}\nType: Legendary Creature — Human Pirate\nPower/Toughness: 4/3\nFlying\nWhenever Zara attacks, look at defending player's hand. You may put a creature card from it onto the battlefield under your control tapped and attacking that player or a planeswalker they control. Return that creature to its owner's hand at the beginning of the next end step.";

#[test]
fn zara_puts_a_creature_from_the_defending_hand_attacking_that_player() {
    for definition in support::definitions("Zara, Renegade Recruiter", ZARA) {
        let debug = format!("{:?}", definition.abilities);
        assert!(debug.contains("LookAtHand"), "{debug}");
        let moves = support::find_all::<MoveToZoneEffect>(&definition);
        let entry = moves
            .iter()
            .find(|effect| effect.zone == Zone::Battlefield)
            .unwrap_or_else(|| panic!("{moves:#?}"));
        assert!(entry.enters_tapped && entry.enters_attacking);
        assert_eq!(
            entry.attack_target_mode,
            Some(MoveToZoneAttackTargetMode::PlayerOrPlaneswalkerControlledBy(
                PlayerFilter::Defending
            ))
        );
        let ironsmith::target::ChooseSpec::Tagged(tag) = entry.target.base() else {
            panic!("the entry must use the selection from the looked-at hand: {entry:?}");
        };
        let choices = support::find_all::<ironsmith::effects::ChooseObjectsEffect>(&definition);
        let choice = choices.iter().find(|choice| &choice.tag == tag)
            .expect("the moved card must come from the preceding hand selection");
        assert_eq!(choice.filter.zone, Some(Zone::Hand));
        assert_eq!(choice.filter.owner, Some(PlayerFilter::Defending));
        assert!(choice.filter.card_types.contains(&ironsmith::types::CardType::Creature));
        assert!(
            !support::find_all::<ReturnToHandEffect>(&definition).is_empty(),
            "the delayed return to its owner's hand: {moves:#?}"
        );
    }
}
