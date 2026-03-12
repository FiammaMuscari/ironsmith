//! Gemstone Caverns card definition.

use crate::ConditionExpr;
use crate::ability::{Ability, AbilityKind};
use crate::card::CardBuilder;
use crate::cards::CardDefinition;
use crate::cost::TotalCost;
use crate::effect::Effect;
use crate::effects::AddManaOfAnyColorEffect;
use crate::ids::CardId;
use crate::mana::ManaSymbol;
use crate::object::CounterType;
use crate::types::{CardType, Supertype};

/// Creates the Gemstone Caverns card definition.
pub fn gemstone_caverns() -> CardDefinition {
    let card = CardBuilder::new(CardId::new(), "Gemstone Caverns")
        .supertypes(vec![Supertype::Legendary])
        .card_types(vec![CardType::Land])
        .oracle_text(
            "If Gemstone Caverns is in your opening hand and you're not starting the game, you may begin the game with it on the battlefield with a luck counter on it. If you do, exile a card from your hand.\n{T}: Add {C}.\n{T}: Add one mana of any color. Activate only if Gemstone Caverns has a luck counter on it.",
        )
        .build();

    let mut rainbow = Ability::mana_with_effects(
        TotalCost::free(),
        vec![Effect::new(AddManaOfAnyColorEffect::you(1))],
    )
    .with_text("{T}: Add one mana of any color.");
    if let AbilityKind::Activated(activated) = &mut rainbow.kind {
        activated.activation_condition = Some(ConditionExpr::SourceHasCounterAtLeast {
            counter_type: CounterType::Luck,
            count: 1,
        });
    }

    CardDefinition::with_abilities(
        card,
        vec![
            Ability::mana(TotalCost::free(), vec![ManaSymbol::Colorless])
                .with_text("{T}: Add {C}."),
            rainbow,
        ],
    )
}
