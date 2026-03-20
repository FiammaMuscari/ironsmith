#![allow(dead_code)]

use crate::cards::builders::{
    CardDefinitionBuilder, CardTextError, LineInfo, NormalizedLine, OwnedLexToken,
    ParseAnnotations, is_ignorable_unparsed_line, lex_line, normalize_line_for_parse,
    parse_metadata_line, parse_single_word_keyword_action,
};

#[derive(Debug, Clone)]
pub(crate) struct PreprocessedDocument {
    pub(crate) builder: CardDefinitionBuilder,
    pub(crate) annotations: ParseAnnotations,
    pub(crate) lines: Vec<PreprocessedLine>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreprocessedLine {
    pub(crate) info: LineInfo,
    pub(crate) tokens: Vec<OwnedLexToken>,
}

pub(crate) fn preprocess_document(
    mut builder: CardDefinitionBuilder,
    text: &str,
) -> Result<PreprocessedDocument, CardTextError> {
    fn normalize_card_name_for_self_reference(name: &str) -> String {
        let lower = name.to_ascii_lowercase();
        let bytes = lower.as_bytes();
        if bytes.len() > 2 && bytes[1] == b'-' && bytes[0].is_ascii_alphabetic() {
            lower[2..].to_string()
        } else {
            lower
        }
    }

    fn short_name_for_self_reference(name: &str) -> String {
        fn is_reserved_short_alias(alias_lower: &str) -> bool {
            matches!(
                alias_lower,
                "a" | "an"
                    | "the"
                    | "one"
                    | "two"
                    | "three"
                    | "four"
                    | "five"
                    | "six"
                    | "seven"
                    | "eight"
                    | "nine"
                    | "ten"
                    | "x"
                    | "this"
                    | "that"
                    | "these"
                    | "those"
                    | "you"
                    | "your"
                    | "when"
                    | "whenever"
                    | "if"
                    | "at"
                    | "add"
                    | "move"
                    | "deal"
                    | "draw"
                    | "counter"
                    | "destroy"
                    | "exile"
                    | "untap"
                    | "scry"
                    | "discard"
                    | "transform"
                    | "regenerate"
                    | "mill"
                    | "get"
                    | "reveal"
                    | "look"
                    | "lose"
                    | "gain"
                    | "put"
                    | "sacrifice"
                    | "create"
                    | "investigate"
                    | "attach"
                    | "remove"
                    | "return"
                    | "exchange"
                    | "become"
                    | "switch"
                    | "skip"
                    | "surveil"
                    | "shuffle"
                    | "reorder"
                    | "pay"
                    | "goad"
                    | "power"
                    | "toughness"
                    | "mana"
                    | "life"
                    | "commander"
                    | "player"
                    | "opponent"
                    | "creature"
                    | "artifact"
                    | "enchantment"
                    | "land"
                    | "spell"
                    | "card"
                    | "token"
                    | "permanent"
                    | "library"
                    | "graveyard"
                    | "hand"
                    | "battlefield"
                    | "controller"
                    | "owner"
                    | "planeswalker"
                    | "battle"
                    | "equipment"
                    | "aura"
            ) || parse_single_word_keyword_action(alias_lower).is_some()
        }

        let trimmed = name.trim();
        let comma_short = trimmed.split(',').next().unwrap_or(trimmed).trim();
        if comma_short != trimmed {
            return comma_short.to_string();
        }

        let mut words = trimmed.split_whitespace();
        let Some(first_word) = words.next() else {
            return trimmed.to_string();
        };
        if words.next().is_none() {
            return trimmed.to_string();
        }

        let alias = first_word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-');
        if alias.len() <= 2 {
            return trimmed.to_string();
        }

        let alias_lower = alias.to_ascii_lowercase();
        if is_reserved_short_alias(alias_lower.as_str()) {
            return trimmed.to_string();
        }

        alias.to_string()
    }

    fn normalize_non_metadata_line(
        raw_line: &str,
        line_index: usize,
        full_name: &str,
        short_name: &str,
        annotations: &mut ParseAnnotations,
    ) -> Result<Option<PreprocessedLine>, CardTextError> {
        let Some(normalized) = normalize_line_for_parse(raw_line, full_name, short_name) else {
            if is_ignorable_unparsed_line(raw_line) {
                return Ok(None);
            }
            return Err(CardTextError::ParseError(format!(
                "rewrite preprocessing could not normalize line: '{raw_line}'"
            )));
        };

        annotations.record_original_line(line_index, &normalized.original);
        annotations.record_normalized_line(line_index, &normalized.normalized);
        annotations.record_char_map(line_index, normalized.char_map.clone());

        let tokens = lex_line(normalized.normalized.as_str(), line_index)?;
        Ok(Some(PreprocessedLine {
            info: LineInfo {
                line_index,
                raw_line: raw_line.trim().to_string(),
                normalized,
            },
            tokens,
        }))
    }

    let card_name = builder.card_builder.name_ref().to_string();
    let front_face_name = card_name
        .split("//")
        .next()
        .unwrap_or(card_name.as_str())
        .trim()
        .to_string();
    let short_name = short_name_for_self_reference(front_face_name.as_str());
    let full_lower = normalize_card_name_for_self_reference(front_face_name.as_str());
    let short_lower = normalize_card_name_for_self_reference(short_name.as_str());
    let mut annotations = ParseAnnotations::default();
    let mut lines = Vec::new();

    for (line_index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(meta) = parse_metadata_line(line)? {
            builder = builder.apply_metadata(meta)?;
            continue;
        }

        if let Some(parsed_line) = normalize_non_metadata_line(
            line,
            line_index,
            full_lower.as_str(),
            short_lower.as_str(),
            &mut annotations,
        )? {
            lines.push(parsed_line);
        }
    }

    if !lines.is_empty() {
        let oracle_text = lines
            .iter()
            .map(|line| line.info.raw_line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        builder = builder.oracle_text(oracle_text);
    }

    Ok(PreprocessedDocument {
        builder,
        annotations,
        lines,
    })
}

pub(crate) fn make_line_info(
    line_index: usize,
    raw_line: impl Into<String>,
    normalized: NormalizedLine,
) -> LineInfo {
    LineInfo {
        line_index,
        raw_line: raw_line.into(),
        normalized,
    }
}
