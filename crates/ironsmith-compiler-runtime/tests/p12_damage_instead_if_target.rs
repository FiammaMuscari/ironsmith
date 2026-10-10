//! "It deals 5 damage instead if that target is white and/or blue": the
//! replacement amount keys on the announced target (CR 614.1a, CR 115.1).
//! Source-authored, unrun.
use ironsmith::cards::CardDefinition;
use ironsmith_compiled_artifact::CompiledCardArtifact;
use ironsmith_compiler::parse_loss;
use ironsmith_compiler_runtime::{compile_to_artifact, compile_to_runtime_definition};

const BARRAGE: &str = "Mana cost: {R}\nType: Instant\nThis spell can't be countered.\nLithomantic Barrage deals 1 damage to target creature or planeswalker. It deals 5 damage instead if that target is white and/or blue.";

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
fn barrage_upgrades_damage_only_for_a_white_or_blue_target() {
    for definition in definitions("Lithomantic Barrage", BARRAGE) {
        use ironsmith::color::ColorSet;
        use ironsmith::decision::SelectFirstDecisionMaker;
        use ironsmith::game_loop::resolve_stack_entry_with;
        use ironsmith::game_state::{StackEntry, TargetAssignment};
        use ironsmith::{CardId, CardType, GameState, PlayerId, Zone};
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);
        for (colors, expected) in [
            (ColorSet::WHITE, 5), (ColorSet::BLUE, 5),
            (ColorSet::WHITE.union(ColorSet::BLUE), 5),
            (ColorSet::RED, 1), (ColorSet::GREEN, 1),
        ] {
            let mut game = GameState::new(vec!["Alice".into(), "Bob".into()], 20);
            let source = game.create_object_from_definition(&definition, alice, Zone::Stack);
            let creature = ironsmith::cards::builders::CardDefinitionBuilder::new(CardId::new(), "Recipient")
                .card_types(vec![CardType::Creature]).color_indicator(colors)
                .power_toughness(ironsmith::card::PowerToughness::fixed(2, 8)).build();
            let target = game.create_object_from_definition(&creature, bob, Zone::Battlefield);
            let requirements = ironsmith::game_loop::extract_target_requirements_from_program_with_modes(
                &game, definition.spell_effect.as_ref().unwrap(), alice, Some(source), None);
            assert_eq!(requirements.len(), 1);
            game.push_to_stack(StackEntry::new(source, alice)
                .with_targets(vec![ironsmith::Target::Object(target)])
                .with_target_assignments(vec![TargetAssignment {
                    spec: requirements[0].spec.clone(), range: 0..1,
                }]));
            resolve_stack_entry_with(&mut game, &mut SelectFirstDecisionMaker).unwrap();
            assert_eq!(game.damage_on(target), expected, "target colors: {colors:?}");
        }
    }
}
