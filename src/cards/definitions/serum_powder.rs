//! Serum Powder card definition.

use crate::ability::Ability;
use crate::card::CardBuilder;
use crate::cards::CardDefinition;
use crate::cost::TotalCost;
use crate::ids::CardId;
use crate::mana::{ManaCost, ManaSymbol};
use crate::types::CardType;

/// Creates the Serum Powder card definition.
pub fn serum_powder() -> CardDefinition {
    let card = CardBuilder::new(CardId::new(), "Serum Powder")
        .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(3)]]))
        .card_types(vec![CardType::Artifact])
        .oracle_text(
            "Any time you could mulligan and Serum Powder is in your hand, you may exile all the cards from your hand, then draw that many cards. (You can do this in addition to taking mulligans.)\n{T}: Add {C}.",
        )
        .build();

    CardDefinition::with_abilities(
        card,
        vec![
            Ability::mana(TotalCost::free(), vec![ManaSymbol::Colorless])
                .with_text("{T}: Add {C}."),
        ],
    )
}
