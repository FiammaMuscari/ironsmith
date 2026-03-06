use super::*;

#[derive(Debug, Clone)]
pub(crate) struct PreparedEffectsForLowering {
    pub(crate) effects: Vec<EffectAst>,
    pub(crate) bindings: ReferenceBindings,
}

pub(crate) fn prepare_effects_for_lowering(
    effects: &[EffectAst],
    seed_last_object_tag: Option<&str>,
) -> PreparedEffectsForLowering {
    let normalized = normalize_effects_ast(effects);
    let bound = bind_unresolved_it_references_with_bindings(&normalized, seed_last_object_tag);
    PreparedEffectsForLowering {
        effects: bound.effects,
        bindings: bound.bindings,
    }
}

pub(crate) fn parse_text_with_annotations(
    builder: CardDefinitionBuilder,
    text: String,
) -> Result<(CardDefinition, ParseAnnotations), CardTextError> {
    let ast = super::parser::parse_card_ast_with_annotations(builder, text)?;
    let ast = normalize_card_ast(ast)?;
    lower_card_ast(ast)
}

pub(crate) fn normalize_card_ast(ast: ParsedCardAst) -> Result<ParsedCardAst, CardTextError> {
    Ok(ast)
}

pub(crate) fn lower_card_ast(
    ast: ParsedCardAst,
) -> Result<(CardDefinition, ParseAnnotations), CardTextError> {
    let ParsedCardAst {
        mut builder,
        mut annotations,
        items,
        allow_unsupported,
    } = ast;

    let mut level_abilities = Vec::new();
    let mut last_restrictable_ability: Option<usize> = None;

    for item in items {
        match item {
            ParsedCardItem::Line(line) => {
                lower_line_ast(
                    &mut builder,
                    &mut annotations,
                    line,
                    allow_unsupported,
                    &mut last_restrictable_ability,
                )?;
            }
            ParsedCardItem::Modal(modal) => {
                let abilities_before = builder.abilities.len();
                builder = super::parser::lower_parsed_modal(builder, modal, allow_unsupported)?;
                update_last_restrictable_ability(
                    &builder,
                    abilities_before,
                    &mut last_restrictable_ability,
                );
            }
            ParsedCardItem::LevelAbility(level) => {
                level_abilities.push(lower_level_ability_ast(level)?);
            }
        }
    }

    if !level_abilities.is_empty() {
        builder = builder.with_level_abilities(level_abilities);
    }

    builder = super::parser::finalize_lowered_card(builder);
    Ok((builder.build(), annotations))
}

fn lower_line_ast(
    builder: &mut CardDefinitionBuilder,
    annotations: &mut ParseAnnotations,
    line: ParsedLineAst,
    allow_unsupported: bool,
    last_restrictable_ability: &mut Option<usize>,
) -> Result<(), CardTextError> {
    let ParsedLineAst {
        info,
        chunks,
        mut restrictions,
    } = line;
    let mut handled_restrictions_for_new_ability = false;

    for parsed in chunks {
        if let LineAst::Statement { effects } = &parsed
            && apply_instead_followup_statement_to_last_ability(
                builder,
                *last_restrictable_ability,
                effects,
                &info,
                annotations,
            )?
        {
            handled_restrictions_for_new_ability = true;
            continue;
        }

        let abilities_before = builder.abilities.len();
        *builder = super::parser::apply_line_ast(
            builder.clone(),
            parsed,
            &info,
            allow_unsupported,
            annotations,
        )?;
        let abilities_after = builder.abilities.len();

        for ability_idx in abilities_before..abilities_after {
            super::parser::apply_pending_restrictions_to_ability(
                &mut builder.abilities[ability_idx],
                &mut restrictions,
            );
            handled_restrictions_for_new_ability = true;
        }

        update_last_restrictable_ability(builder, abilities_before, last_restrictable_ability);
    }

    if !handled_restrictions_for_new_ability
        && let Some(index) = *last_restrictable_ability
        && index < builder.abilities.len()
    {
        super::parser::apply_pending_restrictions_to_ability(
            &mut builder.abilities[index],
            &mut restrictions,
        );
    }

    Ok(())
}

fn update_last_restrictable_ability(
    builder: &CardDefinitionBuilder,
    abilities_before: usize,
    last_restrictable_ability: &mut Option<usize>,
) {
    let abilities_after = builder.abilities.len();
    if abilities_after <= abilities_before {
        return;
    }

    for ability_idx in (abilities_before..abilities_after).rev() {
        if super::parser::is_restrictable_ability(&builder.abilities[ability_idx]) {
            *last_restrictable_ability = Some(ability_idx);
            return;
        }
    }
}

fn lower_level_ability_ast(level: ParsedLevelAbilityAst) -> Result<LevelAbility, CardTextError> {
    let mut lowered = LevelAbility::new(level.min_level, level.max_level);
    if let Some((power, toughness)) = level.pt {
        lowered = lowered.with_pt(power, toughness);
    }

    for item in level.items {
        match item {
            ParsedLevelAbilityItemAst::StaticAbilities(abilities) => {
                lowered
                    .abilities
                    .extend(lower_static_abilities_ast(abilities)?);
            }
            ParsedLevelAbilityItemAst::KeywordActions(actions) => {
                for action in actions {
                    if let Some(ability) = keyword_action_to_static_ability(action) {
                        lowered.abilities.push(ability);
                    }
                }
            }
        }
    }

    Ok(lowered)
}
