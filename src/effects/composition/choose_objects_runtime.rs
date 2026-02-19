//! Runtime orchestration for `ChooseObjectsEffect`.

use crate::decisions::make_decision;
use crate::decisions::specs::ChooseObjectsSpec;
use crate::effect::{ChoiceCount, EffectOutcome, EffectResult};
use crate::effects::helpers::resolve_player_filter;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::ids::{ObjectId, PlayerId};
use crate::snapshot::ObjectSnapshot;
use crate::zone::Zone;

use super::choose_objects::ChooseObjectsEffect;

fn collect_candidates(
    effect: &ChooseObjectsEffect,
    game: &GameState,
    ctx: &ExecutionContext,
    chooser_id: PlayerId,
) -> Result<Vec<ObjectId>, ExecutionError> {
    let filter_ctx = ctx.filter_context(game);
    let search_zone = effect.filter.zone.unwrap_or(effect.zone);

    let candidates = match search_zone {
        Zone::Battlefield => game
            .battlefield
            .iter()
            .filter_map(|&id| game.object(id).map(|obj| (id, obj)))
            .filter(|(_, obj)| effect.filter.matches(obj, &filter_ctx, game))
            .map(|(id, _)| id)
            .collect(),
        Zone::Hand => {
            let player = game
                .player(chooser_id)
                .ok_or(ExecutionError::PlayerNotFound(chooser_id))?;
            player
                .hand
                .iter()
                .filter_map(|&id| game.object(id).map(|obj| (id, obj)))
                .filter(|(_, obj)| effect.filter.matches(obj, &filter_ctx, game))
                .map(|(id, _)| id)
                .collect()
        }
        Zone::Graveyard => {
            let player = game
                .player(chooser_id)
                .ok_or(ExecutionError::PlayerNotFound(chooser_id))?;

            if effect.top_only {
                player
                    .graveyard
                    .iter()
                    .rev()
                    .filter_map(|&id| game.object(id).map(|obj| (id, obj)))
                    .find(|(_, obj)| effect.filter.matches(obj, &filter_ctx, game))
                    .map(|(id, _)| vec![id])
                    .unwrap_or_default()
            } else {
                player
                    .graveyard
                    .iter()
                    .filter_map(|&id| game.object(id).map(|obj| (id, obj)))
                    .filter(|(_, obj)| effect.filter.matches(obj, &filter_ctx, game))
                    .map(|(id, _)| id)
                    .collect()
            }
        }
        _ => game
            .objects_in_zone(search_zone)
            .into_iter()
            .filter_map(|id| game.object(id).map(|obj| (id, obj)))
            .filter(|(_, obj)| effect.filter.matches(obj, &filter_ctx, game))
            .map(|(id, _)| id)
            .collect(),
    };

    Ok(candidates)
}

fn compute_choice_bounds(count: ChoiceCount, candidate_count: usize) -> (usize, usize) {
    let min = count.min.min(candidate_count);
    let max = count.max.unwrap_or(candidate_count).min(candidate_count);
    (min, max)
}

fn normalize_chosen_objects(
    mut chosen: Vec<ObjectId>,
    candidates: &[ObjectId],
    min: usize,
    max: usize,
) -> Vec<ObjectId> {
    chosen.truncate(max);
    chosen.sort();
    chosen.dedup();

    if chosen.len() < min {
        for id in candidates {
            if chosen.len() >= min {
                break;
            }
            if !chosen.contains(id) {
                chosen.push(*id);
            }
        }
    }

    chosen
}

fn snapshot_chosen_objects(game: &GameState, chosen: &[ObjectId]) -> Vec<ObjectSnapshot> {
    chosen
        .iter()
        .filter_map(|&id| {
            game.object(id)
                .map(|obj| ObjectSnapshot::from_object(obj, game))
        })
        .collect()
}

pub(crate) fn run_choose_objects(
    effect: &ChooseObjectsEffect,
    game: &mut GameState,
    ctx: &mut ExecutionContext,
) -> Result<EffectOutcome, ExecutionError> {
    let chooser_id = resolve_player_filter(game, &effect.chooser, ctx)?;

    if effect.is_search && !game.can_search_library(chooser_id) {
        return Ok(EffectOutcome::from_result(EffectResult::Prevented));
    }
    if effect.is_search {
        game.library_searches_this_turn.insert(chooser_id);
    }

    let candidates = collect_candidates(effect, game, ctx, chooser_id)?;
    if candidates.is_empty() {
        return Ok(EffectOutcome::count(0));
    }

    let (min, max) = compute_choice_bounds(effect.count, candidates.len());
    if max == 0 {
        return Ok(EffectOutcome::count(0));
    }

    let spec = ChooseObjectsSpec::new(
        ctx.source,
        effect.description.to_string(),
        candidates.clone(),
        min,
        Some(max),
    );
    let chosen: Vec<ObjectId> =
        make_decision(game, ctx.decision_maker, chooser_id, Some(ctx.source), spec);
    let chosen = normalize_chosen_objects(chosen, &candidates, min, max);

    let snapshots = snapshot_chosen_objects(game, &chosen);
    if !snapshots.is_empty() {
        ctx.tag_objects(effect.tag.clone(), snapshots);
    }

    Ok(EffectOutcome::from_result(EffectResult::Objects(chosen)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_choice_bounds_clamps_to_candidates() {
        let (min, max) = compute_choice_bounds(ChoiceCount::exactly(3), 2);
        assert_eq!(min, 2);
        assert_eq!(max, 2);
    }

    #[test]
    fn test_normalize_chosen_objects_truncates_dedups_and_fills() {
        let candidates = vec![
            ObjectId::from_raw(1),
            ObjectId::from_raw(2),
            ObjectId::from_raw(3),
        ];
        let chosen = vec![
            ObjectId::from_raw(3),
            ObjectId::from_raw(3),
            ObjectId::from_raw(99),
            ObjectId::from_raw(2),
        ];

        let normalized = normalize_chosen_objects(chosen, &candidates, 2, 2);
        assert_eq!(
            normalized,
            vec![ObjectId::from_raw(3), ObjectId::from_raw(1)]
        );
    }
}
