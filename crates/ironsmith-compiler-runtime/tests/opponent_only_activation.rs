//! cf8 p05: "Only your opponents may activate this ability [and only as a
//! sorcery]." (CR 602.1, 602.5d): a typed activator-relative timing.
//! Source-authored; deliberately unrun.
#[path = "cf8_p05_support/mod.rs"]
mod support;
use ironsmith::ability::{AbilityKind, ActivationTiming};

#[test]
fn detention_vortex_is_activated_only_by_opponents_at_sorcery_speed() {
    for definition in support::definitions(&support::row("opponent_only_activation", "Detention Vortex")) {
        let activated = definition
            .abilities
            .iter()
            .find_map(|ability| match &ability.kind {
                AbilityKind::Activated(activated) => Some(activated),
                _ => None,
            })
            .unwrap();
        assert_opponent_timing(activated, true);
        assert!(activated.allows_any_player_to_activate());
        // Original wording remains available for display; executable
        // restrictions are the typed fields checked above.
        assert!(activated.activation_restrictions.is_empty());
    }
}

#[test]
fn opponent_only_timing_sentences_are_typed() {
    for (text, timing) in [
        ("{1}: Draw a card. Only your opponents may activate this ability.", ActivationTiming::AnyTimeByOpponents),
        (
            "{1}: Draw a card. Only your opponents may activate this ability and only as a sorcery.",
            ActivationTiming::SorcerySpeedByOpponents,
        ),
    ] {
        let source = format!("Mana cost: {{1}}\nType: Artifact\n{text}");
        let definition =
            ironsmith_compiler_runtime::compile_to_runtime_definition("Opponent activation probe", &source, false)
                .unwrap();
        let activated = definition.abilities.iter().find_map(|ability| match &ability.kind {
            AbilityKind::Activated(activated) => Some(activated),
            _ => None,
        }).unwrap();
        assert_opponent_timing(activated, timing == ActivationTiming::SorcerySpeedByOpponents);
    }
}

fn assert_opponent_timing(activated: &ironsmith::ability::ActivatedAbility, sorcery_only: bool) {
    // Coordinated restrictions may retain the activator in `timing` and the
    // independent sorcery window as a typed condition. Both must survive.
    if sorcery_only && activated.timing == ActivationTiming::SorcerySpeedByOpponents {
        return;
    }
    assert_eq!(activated.timing, ActivationTiming::AnyTimeByOpponents);
    let expected = sorcery_only.then_some(ironsmith::ConditionExpr::ActivationTiming(ActivationTiming::SorcerySpeed));
    assert_eq!(activated.activation_condition, expected);
}
