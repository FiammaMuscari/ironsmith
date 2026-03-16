use crate::card::{LinkedFaceLayout, PowerToughness};
use crate::cards::{
    CardDefinition, ParseAnnotations,
    builders::{CardDefinitionBuilder as RawCardDefinitionBuilder, CardTextError},
};
use crate::color::ColorSet;
use crate::ids::CardId;
use crate::mana::ManaCost;
use crate::types::{CardType, Subtype, Supertype};

/// Restricted builder surface for hand-written card definitions.
///
/// This wrapper intentionally exposes card metadata and parser entrypoints only.
/// Rules text and abilities in `cards::definitions` should come from `parse_text`
/// and related compilation methods, not from directly constructing effects.
#[derive(Debug, Clone)]
pub(crate) struct CardDefinitionBuilder(RawCardDefinitionBuilder);

#[allow(dead_code)]
impl CardDefinitionBuilder {
    pub(crate) fn new(id: CardId, name: impl Into<String>) -> Self {
        Self(RawCardDefinitionBuilder::new(id, name))
    }

    pub(crate) fn mana_cost(self, cost: ManaCost) -> Self {
        Self(self.0.mana_cost(cost))
    }

    pub(crate) fn color_indicator(self, colors: ColorSet) -> Self {
        Self(self.0.color_indicator(colors))
    }

    pub(crate) fn supertypes(self, supertypes: Vec<Supertype>) -> Self {
        Self(self.0.supertypes(supertypes))
    }

    pub(crate) fn card_types(self, types: Vec<CardType>) -> Self {
        Self(self.0.card_types(types))
    }

    pub(crate) fn subtypes(self, subtypes: Vec<Subtype>) -> Self {
        Self(self.0.subtypes(subtypes))
    }

    pub(crate) fn oracle_text(self, text: impl Into<String>) -> Self {
        Self(self.0.oracle_text(text))
    }

    pub(crate) fn other_face(self, face: CardId) -> Self {
        Self(self.0.other_face(face))
    }

    pub(crate) fn other_face_name(self, name: impl Into<String>) -> Self {
        Self(self.0.other_face_name(name))
    }

    pub(crate) fn linked_face_layout(self, layout: LinkedFaceLayout) -> Self {
        Self(self.0.linked_face_layout(layout))
    }

    pub(crate) fn has_fuse(self) -> Self {
        Self(self.0.has_fuse())
    }

    pub(crate) fn power_toughness(self, pt: PowerToughness) -> Self {
        Self(self.0.power_toughness(pt))
    }

    pub(crate) fn loyalty(self, loyalty: u32) -> Self {
        Self(self.0.loyalty(loyalty))
    }

    pub(crate) fn defense(self, defense: u32) -> Self {
        Self(self.0.defense(defense))
    }

    pub(crate) fn token(self) -> Self {
        Self(self.0.token())
    }

    pub(crate) fn saga(self, max_chapters: u32) -> Self {
        Self(self.0.saga(max_chapters))
    }

    pub(crate) fn parse_text(
        self,
        text: impl Into<String>,
    ) -> Result<CardDefinition, CardTextError> {
        self.0.parse_text(text)
    }

    pub(crate) fn parse_text_allow_unsupported(
        self,
        text: impl Into<String>,
    ) -> Result<CardDefinition, CardTextError> {
        self.0.parse_text_allow_unsupported(text)
    }

    pub(crate) fn parse_text_with_annotations(
        self,
        text: impl Into<String>,
    ) -> Result<(CardDefinition, ParseAnnotations), CardTextError> {
        self.0.parse_text_with_annotations(text)
    }

    pub(crate) fn parse_text_with_annotations_allow_unsupported(
        self,
        text: impl Into<String>,
    ) -> Result<(CardDefinition, ParseAnnotations), CardTextError> {
        self.0.parse_text_with_annotations_allow_unsupported(text)
    }

    pub(crate) fn from_text_with_metadata(
        self,
        text: impl Into<String>,
    ) -> Result<CardDefinition, CardTextError> {
        self.0.from_text_with_metadata(text)
    }

    pub(crate) fn text_box(self, text: impl Into<String>) -> Result<CardDefinition, CardTextError> {
        self.0.text_box(text)
    }

    pub(crate) fn from_text_with_metadata_oracle_only(
        self,
        text: impl Into<String>,
    ) -> CardDefinition {
        self.0.from_text_with_metadata_oracle_only(text)
    }

    pub(crate) fn build(self) -> CardDefinition {
        self.0.build()
    }
}
