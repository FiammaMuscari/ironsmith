#![allow(dead_code)]

use crate::ability::ActivationTiming;
use crate::cards::builders::{CardDefinitionBuilder, LineInfo, ParseAnnotations, TotalCost};

#[derive(Debug, Clone)]
pub(crate) struct RewriteSemanticDocument {
    pub(crate) builder: CardDefinitionBuilder,
    pub(crate) annotations: ParseAnnotations,
    pub(crate) items: Vec<RewriteSemanticItem>,
    pub(crate) allow_unsupported: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum RewriteSemanticItem {
    Activated(RewriteActivatedLine),
    Unsupported(RewriteUnsupportedLine),
}

#[derive(Debug, Clone)]
pub(crate) struct RewriteActivatedLine {
    pub(crate) info: LineInfo,
    pub(crate) cost: TotalCost,
    pub(crate) effect_text: String,
    pub(crate) timing_hint: ActivationTiming,
}

#[derive(Debug, Clone)]
pub(crate) struct RewriteUnsupportedLine {
    pub(crate) info: LineInfo,
    pub(crate) reason_code: &'static str,
}
