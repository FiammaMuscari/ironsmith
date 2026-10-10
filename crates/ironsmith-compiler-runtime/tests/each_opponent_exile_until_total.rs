//! "Each opponent exiles cards from the top of their library until they have
//! exiled cards with total mana value N or greater [this way]": a per-opponent
//! consult (each opponent in turn, CR 101.4) with the cumulative mana-value
//! stop. Source-authored, deliberately UNRUN.
#[path = "cf8_p10_support/mod.rs"]
mod support;

const TASHAS_HIDEOUS_LAUGHTER: &str = "Mana cost: {1}{U}{B}\nType: Sorcery\nEach opponent exiles cards from the top of their library until that player has exiled cards with total mana value 20 or greater.";
const DREAM_HARVEST: &str = "Mana cost: {5}{U}{B}\nType: Sorcery\nEach opponent exiles cards from the top of their library until they have exiled cards with total mana value 5 or greater this way. Until end of turn, you may cast cards exiled this way without paying their mana costs.";

fn assert_opponent_consult(definition: &ironsmith::cards::CardDefinition, total: i32) -> ironsmith::tag::TagKey {
    use ironsmith::effects::{ForPlayersEffect, ConsultTopOfLibraryEffect, ConsultTopOfLibraryStopRule};
    use ironsmith::target::PlayerFilter;
    let effects = definition.spell_effect.as_ref().unwrap().flattened_default_effects();
    let players = effects.iter().find_map(|effect| effect.downcast_ref::<ForPlayersEffect>()).unwrap();
    assert_eq!(players.filter, PlayerFilter::Opponent);
    let consult = players.effects.iter().find_map(|effect|
        effect.downcast_ref::<ConsultTopOfLibraryEffect>()).unwrap();
    assert_eq!(consult.player, PlayerFilter::IteratedPlayer);
    assert_eq!(consult.mode, ironsmith_core::LibraryConsultMode::Exile);
    assert_eq!(consult.stop_rule, ConsultTopOfLibraryStopRule::TotalManaValue(ironsmith::effect::Value::Fixed(total)));
    consult.all_tag.clone()
}

#[test]
fn tashas_hideous_laughter_runs_one_total_mana_value_consult_per_opponent() {
    for definition in support::definitions("Tasha's Hideous Laughter", TASHAS_HIDEOUS_LAUGHTER) {
        let debug = format!("{:?}", definition.spell_effect);
        assert_opponent_consult(&definition, 20);
        assert!(debug.contains("Exile"), "{debug}");
        assert!(
            debug.contains("ForPlayers") || debug.contains("ForEachOpponent") || debug.contains("Opponent"),
            "one consult per opponent: {debug}"
        );
    }
}

#[test]
fn dream_harvest_consults_each_opponent_then_grants_free_casts_of_the_exiled_cards() {
    for definition in support::definitions("Dream Harvest", DREAM_HARVEST) {
        let tag = assert_opponent_consult(&definition, 5);
        let effects = definition.spell_effect.as_ref().unwrap().flattened_default_effects();
        let free = effects.iter().find_map(|effect|
            effect.downcast_ref::<ironsmith::effects::GrantTaggedSpellFreeCastUntilEndOfTurnEffect>()).unwrap();
        assert_eq!(free.tag, tag);
        assert_eq!(free.player, ironsmith::target::PlayerFilter::You);
        assert_eq!(free.zone, Some(ironsmith::Zone::Exile));
        assert_eq!(free.duration, ironsmith_core::GrantPlayTaggedDuration::UntilEndOfTurn);
    }
}

const FEVERED_SUSPICION: &str = "Mana cost: {4}{B}{R}\nType: Sorcery\nEach opponent exiles cards from the top of their library until they exile a nonland card. You may cast any number of spells from among those nonland cards without paying their mana costs.\nRebound (If you cast this spell from your hand, exile it as it resolves. At the beginning of your next upkeep, you may cast this card from exile without paying its mana cost.)";

#[test]
fn fevered_suspicion_casts_freely_from_every_opponents_nonland_card() {
    for definition in support::definitions("Fevered Suspicion", FEVERED_SUSPICION) {
        let debug = format!("{:?}", definition.spell_effect);
        assert!(debug.contains("ForPlayers") || debug.contains("Opponent"), "{debug}");
        assert!(debug.contains("without_paying_mana_cost: true"), "{debug}");
        assert!(debug.contains("Exile"), "{debug}");
    }
}

#[test]
fn dream_harvest_grants_free_casts_of_every_card_exiled_this_way_until_end_of_turn() {
    for definition in support::definitions("Dream Harvest", DREAM_HARVEST) {
        let debug = format!("{:?}", definition.spell_effect);
        assert!(debug.contains("UntilEndOfTurn") || debug.contains("EndOfTurn"), "{debug}");
    }
}

const PLARGG_AND_NASSARI: &str = "Mana cost: {3}{R}{R}\nType: Legendary Creature — Demon Goblin\nPower/Toughness: 5/5\nAt the beginning of your upkeep, each player exiles cards from the top of their library until they exile a nonland card. An opponent chooses a nonland card exiled this way. You may cast up to two spells from among the other cards exiled this way without paying their mana costs.";

#[test]
fn plargg_and_nassari_lets_an_opponent_exclude_one_card_then_casts_up_to_two_others() {
    for definition in support::definitions("Plargg and Nassari", PLARGG_AND_NASSARI) {
        let debug = format!("{:?}", definition.abilities);
        assert!(debug.contains("IsNotTaggedObject"), "the opponent's pick is excluded: {debug}");
        assert!(debug.contains("without_paying_mana_cost: true"), "{debug}");
        assert!(debug.contains("Opponent"), "{debug}");
    }
}
