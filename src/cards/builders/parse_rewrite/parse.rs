#![allow(dead_code)]

use crate::ability::ActivationTiming;
use crate::cards::builders::{
    CardDefinitionBuilder, CardTextError, ParseAnnotations, TextSpan,
};

use super::cst::{
    ActivatedLineCst, MetadataLineCst, RewriteDocumentCst, RewriteLineCst, UnsupportedLineCst,
};
use super::ir::{
    RewriteActivatedLine, RewriteSemanticDocument, RewriteSemanticItem, RewriteUnsupportedLine,
};
use super::leaf::{
    lower_activation_cost_cst, metadata_type_line_cst, parse_activation_cost_rewrite,
};
use super::preprocess::{PreprocessedDocument, preprocess_document};

pub(crate) fn parse_text_with_annotations_rewrite(
    builder: CardDefinitionBuilder,
    text: String,
    allow_unsupported: bool,
) -> Result<(RewriteSemanticDocument, ParseAnnotations), CardTextError> {
    let preprocessed = preprocess_document(builder, text.as_str())?;
    let cst = parse_document_cst(&preprocessed, allow_unsupported)?;
    let semantic = lower_document_cst(preprocessed, cst, allow_unsupported);
    let annotations = semantic.annotations.clone();
    Ok((semantic, annotations))
}

pub(crate) fn parse_document_cst(
    preprocessed: &PreprocessedDocument,
    allow_unsupported: bool,
) -> Result<RewriteDocumentCst, CardTextError> {
    let mut lines = Vec::with_capacity(preprocessed.lines.len());
    for line in &preprocessed.lines {
        let normalized = line.info.normalized.normalized.as_str();
        if let Some((cost_raw, effect_raw)) = normalized.split_once(':') {
            let cost = parse_activation_cost_rewrite(cost_raw)?;
            let colon_start = cost_raw.len();
            let colon_span = TextSpan {
                line: line.info.line_index,
                start: colon_start,
                end: colon_start + 1,
            };
            lines.push(RewriteLineCst::Activated(ActivatedLineCst {
                info: line.info.clone(),
                cost,
                effect_text: effect_raw.trim().to_string(),
                colon_span,
            }));
            continue;
        }

        if allow_unsupported {
            lines.push(RewriteLineCst::Unsupported(UnsupportedLineCst {
                info: line.info.clone(),
                reason_code: "line-family-not-yet-ported",
            }));
            continue;
        }

        return Err(CardTextError::ParseError(format!(
            "rewrite parser does not yet support line family: '{}'",
            line.info.raw_line
        )));
    }

    Ok(RewriteDocumentCst { lines })
}

fn lower_document_cst(
    preprocessed: PreprocessedDocument,
    cst: RewriteDocumentCst,
    allow_unsupported: bool,
) -> RewriteSemanticDocument {
    let mut items = Vec::with_capacity(cst.lines.len());

    for line in cst.lines {
        match line {
            RewriteLineCst::Metadata(MetadataLineCst { .. }) => {}
            RewriteLineCst::Activated(activated) => {
                let cost = lower_activation_cost_cst(&activated.cost)
                    .expect("rewrite activation-cost CST should lower after parsing");
                items.push(RewriteSemanticItem::Activated(RewriteActivatedLine {
                    info: activated.info,
                    cost,
                    effect_text: activated.effect_text,
                    timing_hint: ActivationTiming::AnyTime,
                }));
            }
            RewriteLineCst::Unsupported(unsupported) => {
                items.push(RewriteSemanticItem::Unsupported(RewriteUnsupportedLine {
                    info: unsupported.info,
                    reason_code: unsupported.reason_code,
                }));
            }
        }
    }

    RewriteSemanticDocument {
        builder: preprocessed.builder,
        annotations: preprocessed.annotations,
        items,
        allow_unsupported,
    }
}

pub(crate) fn metadata_line_cst(
    info: crate::cards::builders::LineInfo,
    value: crate::cards::builders::MetadataLine,
) -> Result<MetadataLineCst, CardTextError> {
    Ok(MetadataLineCst {
        info,
        type_line: metadata_type_line_cst(&value)?,
        value,
    })
}
