//! Runtime orchestration for `ChooseModeEffect`.

use crate::ability::AbilityKind;
use crate::decisions::{ModesSpec, make_decision, specs::ModeOption};
use crate::effect::{EffectMode, EffectOutcome};
use crate::effects::helpers::resolve_value;
use crate::executor::{ExecutionContext, ExecutionError, execute_effect};
use crate::game_state::GameState;
use crate::ids::{ObjectId, PlayerId};
use crate::targeting::compute_legal_targets;

use super::choose_mode::ChooseModeEffect;

fn check_mode_legal(
    game: &GameState,
    mode: &EffectMode,
    controller: PlayerId,
    source: ObjectId,
) -> bool {
    for effect in &mode.effects {
        if let Some(target_spec) = effect.0.get_target_spec() {
            let legal_targets = compute_legal_targets(game, target_spec, controller, Some(source));
            // If effect requires targets (min > 0) and none exist, mode is illegal.
            // Most effects require at least one target unless explicitly "up to".
            if legal_targets.is_empty() {
                return false;
            }
        }
    }
    true
}

fn find_source_activated_ability_index(
    game: &GameState,
    source: ObjectId,
    choose_mode: &ChooseModeEffect,
) -> Option<usize> {
    let source_object = game.object(source)?;
    let mut exact_indices = Vec::new();
    let mut fallback_indices = Vec::new();

    for (idx, ability) in source_object.abilities.iter().enumerate() {
        let AbilityKind::Activated(activated) = &ability.kind else {
            continue;
        };

        let mut has_disallow_choose_mode = false;
        let mut has_exact_choose_mode = false;
        for effect in &activated.effects {
            if let Some(candidate) = effect.downcast_ref::<ChooseModeEffect>() {
                if candidate.disallow_previously_chosen_modes {
                    has_disallow_choose_mode = true;
                }
                if candidate == choose_mode {
                    has_exact_choose_mode = true;
                }
            }
        }

        if has_exact_choose_mode {
            exact_indices.push(idx);
        }
        if has_disallow_choose_mode {
            fallback_indices.push(idx);
        }
    }

    if exact_indices.len() == 1 {
        return exact_indices.first().copied();
    }
    if exact_indices.is_empty() && fallback_indices.len() == 1 {
        return fallback_indices.first().copied();
    }
    None
}

pub(crate) fn run_choose_mode(
    effect: &ChooseModeEffect,
    game: &mut GameState,
    ctx: &mut ExecutionContext,
) -> Result<EffectOutcome, ExecutionError> {
    let max_modes = resolve_value(game, &effect.choose_count, ctx)?.max(0) as usize;
    let min_modes = match &effect.min_choose_count {
        Some(min_val) => resolve_value(game, min_val, ctx)?.max(0) as usize,
        None => max_modes,
    };

    if effect.modes.is_empty() || max_modes == 0 {
        return Ok(EffectOutcome::resolved());
    }

    let source_ability_index = if effect.disallow_previously_chosen_modes {
        find_source_activated_ability_index(game, ctx.source, effect)
    } else {
        None
    };
    let is_mode_available = |mode_idx: usize| {
        mode_idx < effect.modes.len()
            && !source_ability_index.is_some_and(|ability_index| {
                game.ability_mode_was_chosen(
                    ctx.source,
                    ability_index,
                    mode_idx,
                    effect.disallow_previously_chosen_modes_this_turn,
                )
            })
    };
    let is_mode_legal = |mode_idx: usize| {
        is_mode_available(mode_idx)
            && effect
                .modes
                .get(mode_idx)
                .is_some_and(|mode| check_mode_legal(game, mode, ctx.controller, ctx.source))
    };

    // Per MTG rule 601.2b, modes are chosen during casting (before targets).
    // Check if modes were pre-chosen during the casting process.
    let chosen_indices: Vec<usize> = if let Some(ref pre_chosen) = ctx.chosen_modes {
        pre_chosen.clone()
    } else {
        let mode_options: Vec<ModeOption> = effect
            .modes
            .iter()
            .enumerate()
            .map(|(i, mode)| {
                ModeOption::with_legality(i, mode.description.clone(), is_mode_legal(i))
            })
            .collect();

        let legal_mode_count = mode_options.iter().filter(|m| m.legal).count();
        if legal_mode_count < min_modes {
            return Err(ExecutionError::Impossible(
                "Not enough legal modes available".to_string(),
            ));
        }

        let spec = ModesSpec::new(ctx.source, mode_options, min_modes, max_modes);
        make_decision(
            game,
            &mut ctx.decision_maker,
            ctx.controller,
            Some(ctx.source),
            spec,
        )
    };

    // Filter to valid/legal indices while preserving selection order.
    let mut valid_chosen_indices: Vec<usize> = Vec::new();
    for idx in chosen_indices {
        if !is_mode_legal(idx) {
            continue;
        }
        if !effect.allow_repeated_modes && valid_chosen_indices.contains(&idx) {
            continue;
        }
        valid_chosen_indices.push(idx);
        if valid_chosen_indices.len() >= max_modes {
            break;
        }
    }

    if valid_chosen_indices.len() < min_modes {
        return Err(ExecutionError::Impossible(
            "Not enough legal modes available".to_string(),
        ));
    }

    if let Some(ability_index) = source_ability_index {
        for &mode_idx in &valid_chosen_indices {
            game.record_ability_mode_choice(
                ctx.source,
                ability_index,
                mode_idx,
                effect.disallow_previously_chosen_modes_this_turn,
            );
        }
    }

    let mut outcomes = Vec::new();
    for &idx in &valid_chosen_indices {
        if let Some(mode) = effect.modes.get(idx) {
            for inner in &mode.effects {
                outcomes.push(execute_effect(game, inner, ctx)?);
            }
        }
    }

    Ok(EffectOutcome::aggregate(outcomes))
}
