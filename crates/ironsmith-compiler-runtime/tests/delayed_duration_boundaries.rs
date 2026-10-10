use ironsmith::{CardId, CardType, GameState, PlayerId, Zone};
use ironsmith::card::CardBuilder;
use ironsmith::effect::Effect;
use ironsmith::effects::{EffectExecutor, EffectContext, ScheduleDelayedTriggerEffect};
use ironsmith::target::{ObjectFilter, PlayerFilter};
use ironsmith::triggers::{Trigger, TriggerEvent, check_delayed_triggers};

#[test]
fn delayed_next_turn_end_tracks_the_actual_controllers_turn() {
    let alice = PlayerId::from_index(0);
    let bob = PlayerId::from_index(1);
    for extra_controller in [alice, bob] {
        let mut game = GameState::new(vec!["Alice".into(), "Bob".into()], 20);
        game.turn.active_player = alice;
        let card = CardBuilder::new(CardId::new(), "Duration witness")
            .card_types(vec![CardType::Land]).build();
        let land = game.create_object_from_card(&card, alice, Zone::Battlefield);
        let effect = ScheduleDelayedTriggerEffect::new(
            Trigger::player_plays_land(PlayerFilter::You, ObjectFilter::land()),
            vec![Effect::draw(1)], false, Vec::new(), PlayerFilter::You,
        ).until_controller_next_turn_end();
        effect.execute(&mut game, &mut EffectContext::new_default(land, alice)).unwrap();
        let fires = |game: &mut GameState| {
            let event = TriggerEvent::new_with_provenance(
                ironsmith::events::LandPlayedEvent::with_current_snapshot(
                    land, alice, Zone::Hand, Zone::Battlefield, game).unwrap(),
                Default::default(),
            );
            check_delayed_triggers(game, &event).len()
        };
        assert_eq!(fires(&mut game), 1);
        game.turn_store.extra_turns.push(extra_controller);
        game.next_turn();
        assert_eq!(game.turn.active_player, extra_controller);
        assert_eq!(fires(&mut game), 1);
        while game.turn.active_player != alice {
            game.next_turn();
            assert_eq!(fires(&mut game), 1);
        }
        assert_eq!(game.effect_store.delayed_triggers[0].expires_at_turn, Some(game.turn.turn_number));
        game.next_turn();
        assert_eq!(fires(&mut game), 0);
        assert!(game.effect_store.delayed_triggers.is_empty());
    }
}

#[test]
fn next_turn_end_duration_survives_compilation_and_artifact_roundtrip() {
    let (artifact, definition) = ironsmith_compiler_runtime::compile_to_artifact(
        "Duration witness", "Type: Sorcery\nUntil the end of your next turn, whenever you play a land, draw a card.", false,
    ).unwrap();
    let decoded = ironsmith_compiled_artifact::CompiledCardArtifact::from_json(&artifact.to_json().unwrap()).unwrap();
    let restored = ironsmith_runtime_catalog::artifact_materializer::materialize_artifact(&decoded).unwrap();
    for definition in [definition, restored] {
        let effects = definition.spell_effect.as_ref().unwrap().all_effects_owned();
        let schedule = effects.iter().find_map(|effect| effect.downcast_ref::<ScheduleDelayedTriggerEffect>()).unwrap();
        assert_eq!(schedule.duration, ironsmith_core::DelayedTriggerDuration::UntilControllerNextTurnEnd);
        assert!(!schedule.one_shot);
    }
}
