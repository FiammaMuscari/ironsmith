use std::fmt::Write;

use winnow::ascii::{digit1, multispace0, multispace1};
use winnow::combinator::{alt, delimited, opt, preceded, repeat, separated, terminated};
use winnow::error::{ContextError, Result as WResult};
use winnow::prelude::*;
use winnow::token::{one_of, take_while};

use crate::cards::builders::{
    CardTextError, MetadataLine, parse_card_type, parse_subtype_word, parse_supertype_word,
};
use crate::cost::TotalCost;
use crate::costs::Cost;
use crate::filter::ObjectFilter;
use crate::mana::{ManaCost, ManaSymbol};
use crate::types::{CardType, Subtype, Supertype};

#[derive(Debug, Clone)]
pub(crate) struct TypeLineCst {
    pub(crate) supertypes: Vec<Supertype>,
    pub(crate) card_types: Vec<CardType>,
    pub(crate) subtypes: Vec<Subtype>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActivationCostCst {
    pub(crate) raw: String,
    pub(crate) segments: Vec<ActivationCostSegmentCst>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ActivationCostSegmentCst {
    Mana(ManaCost),
    Tap,
    Life(u32),
    DiscardSource,
    DiscardCard(u32),
    SacrificeSelf,
    SacrificeCreature,
}

fn parse_word<'a>(input: &mut &'a str) -> WResult<&'a str> {
    take_while(1.., |ch: char| ch.is_ascii_alphabetic() || ch == '\'' || ch == '-')
        .parse_next(input)
}

fn spaced<'a, O, P>(parser: P) -> impl Parser<&'a str, O, ContextError>
where
    P: Parser<&'a str, O, ContextError>,
{
    delimited(multispace0, parser, multispace0)
}

fn count_word_value(word: &str) -> Option<u32> {
    match word.to_ascii_lowercase().as_str() {
        "a" | "an" | "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        _ => None,
    }
}

fn parse_count_inner(input: &mut &str) -> WResult<u32> {
    alt((
        digit1.try_map(str::parse::<u32>),
        parse_word.verify_map(count_word_value),
    ))
    .parse_next(input)
}

fn parse_mana_symbol_inner(input: &mut &str) -> WResult<ManaSymbol> {
    alt((
        digit1.try_map(|digits: &str| digits.parse::<u8>().map(ManaSymbol::Generic)),
        one_of(['W', 'w', 'U', 'u', 'B', 'b', 'R', 'r', 'G', 'g', 'C', 'c', 'S', 's', 'X', 'x', 'P', 'p'])
            .map(|ch: char| match ch.to_ascii_uppercase() {
                'W' => ManaSymbol::White,
                'U' => ManaSymbol::Blue,
                'B' => ManaSymbol::Black,
                'R' => ManaSymbol::Red,
                'G' => ManaSymbol::Green,
                'C' => ManaSymbol::Colorless,
                'S' => ManaSymbol::Snow,
                'X' => ManaSymbol::X,
                'P' => ManaSymbol::Life(2),
                _ => unreachable!("one_of constrains supported mana-symbol letters"),
            }),
    ))
    .parse_next(input)
}

fn parse_mana_group_inner(input: &mut &str) -> WResult<Vec<ManaSymbol>> {
    delimited(
        spaced("{"),
        separated(1.., parse_mana_symbol_inner, spaced('/')),
        spaced("}"),
    )
    .parse_next(input)
}

fn parse_mana_cost_inner(input: &mut &str) -> WResult<ManaCost> {
    repeat(1.., parse_mana_group_inner)
        .map(ManaCost::from_pips)
        .parse_next(input)
}

fn parse_discard_segment_inner(input: &mut &str) -> WResult<ActivationCostSegmentCst> {
    preceded(
        spaced("discard"),
        alt((
            spaced("this card").value(ActivationCostSegmentCst::DiscardSource),
            (
                opt(terminated(parse_count_inner, multispace1)),
                opt(spaced(alt(("a", "an")))),
                spaced(alt(("card", "cards"))),
            )
                .map(|(count, _article, _)| {
                    ActivationCostSegmentCst::DiscardCard(count.unwrap_or(1))
                }),
        )),
    )
    .parse_next(input)
}

fn parse_pay_life_segment_inner(input: &mut &str) -> WResult<ActivationCostSegmentCst> {
    "pay".parse_next(input)?;
    multispace1.parse_next(input)?;
    let amount = parse_count_inner.parse_next(input)?;
    multispace1.parse_next(input)?;
    alt(("life", "lives")).parse_next(input)?;
    Ok(ActivationCostSegmentCst::Life(amount))
}

fn parse_sacrifice_segment_inner(input: &mut &str) -> WResult<ActivationCostSegmentCst> {
    preceded(
        spaced("sacrifice"),
        alt((
            spaced(alt((
                "this creature",
                "this artifact",
                "this enchantment",
                "this land",
                "this permanent",
                "this card",
            )))
            .value(ActivationCostSegmentCst::SacrificeSelf),
            spaced(alt(("a creature", "another creature")))
                .value(ActivationCostSegmentCst::SacrificeCreature),
        )),
    )
    .parse_next(input)
}

fn parse_tap_segment_inner(input: &mut &str) -> WResult<ActivationCostSegmentCst> {
    alt((spaced("{t}"), spaced("t")))
        .value(ActivationCostSegmentCst::Tap)
        .parse_next(input)
}

fn parse_activation_cost_segment_inner(input: &mut &str) -> WResult<ActivationCostSegmentCst> {
    alt((
        parse_tap_segment_inner,
        parse_pay_life_segment_inner,
        parse_discard_segment_inner,
        parse_sacrifice_segment_inner,
        parse_mana_cost_inner.map(ActivationCostSegmentCst::Mana),
    ))
    .parse_next(input)
}

fn finish_parse<'a, O>(
    raw: &'a str,
    mut parser: impl Parser<&'a str, O, ContextError>,
    label: &str,
) -> Result<O, CardTextError> {
    let mut input = raw.trim();
    let parsed = parser
        .parse_next(&mut input)
        .map_err(|err| CardTextError::ParseError(format!("rewrite {label} parse failed: {err}")))?;
    if !input.trim().is_empty() {
        return Err(CardTextError::ParseError(format!(
            "rewrite {label} parser left trailing input: '{}'",
            input.trim()
        )));
    }
    Ok(parsed)
}

pub(crate) fn parse_count_word_rewrite(raw: &str) -> Result<u32, CardTextError> {
    finish_parse(raw, spaced(parse_count_inner), "count-word")
}

pub(crate) fn parse_mana_symbol_group_rewrite(raw: &str) -> Result<Vec<ManaSymbol>, CardTextError> {
    let trimmed = raw.trim().trim_matches('{').trim_matches('}');
    finish_parse(trimmed, separated(1.., parse_mana_symbol_inner, spaced('/')), "mana-group")
}

pub(crate) fn parse_mana_cost_rewrite(raw: &str) -> Result<ManaCost, CardTextError> {
    finish_parse(raw, spaced(parse_mana_cost_inner), "mana-cost")
}

pub(crate) fn parse_type_line_rewrite(raw: &str) -> Result<TypeLineCst, CardTextError> {
    let normalized = raw.trim();
    let mut emdash_parts = normalized.splitn(2, '—');
    let left = emdash_parts.next().unwrap_or("").trim();
    let right = emdash_parts.next().unwrap_or("").trim();

    let left_words = finish_parse(
        left,
        separated::<_, _, Vec<_>, _, _, _, _>(1.., parse_word, multispace1),
        "type-line-left",
    )?;
    let right_words: Vec<&str> = if right.is_empty() {
        Vec::new()
    } else {
        finish_parse(
            right,
            separated::<_, _, Vec<_>, _, _, _, _>(1.., parse_word, multispace1),
            "type-line-right",
        )?
    };

    let mut supertypes = Vec::new();
    let mut card_types = Vec::new();
    for word in left_words {
        if let Some(supertype) = parse_supertype_word(word) {
            supertypes.push(supertype);
            continue;
        }
        if let Some(card_type) = parse_card_type(&word.to_ascii_lowercase()) {
            card_types.push(card_type);
        }
    }

    let mut subtypes = Vec::new();
    for word in right_words {
        if let Some(subtype) = parse_subtype_word(word) {
            subtypes.push(subtype);
        }
    }

    Ok(TypeLineCst {
        supertypes,
        card_types,
        subtypes,
    })
}

pub(crate) fn parse_activation_cost_rewrite(raw: &str) -> Result<ActivationCostCst, CardTextError> {
    let mut segments = Vec::new();
    for raw_segment in raw.split(',') {
        let segment = raw_segment.trim().trim_start_matches("and ").trim();
        if segment.is_empty() {
            continue;
        }
        let normalized_segment = segment.to_ascii_lowercase();
        segments.push(finish_parse(
            normalized_segment.as_str(),
            spaced(parse_activation_cost_segment_inner),
            "activation-cost-segment",
        )?);
    }

    if segments.is_empty() {
        return Err(CardTextError::ParseError(
            "rewrite activation-cost parser found no segments".to_string(),
        ));
    }

    Ok(ActivationCostCst {
        raw: raw.trim().to_string(),
        segments,
    })
}

pub(crate) fn lower_activation_cost_cst(cst: &ActivationCostCst) -> Result<TotalCost, CardTextError> {
    let mut costs = Vec::new();
    for segment in &cst.segments {
        match segment {
            ActivationCostSegmentCst::Mana(cost) => costs.push(Cost::mana(cost.clone())),
            ActivationCostSegmentCst::Tap => costs.push(Cost::tap()),
            ActivationCostSegmentCst::Life(amount) => costs.push(Cost::life(*amount)),
            ActivationCostSegmentCst::DiscardSource => costs.push(Cost::discard_source()),
            ActivationCostSegmentCst::DiscardCard(count) => {
                costs.push(Cost::discard(*count, None));
            }
            ActivationCostSegmentCst::SacrificeSelf => costs.push(Cost::sacrifice_self()),
            ActivationCostSegmentCst::SacrificeCreature => {
                costs.push(Cost::sacrifice(ObjectFilter::creature().you_control()));
            }
        }
    }
    Ok(TotalCost::from_costs(costs))
}

#[allow(dead_code)]
pub(crate) fn metadata_type_line_cst(value: &MetadataLine) -> Result<Option<TypeLineCst>, CardTextError> {
    match value {
        MetadataLine::TypeLine(raw) => parse_type_line_rewrite(raw).map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn display_activation_cost_segments(cst: &ActivationCostCst) -> String {
    let mut rendered = String::new();
    for (idx, segment) in cst.segments.iter().enumerate() {
        if idx > 0 {
            rendered.push_str(", ");
        }
        match segment {
            ActivationCostSegmentCst::Mana(cost) => rendered.push_str(&cost.to_oracle()),
            ActivationCostSegmentCst::Tap => rendered.push_str("{T}"),
            ActivationCostSegmentCst::Life(amount) => {
                let _ = write!(&mut rendered, "Pay {amount} life");
            }
            ActivationCostSegmentCst::DiscardSource => rendered.push_str("Discard this card"),
            ActivationCostSegmentCst::DiscardCard(count) => {
                let _ = write!(&mut rendered, "Discard {count} card");
                if *count != 1 {
                    rendered.push('s');
                }
            }
            ActivationCostSegmentCst::SacrificeSelf => rendered.push_str("Sacrifice this permanent"),
            ActivationCostSegmentCst::SacrificeCreature => rendered.push_str("Sacrifice a creature"),
        }
    }
    rendered
}
