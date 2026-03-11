//! Human Frailty card definition.

use crate::cards::{CardDefinition, CardDefinitionBuilder};
use crate::effect::Effect;
use crate::ids::CardId;
use crate::mana::{ManaCost, ManaSymbol};
use crate::target::{ChooseSpec, ObjectFilter};
use crate::types::{CardType, Subtype};

/// Human Frailty - {B}
/// Instant
/// Destroy target Human creature.
pub fn human_frailty() -> CardDefinition {
    let target = ChooseSpec::target(ChooseSpec::Object(
        ObjectFilter::creature().with_subtype(Subtype::Human),
    ));

    CardDefinitionBuilder::new(CardId::new(), "Human Frailty")
        .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Black]]))
        .card_types(vec![CardType::Instant])
        .with_spell_effect(vec![Effect::destroy(target)])
        .oracle_text("Destroy target Human creature.")
        .build()
}
