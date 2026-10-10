use super::*;
use crate::cards::builders::{PermissionEffectAst, ZoneMoveActionAst};

/// An entry rider modifies the preceding move itself, before entry triggers.
pub(super) fn read(
    sentences: &[SentenceInput],
    index: usize,
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let (Some(first), Some(second)) = (sentences.get(index), sentences.get(index + 1)) else {
        return Ok(None);
    };
    let rider = crate::util::trim_edge_punctuation_tokens(second.lowered());
    if !crate::grammar::primitives::probe_all(
        rider,
        crate::grammar::primitives::any_phrase(&[
            &["they", "enter", "tapped"],
            &["it", "enters", "tapped"],
        ]),
        "moved-object tapped entry",
    ).is_some() {
        return Ok(None);
    }
    fn bind(effects: &mut [EffectAst]) -> bool {
        let [effect] = effects else { return false; };
        match effect {
            EffectAst::SubjectVerb(SubjectVerbEffectAst {
                action: SubjectVerbActionAst::ZoneMoves(ZoneMoveActionAst::MoveToZone {
                    zone: Zone::Battlefield, battlefield_tapped, ..
                }), ..
            }) => { *battlefield_tapped = true; true }
            EffectAst::SourceSentence { effects, .. }
            | EffectAst::Permissions(PermissionEffectAst::May { effects })
            | EffectAst::Permissions(PermissionEffectAst::MayByPlayer { effects, .. }) => bind(effects),
            EffectAst::TagReferenced { effect, .. } | EffectAst::TagAffected { effect, .. } =>
                bind(std::slice::from_mut(effect.as_mut())),
            _ => false,
        }
    }
    let mut effects = super::super::parse_effect_sentence_lexed(first.lowered())?;
    Ok(bind(&mut effects).then_some(effects))
}
