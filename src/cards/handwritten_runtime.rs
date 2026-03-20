use crate::ability::Ability;
use crate::card::PowerToughness;
use crate::cards::CardDefinition;
use crate::cards::CardDefinitionBuilder;
use crate::effect::Effect;
use crate::effects::{
    DemonicConsultationEffect, SavinesReclamationEffect, TaintedPactEffect, ThassasOracleEffect,
    YasharnImplacableEarthEffect,
};
use crate::ids::CardId;
use crate::mana::{ManaCost, ManaSymbol};
use crate::static_abilities::StaticAbility;
use crate::target::PlayerFilter;
use crate::triggers::Trigger;
use crate::types::{CardType, Subtype, Supertype};

const CHOSEN_NAME_TAG: &str = "chosen_name";

pub(crate) fn thassas_oracle() -> CardDefinition {
    CardDefinitionBuilder::new(CardId::new(), "Thassa's Oracle")
        .mana_cost(ManaCost::from_pips(vec![
            vec![ManaSymbol::Blue],
            vec![ManaSymbol::Blue],
        ]))
        .card_types(vec![CardType::Creature])
        .subtypes(vec![Subtype::Merfolk, Subtype::Wizard])
        .power_toughness(PowerToughness::fixed(1, 3))
        .oracle_text(
            "Flying\nWhen Thassa's Oracle enters the battlefield, look at the top X cards of your library, where X is your devotion to blue. Put up to one of them on top of your library and the rest on the bottom of your library in a random order. If X is greater than or equal to the number of cards in your library, you win the game.",
        )
        .flying()
        .with_ability(
            Ability::triggered(
                Trigger::this_enters_battlefield(),
                vec![Effect::new(ThassasOracleEffect)],
            )
            .with_text(
                "When Thassa's Oracle enters the battlefield, look at the top X cards of your library, where X is your devotion to blue. Put up to one of them on top of your library and the rest on the bottom of your library in a random order. If X is greater than or equal to the number of cards in your library, you win the game.",
            ),
        )
        .build()
}

pub(crate) fn demonic_consultation() -> CardDefinition {
    CardDefinitionBuilder::new(CardId::new(), "Demonic Consultation")
        .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Black]]))
        .card_types(vec![CardType::Instant])
        .oracle_text(
            "Choose a card name. Exile the top six cards of your library, then reveal cards from the top of your library until you reveal the chosen card. Put that card into your hand and exile all other cards revealed this way.",
        )
        .with_spell_effect(vec![
            Effect::choose_card_name(PlayerFilter::You, None, CHOSEN_NAME_TAG),
            Effect::new(DemonicConsultationEffect::new(CHOSEN_NAME_TAG)),
        ])
        .build()
}

pub(crate) fn tainted_pact() -> CardDefinition {
    CardDefinitionBuilder::new(CardId::new(), "Tainted Pact")
        .mana_cost(ManaCost::from_pips(vec![
            vec![ManaSymbol::Generic(1)],
            vec![ManaSymbol::Black],
        ]))
        .card_types(vec![CardType::Instant])
        .oracle_text(
            "Exile the top card of your library. You may put that card into your hand unless it has the same name as another card exiled this way. Repeat this process until you put a card into your hand or you exile two cards with the same name, whichever comes first.",
        )
        .with_spell_effect(vec![Effect::new(TaintedPactEffect)])
        .build()
}

pub(crate) fn savines_reclamation() -> CardDefinition {
    CardDefinitionBuilder::new(CardId::new(), "Savine's Reclamation")
        .mana_cost(ManaCost::from_pips(vec![
            vec![ManaSymbol::Generic(4)],
            vec![ManaSymbol::White],
        ]))
        .card_types(vec![CardType::Sorcery])
        .oracle_text(
            "Return target permanent card with mana value 3 or less from your graveyard to the battlefield.\nIf this spell was cast from a graveyard, copy this spell and you may choose a new target for the copy.\nFlashback {5}{W}",
        )
        .with_spell_effect(vec![Effect::new(SavinesReclamationEffect::new())])
        .flashback(ManaCost::from_pips(vec![
            vec![ManaSymbol::Generic(5)],
            vec![ManaSymbol::White],
        ]))
        .build()
}

pub(crate) fn krrik_son_of_yawgmoth() -> CardDefinition {
    CardDefinitionBuilder::new(CardId::new(), "K'rrik, Son of Yawgmoth")
        .mana_cost(ManaCost::from_pips(vec![
            vec![ManaSymbol::Generic(4)],
            vec![ManaSymbol::Black],
            vec![ManaSymbol::Black],
            vec![ManaSymbol::Black],
        ]))
        .supertypes(vec![Supertype::Legendary])
        .card_types(vec![CardType::Creature])
        .subtypes(vec![Subtype::Phyrexian, Subtype::Horror, Subtype::Minion])
        .power_toughness(PowerToughness::fixed(2, 2))
        .oracle_text(
            "Lifelink\nFor each {B} in a cost, you may pay 2 life rather than pay that mana.",
        )
        .with_ability(Ability::static_ability(StaticAbility::lifelink()).with_text("Lifelink"))
        .with_ability(
            Ability::static_ability(StaticAbility::krrik_black_mana_may_be_paid_with_life())
                .with_text("For each {B} in a cost, you may pay 2 life rather than pay that mana."),
        )
        .build()
}

pub(crate) fn trinisphere() -> CardDefinition {
    CardDefinitionBuilder::new(CardId::new(), "Trinisphere")
        .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(3)]]))
        .card_types(vec![CardType::Artifact])
        .oracle_text(
            "As long as Trinisphere is untapped, each spell that would cost less than three mana to cast costs three mana to cast.",
        )
        .with_ability(
            Ability::static_ability(StaticAbility::minimum_spell_total_mana(3)).with_text(
                "As long as Trinisphere is untapped, each spell that would cost less than three mana to cast costs three mana to cast.",
            ),
        )
        .build()
}

pub(crate) fn yasharn_implacable_earth() -> CardDefinition {
    CardDefinitionBuilder::new(CardId::new(), "Yasharn, Implacable Earth")
        .mana_cost(ManaCost::from_pips(vec![
            vec![ManaSymbol::Generic(2)],
            vec![ManaSymbol::Green],
            vec![ManaSymbol::White],
        ]))
        .supertypes(vec![Supertype::Legendary])
        .card_types(vec![CardType::Creature])
        .subtypes(vec![Subtype::Elemental, Subtype::Boar])
        .power_toughness(PowerToughness::fixed(4, 4))
        .oracle_text(
            "When Yasharn, Implacable Earth enters the battlefield, search your library for a basic Forest card and a basic Plains card, reveal those cards, put them into your hand, then shuffle.\nPlayers can't pay life or sacrifice nonland permanents to cast spells or activate abilities.",
        )
        .with_ability(
            Ability::triggered(
                Trigger::this_enters_battlefield(),
                vec![Effect::new(YasharnImplacableEarthEffect::new())],
            )
            .with_text(
                "When Yasharn, Implacable Earth enters the battlefield, search your library for a basic Forest card and a basic Plains card, reveal those cards, put them into your hand, then shuffle.",
            ),
        )
        .with_ability(
            Ability::static_ability(
                StaticAbility::cant_pay_life_or_sacrifice_nonland_for_cast_or_activate(),
            )
            .with_text(
                "Players can't pay life or sacrifice nonland permanents to cast spells or activate abilities.",
            ),
        )
        .build()
}
