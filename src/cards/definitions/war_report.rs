//! War Report card definition.

use crate::cards::{CardDefinition, CardDefinitionBuilder};
use crate::effect::{Effect, Value};
use crate::ids::CardId;
use crate::mana::{ManaCost, ManaSymbol};
use crate::target::ObjectFilter;
use crate::types::CardType;

/// War Report - {3}{W}
/// Instant
/// You gain life equal to the number of creatures on the battlefield plus the
/// number of artifacts on the battlefield.
pub fn war_report() -> CardDefinition {
    let amount = Value::Add(
        Box::new(Value::Count(ObjectFilter::creature())),
        Box::new(Value::Count(ObjectFilter::artifact())),
    );

    CardDefinitionBuilder::new(CardId::new(), "War Report")
        .mana_cost(ManaCost::from_pips(vec![
            vec![ManaSymbol::Generic(3)],
            vec![ManaSymbol::White],
        ]))
        .card_types(vec![CardType::Instant])
        .with_spell_effect(vec![Effect::gain_life(amount)])
        .oracle_text(
            "You gain life equal to the number of creatures on the battlefield plus the number of artifacts on the battlefield.",
        )
        .build()
}
