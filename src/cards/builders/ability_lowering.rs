use super::*;

pub(crate) fn lower_activated_ability_effects_with_seed(
    effects_ast: &[EffectAst],
    seed_last_object_tag: Option<&str>,
) -> Result<(Vec<Effect>, Vec<ChooseSpec>), CardTextError> {
    compile_trigger_effects_with_seed(None, effects_ast, seed_last_object_tag)
}

pub(crate) fn lower_parsed_ability(
    mut parsed: ParsedAbility,
) -> Result<ParsedAbility, CardTextError> {
    let Some(effects_ast) = parsed.effects_ast.as_ref() else {
        return Ok(parsed);
    };

    let AbilityKind::Activated(activated) = &mut parsed.ability.kind else {
        return Ok(parsed);
    };
    if !activated.effects.is_empty() || !activated.choices.is_empty() {
        return Ok(parsed);
    }

    let (effects, choices) = lower_activated_ability_effects_with_seed(
        effects_ast,
        parsed.seed_last_object_tag.as_deref(),
    )?;
    activated.effects = effects;
    activated.choices = choices;
    Ok(parsed)
}
