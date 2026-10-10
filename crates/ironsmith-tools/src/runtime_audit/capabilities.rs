//! Necessary runtime capability checks using materialized effect traits.
//! These are reachability candidates, not gameplay proofs. Paths address the
//! runtime child visitor rather than JSON Pointer fields in the wire model.
//! Costs, static abilities and embedded definitions not exposed by the effect
//! child visitor remain outside these capability checks.

use super::contracts::ContractFinding;
use ironsmith::Effect;
use ironsmith::ability::AbilityKind;
use ironsmith::cards::CardDefinition;
use ironsmith::effects::ChooseObjectsEffect;

fn failure(path: &str, code: &str, message: String) -> ContractFinding {
    ContractFinding {
        path: path.into(),
        severity: "error".into(),
        code: code.into(),
        message,
    }
}

fn visit(effect: &Effect, path: &str, findings: &mut Vec<ContractFinding>) {
    // ForPlayers now executes actions without immutable proposals through
    // its ordered per-player fallback. The proposal capability traits alone
    // no longer establish that a body is unsupported.
    if let Some(choice) = effect.downcast_ref::<ChooseObjectsEffect>() {
        if choice.filter.zone.is_none() && choice.zone.is_none() {
            findings.push(failure(path, "missing_choice_zone",
                "ChooseObjectsEffect has neither filter.zone nor zone; runtime search_zones rejects this instruction if reached".into()));
        }
    }
    let mut index = 0;
    effect.visit_child_effects(&mut |child| {
        visit(child, &format!("{path}/children[{index}]"), findings);
        index += 1;
    });
}

fn visit_program(
    program: &ironsmith::ResolutionProgram,
    path: &str,
    findings: &mut Vec<ContractFinding>,
) {
    for (segment_index, segment) in program.segments.iter().enumerate() {
        for (index, effect) in segment.default_effects.iter().enumerate() {
            visit(
                effect,
                &format!("{path}/segments[{segment_index}]/default_effects[{index}]"),
                findings,
            );
        }
        for (branch_index, branch) in segment.self_replacements.iter().enumerate() {
            for (index, effect) in branch.replacement_effects.iter().enumerate() {
                visit(
                    effect,
                    &format!(
                        "{path}/segments[{segment_index}]/self_replacements[{branch_index}]/effects[{index}]"
                    ),
                    findings,
                );
            }
        }
    }
}

pub fn audit(definition: &CardDefinition) -> Vec<ContractFinding> {
    let mut findings = Vec::new();
    for (index, ability) in definition.abilities.iter().enumerate() {
        let effects = match &ability.kind {
            AbilityKind::Triggered(ability) => &ability.effects,
            AbilityKind::Activated(ability) => &ability.effects,
            AbilityKind::Static(_) => continue,
        };
        visit_program(
            effects,
            &format!("runtime/abilities[{index}]/effects"),
            &mut findings,
        );
    }
    if let Some(effects) = &definition.spell_effect {
        visit_program(effects, "runtime/spell_effect", &mut findings);
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironsmith::cards::builders::CardDefinitionBuilder;
    use ironsmith::effect::{EffectId, EffectPredicate};
    use ironsmith::effects::{IfEffect, ForPlayersEffect};
    use ironsmith::target::PlayerFilter;
    use ironsmith::{CardId, CardType};

    #[test]
    fn simultaneous_draw_and_ordered_conditional_fallback_are_supported() {
        let mut definition = CardDefinitionBuilder::new(CardId::new(), "Capability fixture")
            .card_types(vec![CardType::Sorcery])
            .build();
        definition.spell_effect = Some(
            vec![Effect::new(ForPlayersEffect::new(
                PlayerFilter::Any,
                vec![Effect::target_draws(1, PlayerFilter::IteratedPlayer)],
            ))]
            .into(),
        );
        assert!(audit(&definition).is_empty());
        definition.spell_effect = Some(
            vec![Effect::new(ForPlayersEffect::new(
                PlayerFilter::Any,
                vec![Effect::new(IfEffect::if_then(
                    EffectId(0),
                    EffectPredicate::DidNotHappen,
                    vec![Effect::draw(1)],
                ))],
            ))]
            .into(),
        );
        assert!(audit(&definition).is_empty());
    }
}

#[cfg(test)]
mod collective_payment_contract_tests {
    #[test]
    fn full_join_forces_bodies_have_real_simultaneous_proposals() {
        let rows: Vec<serde_json::Value> = serde_json::from_str(include_str!("../../../../fixtures/join_forces.json.fixture")).unwrap();
        for row in rows {
            let name = row["name"].as_str().unwrap();
            let text = format!("Mana cost: {}\nType: {}\n{}", row["mana_cost"].as_str().unwrap(), row["type_line"].as_str().unwrap(), row["oracle_text"].as_str().unwrap());
            let definition = crate::compile_to_runtime_definition(name, text, false).unwrap();
            let findings = super::audit(&definition);
            assert!(findings.is_empty(), "{name}: {findings:?}");
        }
        // Actual execution is covered by the join-forces gameplay suite;
        // this audit only reports capabilities the runtime cannot provide.
    }
}
