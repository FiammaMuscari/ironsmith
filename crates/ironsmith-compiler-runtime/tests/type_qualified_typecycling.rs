//! "Artifact landcycling {2}" (Sojourner's Companion): CR 702.29e
//! typecycling whose quality is qualified by a leading card type; the
//! searched card must be both an artifact and a land. Source-authored, unrun.
use ironsmith::ability::AbilityKind;
use ironsmith::{CardType, Zone};

#[path = "p02_line_families/compile.rs"]
mod compile;

const SOJOURNERS_COMPANION: &str = "Mana cost: {7}\nType: Artifact Creature — Salamander\nPower/Toughness: 4/4\nAffinity for artifacts\nArtifact landcycling {2} ({2}, Discard this card: Search your library for an artifact land card, reveal it, put it into your hand, then shuffle.)";

#[test]
fn artifact_landcycling_searches_for_artifact_lands_from_hand() {
    for definition in compile::compile_both("Sojourner's Companion", SOJOURNERS_COMPANION) {
        let cycling = definition
            .abilities
            .iter()
            .find(|ability| {
                matches!(ability.kind, AbilityKind::Activated(_))
                    && ability.functional_zones.contains(&Zone::Hand)
            })
            .expect("landcycling is an activated ability from hand");
        let debug = format!("{cycling:?}");
        assert!(debug.contains("Discard"), "{debug}");
        let AbilityKind::Activated(ability) = &cycling.kind else { unreachable!() };
        let search = ability.effects.all_effects().into_iter()
            .find_map(|effect| effect.downcast_ref::<ironsmith::effects::SearchLibraryEffect>())
            .expect("typecycling searches the library");
        assert_eq!(search.filter.all_card_types, vec![CardType::Artifact, CardType::Land]);
        assert_eq!(search.destination, Zone::Hand);
        assert!(search.reveal);

        // Searching owns its shuffle; it need not be a separate program node.
        let mut game = ironsmith::GameState::new(vec!["Alice".into()], 20);
        let alice = ironsmith::PlayerId::from_index(0);
        let source = game.create_object_from_definition(&definition, alice, Zone::Hand);
        for types in [vec![CardType::Artifact, CardType::Land], vec![CardType::Land]] {
            let card = ironsmith::card::CardBuilder::new(ironsmith::CardId::new(), "Search candidate")
                .card_types(types).build();
            game.create_object_from_card(&card, alice, Zone::Library);
        }
        let before = game.irreversible_random_count();
        let mut context = ironsmith::effects::EffectContext::new_default(source, alice);
        ironsmith::effects::EffectExecutor::execute(search, &mut game, &mut context).unwrap();
        assert_eq!(game.irreversible_random_count(), before + 1);
        assert_eq!(game.player(alice).unwrap().hand.len(), 2);
        assert_eq!(game.player(alice).unwrap().library.len(), 1);
    }
}
