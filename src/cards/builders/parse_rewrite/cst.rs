#![allow(dead_code)]

use crate::cards::builders::{LineInfo, MetadataLine, TextSpan};

use super::leaf::{ActivationCostCst, TypeLineCst};

#[derive(Debug, Clone)]
pub(crate) struct RewriteDocumentCst {
    pub(crate) lines: Vec<RewriteLineCst>,
}

#[derive(Debug, Clone)]
pub(crate) enum RewriteLineCst {
    Metadata(MetadataLineCst),
    Activated(ActivatedLineCst),
    Unsupported(UnsupportedLineCst),
}

#[derive(Debug, Clone)]
pub(crate) struct MetadataLineCst {
    pub(crate) info: LineInfo,
    pub(crate) value: MetadataLine,
    pub(crate) type_line: Option<TypeLineCst>,
}

#[derive(Debug, Clone)]
pub(crate) struct ActivatedLineCst {
    pub(crate) info: LineInfo,
    pub(crate) cost: ActivationCostCst,
    pub(crate) effect_text: String,
    pub(crate) colon_span: TextSpan,
}

#[derive(Debug, Clone)]
pub(crate) struct UnsupportedLineCst {
    pub(crate) info: LineInfo,
    pub(crate) reason_code: &'static str,
}
