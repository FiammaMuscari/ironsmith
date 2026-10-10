use ironsmith::{GameState, PlayerId, Zone};
use ironsmith::mana_payment::{ManaPaymentRequest, plan_first_mana_payment, last_mana_payment_perf};

// Freeze the source board and compile through the current artifact envelope.
// UI cache files belong to a deployment and may use an older wire format.
#[test]
fn unrelated_channel_reduction_preserves_missing_color_proof() {
    let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../fixtures/mana_color_reachability.json.fixture"
    )).unwrap();
    let load = |slug: &str| {
        let row = rows.iter().find(|row| row["slug"] == slug).unwrap();
        let (artifact, _) = ironsmith_compiler_runtime::compile_to_artifact(
            row["name"].as_str().unwrap(), row["text"].as_str().unwrap(), false,
        ).unwrap();
        ironsmith_runtime_catalog::artifact_materializer::materialize_artifact(&artifact).unwrap()
    };
    let alice = PlayerId::from_index(0);
    let bob = PlayerId::from_index(1);
    let mut game = GameState::new(vec!["Alice".into(), "Bob".into()], 20);
    for (name, owner) in [
        ("boseiju-who-endures", alice), ("botanical-sanctum", alice),
        ("botanical-sanctum", alice), ("nykthos-shrine-to-nyx", alice),
        ("badgermole-cub", alice), ("blood-crypt", bob), ("blightstep-pathway", bob),
    ] {
        game.create_object_from_definition(&load(name), owner, Zone::Battlefield);
    }
    let source = game.create_object_from_definition(&load("atraxa-grand-unifier"), alice, Zone::Hand);
    game.refresh_continuous_state().unwrap();
    let request = ManaPaymentRequest::new(alice, source,
        ironsmith::costs::PaymentReason::CastSpell,
        game.object(source).unwrap().mana_cost_owned().unwrap());
    assert!(plan_first_mana_payment(&game, &request).is_err());
    assert_eq!(last_mana_payment_perf().visited_nodes, 0,
        "missing white/black is proven before activation enumeration");
}
