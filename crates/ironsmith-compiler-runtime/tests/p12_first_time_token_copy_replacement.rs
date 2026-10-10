//! "The first time you would create one or more tokens each turn, you may
//! instead create that many tokens that are copies of enchanted permanent."
//! (CR 614.1, CR 707.2). Source-authored, unrun.
use ironsmith::cards::CardDefinition;
use ironsmith_compiled_artifact::CompiledCardArtifact;
use ironsmith_compiler::parse_loss;
use ironsmith_compiler_runtime::{compile_to_artifact, compile_to_runtime_definition};

const MOONLIT_MEDITATION: &str = "Mana cost: {2}{U}\nType: Enchantment — Aura\nEnchant artifact or creature you control\nThe first time you would create one or more tokens each turn, you may instead create that many tokens that are copies of enchanted permanent.";

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
fn moonlit_meditation_replaces_only_the_first_creation_with_copies() {
    for definition in definitions("Moonlit Meditation", MOONLIT_MEDITATION) {
        let debug = format!("{definition:?}");
        assert!(debug.contains("TokenCreationTemplates"), "{debug}");
        assert!(debug.contains("CreateTokenCopy"), "copy template: {debug}");
        assert!(debug.contains("TokensCreated"), "first-time gate: {debug}");
        assert!(debug.contains("optional: true"), "{debug}");
    }
}

#[test]
fn first_creation_copies_the_attached_permanent_and_later_creations_are_unchanged() {
    use ironsmith::{GameState, PlayerId, Zone};
    use ironsmith::effects::{EffectContext, execute_effect};
    let alice = PlayerId(0);
    let donor_definition = compile_to_runtime_definition("Original body",
        "Type: Creature\nPower/Toughness: 2/3\nFlying", false).unwrap();
    let maker = compile_to_runtime_definition("Make tokens",
        "Type: Sorcery\nCreate two Treasure tokens.", false).unwrap();
    for aura_definition in definitions("Moonlit Meditation", MOONLIT_MEDITATION) {
        let mut game = GameState::new(vec!["Alice".into(), "Bob".into()], 20);
        let donor = game.create_object_from_definition(&donor_definition, alice, Zone::Battlefield);
        let aura = game.create_object_from_definition(&aura_definition, alice, Zone::Battlefield);
        assert!(game.attach_object_to_target(aura, ironsmith::object::AttachmentTarget::Object(donor)));
        for creation in 0..2 {
            let source = game.create_object_from_definition(&maker, alice, Zone::Stack);
            let mut ctx = EffectContext::new_default(source, alice);
            for effect in maker.spell_effect.as_ref().unwrap().flattened_default_effects() {
                execute_effect(&mut game, effect, &mut ctx).unwrap();
            }
            let copies = game.battlefield.iter().filter(|id| game.object(**id).unwrap().name == "Original body").count();
            assert_eq!(copies, 3, "the first two tokens copy the attached permanent");
            let treasures = game.battlefield.iter().filter(|id| game.object(**id).unwrap().subtypes.contains(&ironsmith::Subtype::Treasure)).count();
            assert_eq!(treasures, creation * 2, "later creation retains its original templates");
        }
    }
}
