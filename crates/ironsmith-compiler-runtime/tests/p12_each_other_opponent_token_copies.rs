//! "Whenever ~ attacks a player, for each other opponent, create a token
//! that's a copy of ~ tapped and attacking that player, except it isn't
//! legendary" (CR 508.4, CR 707.9b). Source-authored, unrun.
use ironsmith::cards::CardDefinition;
#[path = "cf8_p10_support/mod.rs"]
mod support;
use ironsmith_compiled_artifact::CompiledCardArtifact;
use ironsmith_compiler::parse_loss;
use ironsmith_compiler_runtime::{compile_to_artifact, compile_to_runtime_definition};

const SHREDDER: &str = "Mana cost: {3}{B}{B}\nType: Legendary Creature — Human Ninja\nPower/Toughness: 5/5\nWhenever Shredder attacks a player, for each other opponent, create a token that's a copy of Shredder tapped and attacking that player, except it isn't legendary. Sacrifice those tokens at end of combat.\nWhenever Shredder deals combat damage to a player, that player loses half their life, rounded up.";

fn definitions(name: &str, text: &str) -> [CardDefinition; 2] {
    let (direct, loss) = parse_loss::capture(|| compile_to_runtime_definition(name, text, false));
    assert!(!loss.is_lossy(), "{name}: {}", loss.reasons_text());
    let direct = direct.unwrap_or_else(|error| panic!("{name}: {error}"));
    let (artifact, _) = compile_to_artifact(name, text, false).unwrap();
    let restored = CompiledCardArtifact::from_json(&artifact.to_json().unwrap()).unwrap();
    restored.validate().unwrap();
    let decoded =
        ironsmith_runtime_catalog::artifact_materializer::materialize_artifact(&restored).unwrap();
    for definition in [&direct, &decoded] {
        assert!(
            !ironsmith::cards::generated_definition_has_unimplemented_content(definition),
            "{name}: unimplemented content"
        );
    }
    [direct, decoded]
}

#[test]
fn shredder_copies_attack_each_other_opponent() {
    for definition in definitions("Shredder, Shadow Master", SHREDDER) {
        let debug = format!("{definition:?}");
        let loops = support::find_all::<ironsmith::effects::ForPlayersEffect>(&definition);
        assert_eq!(loops.len(), 1, "{debug}");
        assert_eq!(loops[0].filter, ironsmith::target::PlayerFilter::Excluding {
            base: Box::new(ironsmith::target::PlayerFilter::Opponent),
            excluded: Box::new(ironsmith::target::PlayerFilter::Defending),
        });
        assert!(loops[0].effects.iter().any(|effect|
            effect.downcast_ref::<ironsmith::effects::ScheduleDelayedTriggerEffect>().is_some()),
            "each opponent's creation captures its own delayed sacrifice");
        assert!(debug.contains("Defending"), "excludes the attacked player: {debug}");
        assert!(debug.contains("IteratedPlayer"), "attacks the iterated opponent: {debug}");
        assert!(debug.contains("Legendary"), "removes legendary: {debug}");
    }
}
