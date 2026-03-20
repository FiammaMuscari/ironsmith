use crate::cards::builders::{CardDefinitionBuilder, CardTextError, RewriteSemanticItem};
use crate::ids::CardId;
use crate::mana::ManaSymbol;
use crate::types::{CardType, Subtype, Supertype};

use super::{
    assert_activation_cost_parity, assert_mana_cost_parity, lex_line,
    lower_activation_cost_cst, parse_activation_cost_rewrite, parse_count_word_rewrite,
    parse_mana_symbol_group_rewrite, parse_text_with_annotations_rewrite,
    parse_type_line_rewrite,
};

#[test]
fn rewrite_lexer_tracks_spans_for_activation_lines() {
    let tokens = lex_line("{T}, Sacrifice a creature: Add {B}{B}.", 3)
        .expect("rewrite lexer should classify activation line");
    assert_eq!(tokens[0].slice, "{T}");
    assert_eq!(tokens[0].span.line, 3);
    assert_eq!(tokens[0].span.start, 0);
    assert_eq!(tokens[0].span.end, 3);
    assert!(tokens.iter().any(|token| token.slice == ":"));
}

#[test]
fn rewrite_count_word_parser_handles_digits_and_words() {
    assert_eq!(parse_count_word_rewrite("2").expect("digit count"), 2);
    assert_eq!(parse_count_word_rewrite("three").expect("word count"), 3);
}

#[test]
fn rewrite_mana_symbol_group_parser_handles_hybrid_symbols() {
    let symbols = parse_mana_symbol_group_rewrite("{W/U}")
        .expect("rewrite parser should parse hybrid mana group");
    assert_eq!(symbols, vec![ManaSymbol::White, ManaSymbol::Blue]);
}

#[test]
fn rewrite_type_line_parser_handles_supertypes_types_and_subtypes() {
    let parsed = parse_type_line_rewrite("Legendary Creature — Elf Druid")
        .expect("rewrite type-line parser should succeed");
    assert_eq!(parsed.supertypes, vec![Supertype::Legendary]);
    assert_eq!(parsed.card_types, vec![CardType::Creature]);
    assert_eq!(parsed.subtypes, vec![Subtype::Elf, Subtype::Druid]);
}

#[test]
fn rewrite_mana_cost_matches_legacy_parser() {
    assert_mana_cost_parity("{2}{W/U}{B/P}{X}")
        .expect("rewrite mana-cost parser should match legacy semantics");
}

#[test]
fn rewrite_activation_cost_matches_legacy_for_common_segments() {
    for raw in [
        "{T}",
        "{1}{G}, {T}",
        "Pay 2 life",
        "Discard this card",
        "Discard a card",
    ] {
        assert_activation_cost_parity(raw)
            .unwrap_or_else(|err| panic!("expected activation-cost parity for '{raw}': {err}"));
    }
}

#[test]
fn rewrite_activation_cost_parses_sacrifice_segments() {
    let cst = parse_activation_cost_rewrite("Sacrifice a creature")
        .expect("rewrite activation-cost parser should parse sacrifice segments");
    let lowered =
        lower_activation_cost_cst(&cst).expect("rewrite sacrifice segment should lower to TotalCost");
    assert!(!lowered.is_free());
}

#[test]
fn rewrite_entrypoint_collects_metadata_annotations_and_activated_lines() -> Result<(), CardTextError>
{
    let builder = CardDefinitionBuilder::new(CardId::new(), "Llanowar Elves");
    let text = "Mana cost: {G}\nType: Creature — Elf Druid\n{T}: Add {G}.";
    let (doc, annotations) = parse_text_with_annotations_rewrite(builder, text.to_string(), false)?;

    assert_eq!(doc.items.len(), 1);
    let RewriteSemanticItem::Activated(activated) = &doc.items[0] else {
        panic!("expected activated item");
    };
    assert_eq!(activated.cost.display(), "{T}");
    assert_eq!(activated.effect_text, "add {g}.");
    assert!(annotations.normalized_lines.contains_key(&2));

    let built = doc.builder.clone().build();
    assert_eq!(built.card.mana_cost.expect("mana cost").to_oracle(), "{G}");
    assert!(built.card.card_types.contains(&CardType::Creature));
    Ok(())
}

#[test]
fn rewrite_entrypoint_preserves_unsupported_lines_in_allow_unsupported_mode() -> Result<(), CardTextError>
{
    let builder = CardDefinitionBuilder::new(CardId::new(), "Wall of Omens");
    let text = "Flying\nWhen this creature enters the battlefield, draw a card.";
    let (doc, _) = parse_text_with_annotations_rewrite(builder, text.to_string(), true)?;

    assert_eq!(doc.items.len(), 2);
    assert!(matches!(doc.items[0], RewriteSemanticItem::Unsupported(_)));
    assert!(matches!(doc.items[1], RewriteSemanticItem::Unsupported(_)));
    Ok(())
}
