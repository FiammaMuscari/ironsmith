// ============================================================================
// State-Based Actions Integration
// ============================================================================

/// Check and apply all state-based actions, generating trigger events.
///
/// This runs repeatedly until no more SBAs need to be applied.
/// Note: This version auto-keeps the first legend for legend rule violations.
/// Use `check_and_apply_sbas_with` to handle legend rule interactively.
pub fn check_and_apply_sbas(
    game: &mut GameState,
    trigger_queue: &mut TriggerQueue,
) -> Result<(), GameLoopError> {
    let mut dm = crate::decision::AutoPassDecisionMaker;
    check_and_apply_sbas_with(game, trigger_queue, &mut dm)
}

/// Check and apply all state-based actions, generating trigger events.
///
/// This runs repeatedly until no more SBAs need to be applied.
/// Legend rule violations will prompt the decision maker for which legend to keep.
pub fn check_and_apply_sbas_with(
    game: &mut GameState,
    trigger_queue: &mut TriggerQueue,
    decision_maker: &mut impl DecisionMaker,
) -> Result<(), GameLoopError> {
    use crate::decisions::make_decision;
    use crate::rules::state_based::{apply_legend_rule_choice, get_legend_rule_specs};

    // Refresh continuous state (static ability effects and "can't" effect tracking)
    // before checking SBAs. This ensures the layer system is up to date.
    game.refresh_continuous_state();

    loop {
        let actions = check_state_based_actions(game);
        if actions.is_empty() {
            break;
        }

        // Handle legend rule decisions first
        let legend_specs = get_legend_rule_specs(game);
        let had_legend_decisions = !legend_specs.is_empty();
        for (player, spec) in legend_specs {
            let keep_id: ObjectId = make_decision(game, decision_maker, player, None, spec);
            apply_legend_rule_choice(game, keep_id);
        }

        // Apply the SBAs (legend rule already handled above)
        // Use the decision maker version to allow interactive replacement effect choices
        let applied = apply_state_based_actions_with(game, decision_maker);
        // SBA moves queue primitive ZoneChangeEvent via move_object; consume them now.
        drain_pending_trigger_events(game, trigger_queue);
        if !applied && !had_legend_decisions {
            break;
        }
    }

    Ok(())
}

/// Put triggered abilities from the queue onto the stack.
pub fn put_triggers_on_stack(
    game: &mut GameState,
    trigger_queue: &mut TriggerQueue,
) -> Result<(), GameLoopError> {
    let mut dm = crate::decision::AutoPassDecisionMaker;
    put_triggers_on_stack_with_dm(game, trigger_queue, &mut dm)
}

/// Put triggered abilities from the queue onto the stack with target selection.
///
/// This handles the full flow of putting triggers on the stack:
/// 1. Group triggers by controller (APNAP order)
/// 2. For each trigger, handle target selection if needed
/// 3. Push the trigger onto the stack with targets
pub fn put_triggers_on_stack_with_dm(
    game: &mut GameState,
    trigger_queue: &mut TriggerQueue,
    decision_maker: &mut dyn DecisionMaker,
) -> Result<(), GameLoopError> {
    // Group triggers by controller (APNAP order)
    let active_player = game.turn.active_player;
    let mut active_triggers = Vec::new();
    let mut other_triggers = Vec::new();

    for trigger in trigger_queue.take_all() {
        if trigger.controller == active_player {
            active_triggers.push(trigger);
        } else {
            other_triggers.push(trigger);
        }
    }

    // Active player's triggers go on stack first (resolve last)
    for trigger in active_triggers {
        if !can_stack_trigger_this_turn(game, &trigger) {
            continue;
        }
        if let Some(entry) =
            create_triggered_stack_entry_with_targets(game, &trigger, decision_maker)
        {
            game.record_trigger_fired(trigger.source, trigger.trigger_identity);
            game.push_to_stack(entry);
        }
    }

    // Then other players' triggers (in turn order)
    for trigger in other_triggers {
        if !can_stack_trigger_this_turn(game, &trigger) {
            continue;
        }
        if let Some(entry) =
            create_triggered_stack_entry_with_targets(game, &trigger, decision_maker)
        {
            game.record_trigger_fired(trigger.source, trigger.trigger_identity);
            game.push_to_stack(entry);
        }
    }

    Ok(())
}

fn can_stack_trigger_this_turn(game: &GameState, trigger: &TriggeredAbilityEntry) -> bool {
    let Some(ref condition) = trigger.ability.intervening_if else {
        return true;
    };

    match condition {
        crate::ConditionExpr::FirstTimeThisTurn | crate::ConditionExpr::MaxTimesEachTurn(_) => {
            verify_intervening_if(
                game,
                condition,
                trigger.controller,
                &trigger.triggering_event,
                trigger.source,
                Some(trigger.trigger_identity),
            )
        }
        _ => true,
    }
}

/// Create a stack entry for a triggered ability, handling target selection.
///
/// Returns None if the trigger has mandatory targets but no legal targets exist.
fn create_triggered_stack_entry_with_targets(
    game: &GameState,
    trigger: &TriggeredAbilityEntry,
    decision_maker: &mut dyn DecisionMaker,
) -> Option<StackEntry> {
    let mut entry = triggered_to_stack_entry(game, trigger);

    // Check if this trigger has targets that need to be selected
    if trigger.ability.choices.is_empty() {
        // No targets needed
        return Some(entry);
    }

    // Build tagged-object context for target selection when filters reference
    // saddle/crew contributors (e.g., "target creature that saddled it this turn").
    let mut tagged_objects: std::collections::HashMap<
        crate::tag::TagKey,
        Vec<crate::snapshot::ObjectSnapshot>,
    > = std::collections::HashMap::new();
    if !entry.crew_contributors.is_empty() {
        let snapshots = entry
            .crew_contributors
            .iter()
            .filter_map(|id| {
                game.object(*id)
                    .map(|obj| ObjectSnapshot::from_object(obj, game))
            })
            .collect::<Vec<_>>();
        if !snapshots.is_empty() {
            tagged_objects.insert(crate::tag::TagKey::from("crewed_it_this_turn"), snapshots);
        }
    }
    if !entry.saddle_contributors.is_empty() {
        let snapshots = entry
            .saddle_contributors
            .iter()
            .filter_map(|id| {
                game.object(*id)
                    .map(|obj| ObjectSnapshot::from_object(obj, game))
            })
            .collect::<Vec<_>>();
        if !snapshots.is_empty() {
            tagged_objects.insert(crate::tag::TagKey::from("saddled_it_this_turn"), snapshots);
        }
    }
    let tagged_objects_ref = if tagged_objects.is_empty() {
        None
    } else {
        Some(&tagged_objects)
    };

    // Select targets for each target spec
    let mut chosen_targets = Vec::new();
    for target_spec in &trigger.ability.choices {
        // Compute legal targets for this spec
        let legal_targets = compute_legal_targets_with_tagged_objects(
            game,
            target_spec,
            trigger.controller,
            Some(trigger.source),
            tagged_objects_ref,
        );

        if legal_targets.is_empty() {
            // No legal targets - trigger can't go on stack
            return None;
        }

        // Create a context for target selection
        let ctx = crate::decisions::context::TargetsContext::new(
            trigger.controller,
            trigger.source,
            format!("{}'s triggered ability", trigger.source_name),
            vec![crate::decisions::context::TargetRequirementContext {
                description: format!("target for {}", trigger.source_name),
                legal_targets: legal_targets.clone(),
                min_targets: 1,
                max_targets: Some(1),
            }],
        );

        // Get the choice from the decision maker
        let targets = decision_maker.decide_targets(game, &ctx);

        if let Some(first_target) = targets.first() {
            chosen_targets.push(*first_target);
        } else {
            // No target chosen - use the first legal target as default
            if let Some(first_legal) = legal_targets.first() {
                chosen_targets.push(*first_legal);
            } else {
                return None;
            }
        }
    }

    // Add the chosen targets to the stack entry
    entry.targets = chosen_targets;

    Some(entry)
}

/// Convert a triggered ability entry to a stack entry.
fn triggered_to_stack_entry(game: &GameState, trigger: &TriggeredAbilityEntry) -> StackEntry {
    use crate::events::EventKind;
    use crate::events::combat::{CreatureAttackedEvent, CreatureBecameBlockedEvent};
    use crate::events::zones::ZoneChangeEvent;
    use crate::triggers::AttackEventTarget;

    // Capture source LKI at trigger-to-stack time. If the source no longer exists,
    // fall back to snapshot data from the triggering event (e.g. dies triggers).
    let source_snapshot = game
        .object(trigger.source)
        .map(|obj| ObjectSnapshot::from_object_with_calculated_characteristics(obj, game))
        .or_else(|| {
            trigger
                .triggering_event
                .downcast::<ZoneChangeEvent>()
                .and_then(|zc| zc.snapshot.clone())
                .filter(|snapshot| snapshot.object_id == trigger.source)
        })
        .or_else(|| {
            game.find_object_by_stable_id(trigger.source_stable_id)
                .and_then(|id| game.object(id))
                .map(|obj| ObjectSnapshot::from_object_with_calculated_characteristics(obj, game))
        });

    // Create an ability stack entry with the effects from the triggered ability
    let mut entry = StackEntry::ability(
        trigger.source,
        trigger.controller,
        trigger.ability.effects.clone(),
    )
    .with_source_info(trigger.source_stable_id, trigger.source_name.clone())
    .with_triggering_event(trigger.triggering_event.clone());
    if let Some(snapshot) = source_snapshot {
        entry = entry.with_source_snapshot(snapshot);
    }
    // If the source was cast with X, propagate that value to the triggered ability.
    if let Some(x) = trigger.x_value {
        entry = entry.with_x(x);
    } else if let Some(obj) = game.object(trigger.source)
        && let Some(x) = obj.x_value
    {
        entry = entry.with_x(x);
    } else if let Some(ref snapshot) = entry.source_snapshot
        && let Some(x) = snapshot.x_value
    {
        entry = entry.with_x(x);
    }
    // Propagate keyword payment contributions from the source permanent's cast,
    // so triggered abilities can reference "each creature that convoked it", etc.
    if let Some(obj) = game.object(trigger.source)
        && !obj.keyword_payment_contributions_to_cast.is_empty()
    {
        entry.keyword_payment_contributions = obj.keyword_payment_contributions_to_cast.clone();
    }

    if let Some(crewers) = game.crewed_this_turn.get(&trigger.source)
        && !crewers.is_empty()
    {
        entry.crew_contributors = crewers.clone();
    }

    if let Some(saddlers) = game.saddled_this_turn.get(&trigger.source)
        && !saddlers.is_empty()
    {
        entry.saddle_contributors = saddlers.clone();
    }

    // Copy intervening-if condition if present (must be rechecked at resolution time)
    if let Some(ref condition) = trigger.ability.intervening_if {
        entry = entry.with_intervening_if(condition.clone());
    }

    // Extract defending player from combat triggers
    if trigger.triggering_event.kind() == EventKind::CreatureAttacked
        && let Some(attacked) = trigger.triggering_event.downcast::<CreatureAttackedEvent>()
    {
        match attacked.target {
            AttackEventTarget::Player(player_id) => {
                entry = entry.with_defending_player(player_id);
            }
            AttackEventTarget::Planeswalker(planeswalker_id) => {
                if let Some(planeswalker) = game.object(planeswalker_id) {
                    entry = entry.with_defending_player(planeswalker.controller);
                }
            }
        }
    }
    if trigger.triggering_event.kind() == EventKind::CreatureAttackedAndUnblocked
        && let Some(attacked) = trigger
            .triggering_event
            .downcast::<CreatureAttackedAndUnblockedEvent>()
    {
        match attacked.target {
            AttackEventTarget::Player(player_id) => {
                entry = entry.with_defending_player(player_id);
            }
            AttackEventTarget::Planeswalker(planeswalker_id) => {
                if let Some(planeswalker) = game.object(planeswalker_id) {
                    entry = entry.with_defending_player(planeswalker.controller);
                }
            }
        }
    }
    if trigger.triggering_event.kind() == EventKind::CreatureBecameBlocked
        && let Some(blocked) = trigger
            .triggering_event
            .downcast::<CreatureBecameBlockedEvent>()
        && let Some(target) = blocked.attack_target
    {
        match target {
            AttackEventTarget::Player(player_id) => {
                entry = entry.with_defending_player(player_id);
            }
            AttackEventTarget::Planeswalker(planeswalker_id) => {
                if let Some(planeswalker) = game.object(planeswalker_id) {
                    entry = entry.with_defending_player(planeswalker.controller);
                }
            }
        }
    }

    // Check if this is a saga's final chapter ability.
    // Use trigger metadata directly instead of parsing display strings.
    if let Some(chapters) = trigger.ability.trigger.saga_chapters()
        && let Some(saga_obj) = game.object(trigger.source)
    {
        let max_chapter = saga_obj.max_saga_chapter.unwrap_or(0);
        if chapters.iter().any(|&ch| ch >= max_chapter) {
            entry = entry.with_saga_final_chapter(trigger.source);
        }
    }

    entry
}

