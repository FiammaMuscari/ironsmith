//! Event triggers: cast-or-cycle this card, kicks a spell, play an Island,
//! end of combat on your turn (p12-other). Source-authored, unrun.
#[path = "p12_other/support.rs"]
mod support;

use ironsmith::ability::AbilityKind;
use ironsmith::Zone;

#[test]
fn cast_or_cycle_trigger_functions_from_the_stack_and_after_cycling() {
    for name in ["Drownyard Lurker", "Warped Tusker"] {
        for definition in support::definitions(name) {
            let trigger = definition
                .abilities
                .iter()
                .find(|ability| matches!(ability.kind, AbilityKind::Triggered(_)))
                .expect("cast-or-cycle trigger");
            // Casting observes the spell on the stack. Cycling observes the
            // discarded card's captured hand state, independent of where the
            // discard ultimately put it (including replacement destinations).
            assert!(trigger.functional_zones.contains(&Zone::Stack), "{name}");
            assert!(trigger.functional_zones.contains(&Zone::Hand), "{name}");
            assert!(
                support::effects(&definition)
                    .iter()
                    .any(|effect| effect.downcast_ref::<ironsmith::effects::CreateTokenEffect>().is_some()),
                "{name}: Eldrazi Spawn creation"
            );
        }
    }
}

#[test]
fn saproling_infestation_triggers_on_kicked_casts() {
    for definition in support::definitions("Saproling Infestation") {
        assert_eq!(support::triggered_count(&definition), 1);
        assert!(support::debug(&definition).to_ascii_lowercase().contains("kicked"));
    }
}

#[test]
fn jokulmorder_untap_trigger_names_islands() {
    for definition in support::definitions("Jokulmorder") {
        let debug = support::debug(&definition);
        assert!(debug.contains("Island"), "the played land is filtered by subtype");
        assert!(support::triggered_count(&definition) >= 2);
    }
}

#[test]
fn rose_end_of_combat_trigger_is_qualified_by_your_turn() {
    for definition in support::definitions("Rose, Cutthroat Raider") {
        assert_eq!(support::triggered_count(&definition), 2);
        let debug = support::debug(&definition);
        assert!(debug.contains("YourTurn") || debug.contains("your turn"), "{debug}");
    }
}
