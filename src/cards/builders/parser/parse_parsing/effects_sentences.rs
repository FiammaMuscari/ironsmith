fn parse_effect_sentences(tokens: &[Token]) -> Result<Vec<EffectAst>, CardTextError> {
    let mut effects = Vec::new();
    let sentences = split_on_period(tokens);
    let mut sentence_idx = 0usize;
    let mut carried_context: Option<CarryContext> = None;

    fn effect_contains_search_library(effect: &EffectAst) -> bool {
        match effect {
            EffectAst::SearchLibrary { .. } => true,
            EffectAst::Conditional {
                if_true, if_false, ..
            } => {
                if_true.iter().any(effect_contains_search_library)
                    || if_false.iter().any(effect_contains_search_library)
            }
            EffectAst::UnlessPays { effects, .. }
            | EffectAst::May { effects }
            | EffectAst::MayByPlayer { effects, .. }
            | EffectAst::MayByTaggedController { effects, .. }
            | EffectAst::IfResult { effects, .. }
            | EffectAst::ForEachOpponent { effects }
            | EffectAst::ForEachPlayer { effects }
            | EffectAst::ForEachTargetPlayers { effects, .. }
            | EffectAst::ForEachObject { effects, .. }
            | EffectAst::ForEachTagged { effects, .. }
            | EffectAst::ForEachOpponentDoesNot { effects }
            | EffectAst::ForEachPlayerDoesNot { effects }
            | EffectAst::ForEachOpponentDid { effects, .. }
            | EffectAst::ForEachPlayerDid { effects, .. }
            | EffectAst::ForEachTaggedPlayer { effects, .. }
            | EffectAst::DelayedUntilNextEndStep { effects, .. }
            | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
            | EffectAst::DelayedUntilEndOfCombat { effects }
            | EffectAst::DelayedTriggerThisTurn { effects, .. }
            | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
            | EffectAst::VoteOption { effects, .. } => {
                effects.iter().any(effect_contains_search_library)
            }
            EffectAst::UnlessAction {
                effects,
                alternative,
                ..
            } => {
                effects.iter().any(effect_contains_search_library)
                    || alternative.iter().any(effect_contains_search_library)
            }
            _ => false,
        }
    }

    fn is_if_you_search_library_this_way_shuffle_sentence(tokens: &[Token]) -> bool {
        let words: Vec<&str> = words(tokens)
            .into_iter()
            .filter(|word| !is_article(word))
            .collect();
        // "If you search your library this way, shuffle."
        words.as_slice()
            == [
                "if", "you", "search", "your", "library", "this", "way", "shuffle",
            ]
            || words.as_slice()
                == [
                    "if", "you", "search", "your", "library", "this", "way", "shuffles",
                ]
    }

    while sentence_idx < sentences.len() {
        let sentence = &sentences[sentence_idx];
        if sentence.is_empty() {
            sentence_idx += 1;
            continue;
        }

        if sentence_idx + 1 < sentences.len()
            && let Some(mut combined) = parse_target_player_chooses_then_other_cant_block(
                sentence,
                &sentences[sentence_idx + 1],
            )?
        {
            parser_trace(
                "parse_effect_sentences:sequence-hit:target-chooses-other-cant-block",
                sentence,
            );
            effects.append(&mut combined);
            sentence_idx += 2;
            continue;
        }
        if sentence_idx + 1 < sentences.len()
            && let Some(mut combined) =
                parse_choose_creature_type_then_become_type(sentence, &sentences[sentence_idx + 1])?
        {
            parser_trace(
                "parse_effect_sentences:sequence-hit:choose-creature-type-then-become-type",
                sentence,
            );
            effects.append(&mut combined);
            sentence_idx += 2;
            continue;
        }
        let mut sentence_tokens = strip_embedded_token_rules_text(sentence);
        if sentence_tokens.is_empty() {
            sentence_idx += 1;
            continue;
        }
        sentence_tokens = rewrite_when_you_do_clause_prefix(&sentence_tokens);

        // Oracle frequently splits shuffle followups as a standalone sentence:
        // "If you search your library this way, shuffle." This clause is redundant when the
        // preceding sentence already compiles a library-search effect that shuffles.
        if is_if_you_search_library_this_way_shuffle_sentence(&sentence_tokens)
            && effects.iter().any(effect_contains_search_library)
        {
            parser_trace(
                "parse_effect_sentences:skip:if-you-search-library-this-way-shuffle",
                &sentence_tokens,
            );
            sentence_idx += 1;
            continue;
        }

        let mut wraps_as_if_did_not = false;
        if let Some(without_otherwise) = strip_otherwise_sentence_prefix(&sentence_tokens) {
            sentence_tokens = rewrite_otherwise_referential_subject(without_otherwise);
            wraps_as_if_did_not = true;
        }
        parser_trace("parse_effect_sentences:sentence", &sentence_tokens);

        // "Destroy ... . It/They can't be regenerated." followups.
        if is_cant_be_regenerated_followup_sentence(&sentence_tokens) {
            if apply_cant_be_regenerated_to_last_destroy_effect(&mut effects) {
                parser_trace(
                    "parse_effect_sentences:cant-be-regenerated-followup",
                    &sentence_tokens,
                );
                sentence_idx += 1;
                continue;
            }
            return Err(CardTextError::ParseError(format!(
                "unsupported standalone cant-be-regenerated clause (clause: '{}')",
                words(&sentence_tokens).join(" ")
            )));
        }

        if sentence_idx + 1 < sentences.len() && is_simple_copy_reference_sentence(&sentence_tokens)
        {
            let next_tokens = strip_embedded_token_rules_text(&sentences[sentence_idx + 1]);
            if let Some(spec) = parse_may_cast_it_sentence(&next_tokens)
                && spec.as_copy
            {
                parser_trace(
                    "parse_effect_sentences:copy-reference-next-may-cast-copy",
                    &sentence_tokens,
                );
                effects.push(build_may_cast_tagged_effect(&spec));
                sentence_idx += 2;
                continue;
            }
        }

        if let Some(spec) = parse_may_cast_it_sentence(&sentence_tokens) {
            parser_trace(
                "parse_effect_sentences:may-cast-it-sentence",
                &sentence_tokens,
            );
            effects.push(build_may_cast_tagged_effect(&spec));
            sentence_idx += 1;
            continue;
        }

        if is_spawn_scion_token_mana_reminder(&sentence_tokens) {
            if effects
                .last()
                .is_some_and(effect_creates_eldrazi_spawn_or_scion)
            {
                parser_trace(
                    "parse_effect_sentences:spawn-scion-reminder",
                    &sentence_tokens,
                );
                sentence_idx += 1;
                continue;
            }
            return Err(CardTextError::ParseError(format!(
                "unsupported standalone token mana reminder clause (clause: '{}')",
                words(&sentence_tokens).join(" ")
            )));
        }
        if let Some(effect) =
            parse_sentence_exile_that_token_when_source_leaves(&sentence_tokens, &effects)
        {
            parser_trace(
                "parse_effect_sentences:linked-token-exile-when-source-leaves",
                &sentence_tokens,
            );
            effects.push(effect);
            sentence_idx += 1;
            continue;
        }
        if let Some(effect) =
            parse_sentence_sacrifice_source_when_that_token_leaves(&sentence_tokens, &effects)
        {
            parser_trace(
                "parse_effect_sentences:linked-token-sacrifice-source-when-token-leaves",
                &sentence_tokens,
            );
            effects.push(effect);
            sentence_idx += 1;
            continue;
        }
        if is_generic_token_reminder_sentence(&sentence_tokens)
            && effects.last().is_some_and(effect_creates_any_token)
        {
            if append_token_reminder_to_last_create_effect(&mut effects, &sentence_tokens) {
                parser_trace(
                    "parse_effect_sentences:token-reminder-followup",
                    &sentence_tokens,
                );
                sentence_idx += 1;
                continue;
            }
            return Err(CardTextError::ParseError(format!(
                "unsupported standalone token reminder clause (clause: '{}')",
                words(&sentence_tokens).join(" ")
            )));
        }

        if let Some(effect) = parse_choose_target_prelude_sentence(&sentence_tokens)? {
            effects.push(effect);
            carried_context = None;
            sentence_idx += 1;
            continue;
        }

        let mut sentence_effects = parse_effect_sentence(&sentence_tokens)?;
        if wraps_as_if_did_not {
            sentence_effects = vec![EffectAst::IfResult {
                predicate: IfResultPredicate::DidNot,
                effects: sentence_effects,
            }];
            carried_context = None;
        }
        collapse_token_copy_next_end_step_exile_followup(&mut sentence_effects, &sentence_tokens);
        collapse_token_copy_end_of_combat_exile_followup(&mut sentence_effects, &sentence_tokens);
        if is_that_turn_end_step_sentence(&sentence_tokens)
            && let Some(extra_turn_player) = most_recent_extra_turn_player(&effects)
            && !sentence_effects.is_empty()
        {
            sentence_effects = vec![EffectAst::DelayedUntilEndStepOfExtraTurn {
                player: extra_turn_player,
                effects: sentence_effects,
            }];
        }
        if words(&sentence_tokens).first().copied() == Some("you") {
            carried_context = None;
        }
        if try_apply_token_copy_followup(&mut effects, &sentence_effects)? {
            parser_trace(
                "parse_effect_sentences:token-copy-followup",
                &sentence_tokens,
            );
            sentence_idx += 1;
            continue;
        }
        if sentence_effects.is_empty()
            && !is_round_up_each_time_sentence(&sentence_tokens)
            && !is_nonsemantic_restriction_sentence(&sentence_tokens)
        {
            return Err(CardTextError::ParseError(format!(
                "sentence parsed to no semantic effects (clause: '{}')",
                words(&sentence_tokens).join(" ")
            )));
        }
        // If a token-copy modifier sentinel didn't apply (no preceding CreateTokenCopy),
        // convert it to a proper effect on the tagged "it" object.
        for effect in &mut sentence_effects {
            if matches!(effect, EffectAst::TokenCopyHasHaste) {
                let span = span_from_tokens(&sentence);
                *effect = EffectAst::GrantAbilitiesToTarget {
                    target: TargetAst::Tagged(TagKey::from(IT_TAG), span),
                    abilities: vec![StaticAbility::haste()],
                    duration: Until::Forever,
                };
            } else if matches!(effect, EffectAst::TokenCopyGainHasteUntilEot) {
                let span = span_from_tokens(&sentence);
                *effect = EffectAst::GrantAbilitiesToTarget {
                    target: TargetAst::Tagged(TagKey::from(IT_TAG), span),
                    abilities: vec![StaticAbility::haste()],
                    duration: Until::EndOfTurn,
                };
            } else if matches!(effect, EffectAst::TokenCopySacrificeAtNextEndStep) {
                *effect = EffectAst::DelayedUntilNextEndStep {
                    player: PlayerFilter::Any,
                    effects: vec![EffectAst::Sacrifice {
                        filter: ObjectFilter::tagged(TagKey::from(IT_TAG)),
                        player: PlayerAst::Implicit,
                        count: 1,
                    }],
                };
            } else if matches!(effect, EffectAst::TokenCopyExileAtNextEndStep) {
                let span = span_from_tokens(&sentence);
                *effect = EffectAst::DelayedUntilNextEndStep {
                    player: PlayerFilter::Any,
                    effects: vec![EffectAst::Exile {
                        target: TargetAst::Object(
                            ObjectFilter::tagged(TagKey::from(IT_TAG)),
                            span,
                            None,
                        ),
                        face_down: false,
                    }],
                };
            }
        }
        for effect in &mut sentence_effects {
            if let Some(context) = carried_context {
                maybe_apply_carried_player_with_clause(effect, context, &sentence_tokens);
            }
            if let Some(context) = explicit_player_for_carry(effect) {
                carried_context = Some(context);
            }
        }
        if sentence_effects.len() == 1
            && let Some(previous_effect) = effects.last()
            && let Some(EffectAst::IfResult {
                predicate,
                effects: if_result_effects,
            }) = sentence_effects.first_mut()
        {
            if matches!(predicate, IfResultPredicate::Did)
                && matches!(previous_effect, EffectAst::UnlessPays { .. })
            {
                *predicate = IfResultPredicate::DidNot;
            }
            if let Some(previous_target) = primary_damage_target_from_effect(previous_effect) {
                replace_it_damage_target_in_effects(if_result_effects, &previous_target);
            }
        }
        let has_instead = sentence.iter().any(|token| token.is_word("instead"));
        if has_instead && sentence_effects.len() == 1 && effects.len() >= 1 {
            if matches!(
                sentence_effects.first(),
                Some(EffectAst::Conditional { .. })
            ) {
                let previous = effects.pop().expect("effects length checked above");
                let previous_target = primary_target_from_effect(&previous);
                let previous_damage_target = primary_damage_target_from_effect(&previous);
                if let Some(EffectAst::Conditional {
                    predicate,
                    mut if_true,
                    mut if_false,
                }) = sentence_effects.pop()
                {
                    if let Some(target) = previous_target {
                        replace_it_target_in_effects(&mut if_true, &target);
                    }
                    if let Some(target) = previous_damage_target {
                        replace_it_damage_target_in_effects(&mut if_true, &target);
                        replace_placeholder_damage_target_in_effects(&mut if_true, &target);
                    }
                    if_false.insert(0, previous);
                    effects.push(EffectAst::Conditional {
                        predicate,
                        if_true,
                        if_false,
                    });
                    sentence_idx += 1;
                    continue;
                }
            }
        }

        effects.extend(sentence_effects);
        sentence_idx += 1;
    }

    parser_trace("parse_effect_sentences:done", tokens);
    Ok(effects)
}

fn is_cant_be_regenerated_followup_sentence(tokens: &[Token]) -> bool {
    let words = normalize_cant_words(tokens);
    matches!(
        words.as_slice(),
        ["it", "cant", "be", "regenerated"]
            | ["it", "cant", "be", "regenerated", "this", "turn"]
            | ["they", "cant", "be", "regenerated"]
            | ["they", "cant", "be", "regenerated", "this", "turn"]
    )
}

fn apply_cant_be_regenerated_to_last_destroy_effect(effects: &mut Vec<EffectAst>) -> bool {
    let Some(last) = effects.pop() else {
        return false;
    };

    match last {
        EffectAst::Destroy { target } => {
            effects.push(EffectAst::DestroyNoRegeneration { target });
            true
        }
        EffectAst::DestroyAll { filter } => {
            effects.push(EffectAst::DestroyAllNoRegeneration { filter });
            true
        }
        EffectAst::DestroyAllOfChosenColor { filter } => {
            effects.push(EffectAst::DestroyAllOfChosenColorNoRegeneration { filter });
            true
        }
        other => {
            effects.push(other);
            false
        }
    }
}

fn primary_damage_target_from_effect(effect: &EffectAst) -> Option<TargetAst> {
    match effect {
        EffectAst::DealDamage { target, .. } | EffectAst::DealDamageEqualToPower { target, .. } => {
            Some(target.clone())
        }
        EffectAst::Conditional {
            if_true, if_false, ..
        } => if_true
            .iter()
            .find_map(primary_damage_target_from_effect)
            .or_else(|| if_false.iter().find_map(primary_damage_target_from_effect)),
        EffectAst::UnlessPays { effects, .. }
        | EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachPlayerDoesNot { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::VoteOption { effects, .. }
        | EffectAst::UnlessAction {
            effects,
            alternative: _,
            ..
        } => effects.iter().find_map(primary_damage_target_from_effect),
        _ => None,
    }
}

fn primary_target_from_effect(effect: &EffectAst) -> Option<TargetAst> {
    match effect {
        EffectAst::DealDamage { target, .. }
        | EffectAst::DealDamageEqualToPower { target, .. }
        | EffectAst::Counter { target }
        | EffectAst::CounterUnlessPays { target, .. }
        | EffectAst::Explore { target }
        | EffectAst::Connive { target }
        | EffectAst::Goad { target }
        | EffectAst::Tap { target }
        | EffectAst::Untap { target }
        | EffectAst::RemoveFromCombat { target }
        | EffectAst::TapOrUntap { target }
        | EffectAst::Destroy { target }
        | EffectAst::DestroyNoRegeneration { target }
        | EffectAst::Exile { target, .. }
        | EffectAst::ExileWhenSourceLeaves { target }
        | EffectAst::SacrificeSourceWhenLeaves { target }
        | EffectAst::ExileUntilSourceLeaves { target, .. }
        | EffectAst::LookAtHand { target }
        | EffectAst::Transform { target }
        | EffectAst::Flip { target }
        | EffectAst::Regenerate { target }
        | EffectAst::TargetOnly { target }
        | EffectAst::ReturnToHand { target, .. }
        | EffectAst::ReturnToBattlefield { target, .. }
        | EffectAst::MoveToZone { target, .. }
        | EffectAst::Pump { target, .. }
        | EffectAst::GrantAbilitiesToTarget { target, .. }
        | EffectAst::GrantAbilitiesChoiceToTarget { target, .. }
        | EffectAst::GrantProtectionChoice { target, .. }
        | EffectAst::PreventDamage { target, .. }
        | EffectAst::PreventAllDamageToTarget { target, .. }
        | EffectAst::PreventAllCombatDamageFromSource { source: target, .. }
        | EffectAst::RedirectNextDamageFromSourceToTarget { target, .. }
        | EffectAst::RedirectNextTimeDamageToSource { target, .. }
        | EffectAst::GainControl { target, .. } => Some(target.clone()),
        EffectAst::Conditional {
            if_true, if_false, ..
        } => if_true
            .iter()
            .find_map(primary_target_from_effect)
            .or_else(|| if_false.iter().find_map(primary_target_from_effect)),
        EffectAst::UnlessPays { effects, .. }
        | EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachPlayerDoesNot { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::VoteOption { effects, .. }
        | EffectAst::UnlessAction {
            effects,
            alternative: _,
            ..
        } => effects.iter().find_map(primary_target_from_effect),
        _ => None,
    }
}

fn replace_it_damage_target_in_effects(effects: &mut [EffectAst], target: &TargetAst) {
    for effect in effects {
        replace_it_damage_target(effect, target);
    }
}

fn replace_it_target_in_effects(effects: &mut [EffectAst], target: &TargetAst) {
    for effect in effects {
        replace_it_target(effect, target);
    }
}

fn is_placeholder_damage_target(target: &TargetAst) -> bool {
    matches!(
        target,
        TargetAst::PlayerOrPlaneswalker(PlayerFilter::Any, None)
    )
}

fn replace_placeholder_damage_target_in_effects(effects: &mut [EffectAst], target: &TargetAst) {
    for effect in effects {
        replace_placeholder_damage_target(effect, target);
    }
}

fn replace_placeholder_damage_target(effect: &mut EffectAst, target: &TargetAst) {
    match effect {
        EffectAst::DealDamage {
            target: damage_target,
            ..
        }
        | EffectAst::DealDamageEqualToPower {
            target: damage_target,
            ..
        } => {
            if is_placeholder_damage_target(damage_target) {
                *damage_target = target.clone();
            }
        }
        EffectAst::Conditional {
            if_true, if_false, ..
        } => {
            replace_placeholder_damage_target_in_effects(if_true, target);
            replace_placeholder_damage_target_in_effects(if_false, target);
        }
        EffectAst::UnlessPays { effects, .. }
        | EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachPlayerDoesNot { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::VoteOption { effects, .. } => {
            replace_placeholder_damage_target_in_effects(effects, target);
        }
        EffectAst::UnlessAction {
            effects,
            alternative,
            ..
        } => {
            replace_placeholder_damage_target_in_effects(effects, target);
            replace_placeholder_damage_target_in_effects(alternative, target);
        }
        _ => {}
    }
}

fn replace_unbound_x_in_damage_effects(
    effects: &mut [EffectAst],
    replacement: &Value,
    clause: &str,
) -> Result<(), CardTextError> {
    for effect in effects {
        replace_unbound_x_in_damage_effect(effect, replacement, clause)?;
    }
    Ok(())
}

fn replace_unbound_x_in_damage_effect(
    effect: &mut EffectAst,
    replacement: &Value,
    clause: &str,
) -> Result<(), CardTextError> {
    match effect {
        EffectAst::DealDamage { amount, .. }
        | EffectAst::DealDamageEach { amount, .. }
        | EffectAst::GainLife { amount, .. }
        | EffectAst::LoseLife { amount, .. } => {
            if value_contains_unbound_x(amount) {
                *amount = replace_unbound_x_with_value(amount.clone(), replacement, clause)?;
            }
        }
        EffectAst::Conditional {
            if_true, if_false, ..
        } => {
            replace_unbound_x_in_damage_effects(if_true, replacement, clause)?;
            replace_unbound_x_in_damage_effects(if_false, replacement, clause)?;
        }
        EffectAst::UnlessPays { effects, .. }
        | EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachPlayerDoesNot { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::VoteOption { effects, .. } => {
            replace_unbound_x_in_damage_effects(effects, replacement, clause)?;
        }
        EffectAst::UnlessAction {
            effects,
            alternative,
            ..
        } => {
            replace_unbound_x_in_damage_effects(effects, replacement, clause)?;
            replace_unbound_x_in_damage_effects(alternative, replacement, clause)?;
        }
        _ => {}
    }
    Ok(())
}

fn replace_unbound_x_in_effects_anywhere(
    effects: &mut [EffectAst],
    replacement: &Value,
    clause: &str,
) -> Result<(), CardTextError> {
    for effect in effects {
        replace_unbound_x_in_effect_anywhere(effect, replacement, clause)?;
    }
    Ok(())
}

fn replace_unbound_x_in_effect_anywhere(
    effect: &mut EffectAst,
    replacement: &Value,
    clause: &str,
) -> Result<(), CardTextError> {
    fn replace_value(
        value: &mut Value,
        replacement: &Value,
        clause: &str,
    ) -> Result<(), CardTextError> {
        if value_contains_unbound_x(value) {
            *value = replace_unbound_x_with_value(value.clone(), replacement, clause)?;
        }
        Ok(())
    }

    match effect {
        EffectAst::DealDamage { amount, .. }
        | EffectAst::DealDamageEach { amount, .. }
        | EffectAst::Draw { count: amount, .. }
        | EffectAst::LoseLife { amount, .. }
        | EffectAst::GainLife { amount, .. }
        | EffectAst::PreventDamage { amount, .. }
        | EffectAst::PreventDamageEach { amount, .. }
        | EffectAst::PutCounters { count: amount, .. }
        | EffectAst::PutCountersAll { count: amount, .. }
        | EffectAst::Mill { count: amount, .. }
        | EffectAst::Discard { count: amount, .. }
        | EffectAst::Scry { count: amount, .. }
        | EffectAst::Surveil { count: amount, .. }
        | EffectAst::Discover { count: amount, .. }
        | EffectAst::PayEnergy { amount, .. }
        | EffectAst::CopySpell { count: amount, .. }
        | EffectAst::SetLifeTotal { amount, .. }
        | EffectAst::Monstrosity { amount } => {
            replace_value(amount, replacement, clause)?;
        }
        EffectAst::PutOrRemoveCounters {
            put_count,
            remove_count,
            ..
        } => {
            replace_value(put_count, replacement, clause)?;
            replace_value(remove_count, replacement, clause)?;
        }
        EffectAst::RemoveUpToAnyCounters { amount, .. } => {
            replace_value(amount, replacement, clause)?;
        }
        EffectAst::AddManaScaled { amount, .. }
        | EffectAst::AddManaAnyColor { amount, .. }
        | EffectAst::AddManaAnyOneColor { amount, .. }
        | EffectAst::AddManaChosenColor { amount, .. }
        | EffectAst::AddManaFromLandCouldProduce { amount, .. }
        | EffectAst::AddManaCommanderIdentity { amount, .. } => {
            replace_value(amount, replacement, clause)?;
        }
        EffectAst::CreateToken { count, .. }
        | EffectAst::CreateTokenWithMods { count, .. }
        | EffectAst::CreateTokenCopy { count, .. }
        | EffectAst::CreateTokenCopyFromSource { count, .. } => {
            replace_value(count, replacement, clause)?;
        }
        EffectAst::CounterUnlessPays {
            life,
            additional_generic,
            ..
        } => {
            if let Some(life) = life.as_mut() {
                replace_value(life, replacement, clause)?;
            }
            if let Some(generic) = additional_generic.as_mut() {
                replace_value(generic, replacement, clause)?;
            }
        }
        EffectAst::Conditional {
            if_true, if_false, ..
        } => {
            replace_unbound_x_in_effects_anywhere(if_true, replacement, clause)?;
            replace_unbound_x_in_effects_anywhere(if_false, replacement, clause)?;
        }
        EffectAst::UnlessPays { effects, .. }
        | EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachPlayerDoesNot { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::VoteOption { effects, .. } => {
            replace_unbound_x_in_effects_anywhere(effects, replacement, clause)?;
        }
        EffectAst::UnlessAction {
            effects,
            alternative,
            ..
        } => {
            replace_unbound_x_in_effects_anywhere(effects, replacement, clause)?;
            replace_unbound_x_in_effects_anywhere(alternative, replacement, clause)?;
        }
        _ => {}
    }
    Ok(())
}

fn apply_where_x_to_damage_amounts(
    tokens: &[Token],
    effects: &mut [EffectAst],
) -> Result<(), CardTextError> {
    let clause_words = words(tokens);
    let has_deal_x = clause_words.windows(3).any(|window| {
        (window[0] == "deal" || window[0] == "deals") && window[1] == "x" && window[2] == "damage"
    });
    let has_x_life = clause_words.windows(3).any(|window| {
        (window[0] == "gain" || window[0] == "gains" || window[0] == "lose" || window[0] == "loses")
            && window[1] == "x"
            && window[2] == "life"
    });
    if !has_deal_x && !has_x_life {
        return Ok(());
    }
    let Some(where_idx) = clause_words
        .windows(3)
        .position(|window| window == ["where", "x", "is"])
    else {
        return Ok(());
    };
    let Some(where_token_idx) = token_index_for_word_index(tokens, where_idx) else {
        return Ok(());
    };
    let where_tokens = &tokens[where_token_idx..];
    let Some(where_value) = parse_where_x_value_clause(where_tokens) else {
        return Ok(());
    };
    replace_unbound_x_in_damage_effects(effects, &where_value, &clause_words.join(" "))
}

fn replace_it_damage_target(effect: &mut EffectAst, target: &TargetAst) {
    match effect {
        EffectAst::DealDamage {
            target: damage_target,
            ..
        } => {
            if target_references_it(damage_target) {
                *damage_target = target.clone();
            }
        }
        EffectAst::Conditional {
            if_true, if_false, ..
        } => {
            replace_it_damage_target_in_effects(if_true, target);
            replace_it_damage_target_in_effects(if_false, target);
        }
        EffectAst::UnlessPays { effects, .. }
        | EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachPlayerDoesNot { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::VoteOption { effects, .. } => {
            replace_it_damage_target_in_effects(effects, target);
        }
        EffectAst::UnlessAction {
            effects,
            alternative,
            ..
        } => {
            replace_it_damage_target_in_effects(effects, target);
            replace_it_damage_target_in_effects(alternative, target);
        }
        _ => {}
    }
}

fn replace_it_target(effect: &mut EffectAst, target: &TargetAst) {
    match effect {
        EffectAst::DealDamage {
            target: effect_target,
            ..
        }
        | EffectAst::DealDamageEqualToPower {
            target: effect_target,
            ..
        }
        | EffectAst::Counter {
            target: effect_target,
        }
        | EffectAst::CounterUnlessPays {
            target: effect_target,
            ..
        }
        | EffectAst::Explore {
            target: effect_target,
        }
        | EffectAst::Connive {
            target: effect_target,
        }
        | EffectAst::Goad {
            target: effect_target,
        }
        | EffectAst::Tap {
            target: effect_target,
        }
        | EffectAst::Untap {
            target: effect_target,
        }
        | EffectAst::RemoveFromCombat {
            target: effect_target,
        }
        | EffectAst::TapOrUntap {
            target: effect_target,
        }
        | EffectAst::Destroy {
            target: effect_target,
        }
        | EffectAst::DestroyNoRegeneration {
            target: effect_target,
        }
        | EffectAst::Exile {
            target: effect_target,
            ..
        }
        | EffectAst::ExileWhenSourceLeaves {
            target: effect_target,
        }
        | EffectAst::SacrificeSourceWhenLeaves {
            target: effect_target,
        }
        | EffectAst::ExileUntilSourceLeaves {
            target: effect_target,
            ..
        }
        | EffectAst::LookAtHand {
            target: effect_target,
        }
        | EffectAst::Transform {
            target: effect_target,
        }
        | EffectAst::Flip {
            target: effect_target,
        }
        | EffectAst::Regenerate {
            target: effect_target,
        }
        | EffectAst::TargetOnly {
            target: effect_target,
        }
        | EffectAst::ReturnToHand {
            target: effect_target,
            ..
        }
        | EffectAst::ReturnToBattlefield {
            target: effect_target,
            ..
        }
        | EffectAst::MoveToZone {
            target: effect_target,
            ..
        }
        | EffectAst::Pump {
            target: effect_target,
            ..
        }
        | EffectAst::GrantAbilitiesToTarget {
            target: effect_target,
            ..
        }
        | EffectAst::GrantAbilitiesChoiceToTarget {
            target: effect_target,
            ..
        }
        | EffectAst::GrantProtectionChoice {
            target: effect_target,
            ..
        }
        | EffectAst::PreventDamage {
            target: effect_target,
            ..
        }
        | EffectAst::PreventAllDamageToTarget {
            target: effect_target,
            ..
        }
        | EffectAst::PreventAllCombatDamageFromSource {
            source: effect_target,
            ..
        }
        | EffectAst::RedirectNextDamageFromSourceToTarget {
            target: effect_target,
            ..
        }
        | EffectAst::RedirectNextTimeDamageToSource {
            target: effect_target,
            ..
        }
        | EffectAst::GainControl {
            target: effect_target,
            ..
        } => {
            if target_references_it(effect_target) {
                *effect_target = target.clone();
            }
        }
        EffectAst::Conditional {
            if_true, if_false, ..
        } => {
            replace_it_target_in_effects(if_true, target);
            replace_it_target_in_effects(if_false, target);
        }
        EffectAst::UnlessPays { effects, .. }
        | EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachPlayerDoesNot { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::VoteOption { effects, .. } => {
            replace_it_target_in_effects(effects, target);
        }
        EffectAst::UnlessAction {
            effects,
            alternative,
            ..
        } => {
            replace_it_target_in_effects(effects, target);
            replace_it_target_in_effects(alternative, target);
        }
        _ => {}
    }
}

fn target_references_it(target: &TargetAst) -> bool {
    match target {
        TargetAst::Tagged(tag, _) => tag.as_str() == IT_TAG,
        TargetAst::Object(filter, _, _) => filter
            .tagged_constraints
            .iter()
            .any(|constraint| constraint.tag.as_str() == IT_TAG),
        TargetAst::WithCount(inner, _) => target_references_it(inner),
        _ => false,
    }
}

fn is_that_turn_end_step_sentence(tokens: &[Token]) -> bool {
    let clause_words = words(tokens);
    clause_words.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "that",
        "turn",
        "end",
        "step",
    ]) || clause_words.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "that",
        "turns",
        "end",
        "step",
    ])
}

fn most_recent_extra_turn_player(effects: &[EffectAst]) -> Option<PlayerAst> {
    effects.iter().rev().find_map(|effect| {
        if let EffectAst::ExtraTurnAfterTurn { player } = effect {
            Some(*player)
        } else {
            None
        }
    })
}

fn rewrite_when_you_do_clause_prefix(tokens: &[Token]) -> Vec<Token> {
    let clause_words = words(tokens);
    if clause_words.starts_with(&["when", "you", "do"]) {
        let mut rewritten = tokens.to_vec();
        for token in &mut rewritten {
            if let Token::Word(word, _) = token {
                if word.eq_ignore_ascii_case("when") {
                    *word = "if".to_string();
                }
                break;
            }
        }
        return rewritten;
    }

    // Generic "When one or more ... this way, ..." follow-ups are semantically
    // "If you do, ..." against the immediately previous effect result.
    let has_this_way = clause_words
        .windows(2)
        .any(|window| window == ["this", "way"]);
    if (clause_words.starts_with(&["when", "one", "or", "more"])
        || clause_words.starts_with(&["whenever", "one", "or", "more"]))
        && has_this_way
    {
        let Some(comma_idx) = tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)))
        else {
            return tokens.to_vec();
        };
        let mut rewritten = Vec::new();

        let mut if_token = tokens[0].clone();
        if let Token::Word(word, _) = &mut if_token {
            *word = "if".to_string();
        }
        rewritten.push(if_token);

        let mut you_token = tokens.get(1).cloned().unwrap_or_else(|| tokens[0].clone());
        if let Token::Word(word, _) = &mut you_token {
            *word = "you".to_string();
        }
        rewritten.push(you_token);

        let mut do_token = tokens.get(2).cloned().unwrap_or_else(|| tokens[0].clone());
        if let Token::Word(word, _) = &mut do_token {
            *word = "do".to_string();
        }
        rewritten.push(do_token);

        rewritten.push(tokens[comma_idx].clone());
        rewritten.extend_from_slice(&tokens[comma_idx + 1..]);
        return rewritten;
    }

    tokens.to_vec()
}

fn strip_otherwise_sentence_prefix(tokens: &[Token]) -> Option<Vec<Token>> {
    if !tokens
        .first()
        .is_some_and(|token| token.is_word("otherwise"))
    {
        return None;
    }

    let mut idx = 1usize;
    while matches!(tokens.get(idx), Some(Token::Comma(_))) {
        idx += 1;
    }
    if tokens.get(idx).is_some_and(|token| token.is_word("then")) {
        idx += 1;
    }
    while matches!(tokens.get(idx), Some(Token::Comma(_))) {
        idx += 1;
    }

    let remainder = trim_commas(&tokens[idx..]);
    if remainder.is_empty() {
        None
    } else {
        Some(remainder)
    }
}

fn rewrite_otherwise_referential_subject(tokens: Vec<Token>) -> Vec<Token> {
    let clause_words = words(&tokens);
    let is_referential_get = clause_words.len() >= 3
        && clause_words[0] == "that"
        && matches!(clause_words[1], "creature" | "permanent")
        && matches!(clause_words[2], "gets" | "get" | "gains" | "gain");
    if !is_referential_get {
        return tokens;
    }

    let mut rewritten = tokens;
    if let Some(first) = rewritten.get_mut(0)
        && let Token::Word(word, _) = first
    {
        *word = "target".to_string();
    }
    rewritten
}

fn is_nonsemantic_restriction_sentence(tokens: &[Token]) -> bool {
    is_activate_only_restriction_sentence(tokens) || is_trigger_only_restriction_sentence(tokens)
}

fn try_apply_token_copy_followup(
    effects: &mut [EffectAst],
    sentence_effects: &[EffectAst],
) -> Result<bool, CardTextError> {
    if sentence_effects.len() != 1 {
        return Ok(false);
    }

    let Some(last) = effects.last_mut() else {
        return Ok(false);
    };

    let Some((haste, sacrifice, exile_next_end_step, exile_end_of_combat)) =
        (match sentence_effects.first() {
            Some(EffectAst::TokenCopyHasHaste) => Some((true, false, false, false)),
            Some(EffectAst::TokenCopySacrificeAtNextEndStep) => Some((false, true, false, false)),
            Some(EffectAst::TokenCopyExileAtNextEndStep) => Some((false, false, true, false)),
            Some(EffectAst::ExileThatTokenAtEndOfCombat) => Some((false, false, false, true)),
            _ => None,
        })
    else {
        return Ok(false);
    };

    match last {
        EffectAst::CreateTokenCopy {
            has_haste,
            exile_at_end_of_combat,
            sacrifice_at_next_end_step,
            exile_at_next_end_step,
            ..
        }
        | EffectAst::CreateTokenCopyFromSource {
            has_haste,
            exile_at_end_of_combat,
            sacrifice_at_next_end_step,
            exile_at_next_end_step,
            ..
        } => {
            if haste {
                *has_haste = true;
            }
            if sacrifice {
                *sacrifice_at_next_end_step = true;
            }
            if exile_next_end_step {
                *exile_at_next_end_step = true;
            }
            if exile_end_of_combat {
                *exile_at_end_of_combat = true;
            }
            Ok(true)
        }
        EffectAst::CreateTokenWithMods {
            exile_at_end_of_combat,
            ..
        } if exile_end_of_combat => {
            *exile_at_end_of_combat = true;
            Ok(true)
        }
        EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. } => {
            if !exile_end_of_combat {
                return Ok(false);
            }
            if effects.is_empty() {
                return Ok(false);
            }
            try_apply_token_copy_followup(effects.as_mut_slice(), sentence_effects)
        }
        _ => Ok(false),
    }
}

type SentencePrimitiveParser = fn(&[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError>;

struct SentencePrimitive {
    name: &'static str,
    parser: SentencePrimitiveParser,
}

fn run_sentence_primitives(
    tokens: &[Token],
    primitives: &[SentencePrimitive],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    for primitive in primitives {
        match (primitive.parser)(tokens) {
            Ok(Some(effects)) => {
                let stage = format!("parse_effect_sentence:primitive-hit:{}", primitive.name);
                parser_trace(&stage, tokens);
                if effects.is_empty() {
                    return Err(CardTextError::ParseError(format!(
                        "primitive '{}' produced empty effects (clause: '{}')",
                        primitive.name,
                        words(tokens).join(" ")
                    )));
                }
                return Ok(Some(effects));
            }
            Ok(None) => {}
            Err(err) => {
                if parser_trace_enabled() {
                    eprintln!(
                        "[parser-flow] stage=parse_effect_sentence:primitive-error primitive={} clause='{}' error={err:?}",
                        primitive.name,
                        words(tokens).join(" ")
                    );
                }
                return Err(err);
            }
        }
    }
    Ok(None)
}

fn parse_you_and_target_player_each_draw_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.len() < 6 {
        return Ok(None);
    }
    if !clause_words.starts_with(&["you", "and", "target"]) {
        return Ok(None);
    }

    let target_player = match clause_words.get(3).copied() {
        Some("opponent" | "opponents") => PlayerAst::TargetOpponent,
        Some("player" | "players") => PlayerAst::Target,
        _ => return Ok(None),
    };

    let mut idx = 4usize;

    if clause_words.get(idx) == Some(&"each") {
        idx += 1;
    }
    if !matches!(clause_words.get(idx).copied(), Some("draw" | "draws")) {
        return Ok(None);
    }
    idx += 1;

    let remainder_words = &clause_words[idx..];
    if remainder_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing draw count in shared draw sentence (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let synthetic_tokens = remainder_words
        .iter()
        .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
        .collect::<Vec<_>>();
    let (count, used) = parse_value(&synthetic_tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing draw count in shared draw sentence (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;
    if synthetic_tokens
        .get(used)
        .and_then(Token::as_word)
        .is_none_or(|word| word != "card" && word != "cards")
    {
        return Err(CardTextError::ParseError(format!(
            "missing card keyword in shared draw sentence (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let trailing_words = words(&synthetic_tokens[used + 1..]);
    if !trailing_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing shared draw clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(Some(vec![
        EffectAst::Draw {
            count: count.clone(),
            player: PlayerAst::You,
        },
        EffectAst::Draw {
            count,
            player: target_player,
        },
    ]))
}

fn parse_sentence_you_and_target_player_each_draw(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_you_and_target_player_each_draw_sentence(tokens)
}

fn parse_sentence_you_and_attacking_player_each_draw_and_lose(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.len() < 11 || !clause_words.starts_with(&["you", "and"]) {
        return Ok(None);
    }

    let mut idx = 2usize;
    if clause_words.get(idx) == Some(&"the") {
        idx += 1;
    }
    if clause_words.get(idx) != Some(&"attacking") || clause_words.get(idx + 1) != Some(&"player") {
        return Ok(None);
    }
    idx += 2;

    if clause_words.get(idx) == Some(&"each") {
        idx += 1;
    }
    if !matches!(clause_words.get(idx).copied(), Some("draw" | "draws")) {
        return Ok(None);
    }
    idx += 1;

    let draw_tokens = clause_words[idx..]
        .iter()
        .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
        .collect::<Vec<_>>();
    let (draw_count, draw_used) = parse_value(&draw_tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing shared draw count (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;
    if draw_tokens
        .get(draw_used)
        .and_then(Token::as_word)
        .is_none_or(|word| word != "card" && word != "cards")
    {
        return Err(CardTextError::ParseError(format!(
            "missing card keyword in shared draw/lose sentence (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let after_draw_words = words(&draw_tokens[draw_used + 1..]);
    if after_draw_words.first() != Some(&"and")
        || !matches!(after_draw_words.get(1).copied(), Some("lose" | "loses"))
    {
        return Ok(None);
    }

    let lose_tokens = after_draw_words[2..]
        .iter()
        .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
        .collect::<Vec<_>>();
    let (lose_amount, lose_used) = parse_value(&lose_tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing shared life-loss amount (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;
    if lose_tokens
        .get(lose_used)
        .and_then(Token::as_word)
        .is_none_or(|word| word != "life")
    {
        return Err(CardTextError::ParseError(format!(
            "missing life keyword in shared draw/lose sentence (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let trailing_words = words(&lose_tokens[lose_used + 1..]);
    if !trailing_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing shared draw/lose clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(Some(vec![
        EffectAst::Draw {
            count: draw_count.clone(),
            player: PlayerAst::You,
        },
        EffectAst::Draw {
            count: draw_count,
            player: PlayerAst::Attacking,
        },
        EffectAst::LoseLife {
            amount: lose_amount.clone(),
            player: PlayerAst::You,
        },
        EffectAst::LoseLife {
            amount: lose_amount,
            player: PlayerAst::Attacking,
        },
    ]))
}

fn parse_sentence_token_copy_modifier(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let effect = parse_token_copy_modifier_sentence(tokens);
    if effect.is_some() && tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "token copy modifier sentence missing tokens".to_string(),
        ));
    }
    Ok(effect.map(|effect| vec![effect]))
}

fn parse_sentence_sacrifice_it_next_end_step(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens
        .first()
        .is_some_and(|token| token.is_word("sacrifice"))
    {
        return Ok(None);
    }

    let Some(at_idx) = tokens.iter().position(|token| token.is_word("at")) else {
        return Ok(None);
    };
    if at_idx <= 1 {
        return Ok(None);
    }

    let timing_words = words(&tokens[at_idx..]);
    let matches_sacrifice_delay = timing_words.as_slice()
        == ["at", "the", "beginning", "of", "the", "next", "end", "step"]
        || timing_words.as_slice() == ["at", "the", "beginning", "of", "next", "end", "step"];
    if !matches_sacrifice_delay {
        return Ok(None);
    }

    let object_tokens = trim_commas(&tokens[1..at_idx]);
    if object_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing sacrifice object in delayed next-end-step clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let object_words = words(&object_tokens);
    let filter = if matches!(
        object_words.as_slice(),
        ["it"]
            | ["them"]
            | ["the", "creature"]
            | ["that", "creature"]
            | ["the", "permanent"]
            | ["that", "permanent"]
            | ["the", "token"]
            | ["that", "token"]
    ) {
        ObjectFilter::tagged(TagKey::from(IT_TAG))
    } else {
        parse_object_filter(&object_tokens, false)?
    };

    Ok(Some(vec![EffectAst::DelayedUntilNextEndStep {
        player: PlayerFilter::Any,
        effects: vec![EffectAst::Sacrifice {
            filter,
            player: PlayerAst::Implicit,
            count: 1,
        }],
    }]))
}

fn parse_sentence_sacrifice_at_end_of_combat(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens
        .first()
        .is_some_and(|token| token.is_word("sacrifice"))
    {
        return Ok(None);
    }
    let Some(at_idx) = tokens.iter().position(|token| token.is_word("at")) else {
        return Ok(None);
    };
    if at_idx <= 1 {
        return Ok(None);
    }

    let timing_words = words(&tokens[at_idx..]);
    let matches_end_of_combat = timing_words.as_slice() == ["at", "end", "of", "combat"]
        || timing_words.as_slice() == ["at", "the", "end", "of", "combat"];
    if !matches_end_of_combat {
        return Ok(None);
    }

    let object_tokens = trim_commas(&tokens[1..at_idx]);
    if object_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing sacrifice object in end-of-combat clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let object_words = words(&object_tokens);
    let filter = if matches!(
        object_words.as_slice(),
        ["it"]
            | ["them"]
            | ["that", "token"]
            | ["this", "token"]
            | ["that", "permanent"]
            | ["this", "permanent"]
    ) {
        ObjectFilter::tagged(TagKey::from(IT_TAG))
    } else {
        parse_object_filter(&object_tokens, false)?
    };

    Ok(Some(vec![EffectAst::DelayedUntilEndOfCombat {
        effects: vec![EffectAst::Sacrifice {
            filter,
            player: PlayerAst::Implicit,
            count: 1,
        }],
    }]))
}

fn parse_sentence_each_player_choose_and_sacrifice_rest(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_each_player_choose_and_sacrifice_rest(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_exile_instead_of_graveyard(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_exile_instead_of_graveyard_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_monstrosity(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_monstrosity_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_for_each_counter_removed(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_for_each_counter_removed_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_put_counter_ladder_segments(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let segments = split_on_comma(tokens);
    if segments.len() != 3 {
        return Ok(None);
    }

    let mut effects = Vec::new();
    for (idx, segment) in segments.iter().enumerate() {
        let mut clause = trim_commas(segment).to_vec();
        if idx == 0 {
            if clause.is_empty() || !clause[0].is_word("put") {
                return Ok(None);
            }
            clause.remove(0);
        } else if clause.first().is_some_and(|token| token.is_word("and")) {
            clause.remove(0);
        }
        if clause.is_empty() {
            return Ok(None);
        }

        let Some(on_idx) = clause.iter().position(|token| token.is_word("on")) else {
            return Ok(None);
        };
        let descriptor = trim_commas(&clause[..on_idx]);
        let target_tokens = trim_commas(&clause[on_idx + 1..]);
        if descriptor.is_empty() || target_tokens.is_empty() {
            return Ok(None);
        }

        let (count, counter_type) = parse_counter_descriptor(&descriptor)?;
        let target = parse_target_phrase(&target_tokens)?;
        effects.push(EffectAst::PutCounters {
            counter_type,
            count: Value::Fixed(count as i32),
            target,
            target_count: None,
            distributed: false,
        });
    }

    Ok(Some(effects))
}

fn parse_sentence_put_counter_sequence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens.first().is_some_and(|token| token.is_word("put")) {
        return Ok(None);
    }
    if !tokens
        .iter()
        .any(|token| token.is_word("counter") || token.is_word("counters"))
    {
        return Ok(None);
    }

    if let Some(effects) = parse_put_counter_ladder_segments(tokens)? {
        return Ok(Some(effects));
    }

    if let Some(on_idx) = tokens.iter().position(|token| token.is_word("on")) {
        let descriptor_tokens = trim_commas(&tokens[1..on_idx]);
        let target_tokens = trim_commas(&tokens[on_idx + 1..]);
        if !descriptor_tokens.is_empty() && !target_tokens.is_empty() {
            let mut descriptors: Vec<Vec<Token>> = Vec::new();
            let comma_segments = split_on_comma(&descriptor_tokens);
            if comma_segments.len() >= 2 {
                for segment in comma_segments {
                    let mut clause = trim_commas(&segment);
                    if clause.first().is_some_and(|token| token.is_word("and")) {
                        clause.remove(0);
                    }
                    if clause.is_empty() {
                        descriptors.clear();
                        break;
                    }
                    descriptors.push(clause);
                }
            } else if let Some(and_idx) = descriptor_tokens
                .iter()
                .position(|token| token.is_word("and"))
            {
                let first = trim_commas(&descriptor_tokens[..and_idx]);
                let second = trim_commas(&descriptor_tokens[and_idx + 1..]);
                if !first.is_empty() && !second.is_empty() {
                    descriptors.push(first);
                    descriptors.push(second);
                }
            }

            if descriptors.len() >= 2 {
                let target = parse_target_phrase(&target_tokens)?;
                let mut effects = Vec::new();
                for descriptor in descriptors {
                    let (count, counter_type) = parse_counter_descriptor(&descriptor)?;
                    effects.push(EffectAst::PutCounters {
                        counter_type,
                        count: Value::Fixed(count as i32),
                        target: target.clone(),
                        target_count: None,
                        distributed: false,
                    });
                }
                return Ok(Some(effects));
            }
        }
    }

    // Handle "put ... counter on X and it gains ... until end of turn."
    if let Some(and_idx) = tokens
        .windows(2)
        .position(|window| window[0].is_word("and") && window[1].is_word("it"))
    {
        let first_clause = trim_commas(&tokens[1..and_idx]);
        let second_clause = trim_commas(&tokens[and_idx + 1..]);
        if !first_clause.is_empty()
            && !second_clause.is_empty()
            && second_clause.iter().any(|token| {
                token.is_word("gain")
                    || token.is_word("gains")
                    || token.is_word("has")
                    || token.is_word("have")
            })
            && let Ok(first) = parse_put_counters(&first_clause)
            && let Some(mut gain_effects) = parse_gain_ability_sentence(&second_clause)?
        {
            let source_target = match &first {
                EffectAst::PutCounters { target, .. } => Some(target.clone()),
                EffectAst::Conditional { if_true, .. }
                    if if_true.len() == 1
                        && matches!(if_true.first(), Some(EffectAst::PutCounters { .. })) =>
                {
                    if let Some(EffectAst::PutCounters { target, .. }) = if_true.first() {
                        Some(target.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(source_target) = source_target {
                for effect in &mut gain_effects {
                    match effect {
                        EffectAst::Pump { target, .. }
                        | EffectAst::GrantAbilitiesToTarget { target, .. }
                        | EffectAst::GrantAbilitiesChoiceToTarget { target, .. } => {
                            if let TargetAst::Tagged(tag, _) = target
                                && tag.as_str() == IT_TAG
                            {
                                *target = source_target.clone();
                            }
                        }
                        _ => {}
                    }
                }

                let mut effects = vec![first];
                effects.append(&mut gain_effects);
                return Ok(Some(effects));
            }
        }
    }

    // Handle "put ... and ... counter on ..." without comma separation.
    if let Some(and_idx) = tokens.iter().position(|token| token.is_word("and")) {
        let first_clause = trim_commas(&tokens[1..and_idx]);
        let second_clause = trim_commas(&tokens[and_idx + 1..]);
        if !first_clause.is_empty() && !second_clause.is_empty() {
            if let (Ok(first), Ok(second)) = (
                parse_put_counters(&first_clause),
                parse_put_counters(&second_clause),
            ) {
                return Ok(Some(vec![first, second]));
            }
        }
    }

    let segments = split_on_comma(tokens);
    if segments.len() < 2 {
        return Ok(None);
    }

    let mut effects = Vec::new();
    for (idx, segment) in segments.iter().enumerate() {
        let mut clause = segment.clone();
        if idx == 0 {
            if clause.is_empty() || !clause[0].is_word("put") {
                return Ok(None);
            }
            clause.remove(0);
        } else if clause.first().is_some_and(|token| token.is_word("and")) {
            clause.remove(0);
        }

        if clause.is_empty() {
            return Ok(None);
        }

        let clause_words = words(&clause);
        if !clause_words.contains(&"counter") && !clause_words.contains(&"counters") {
            return Ok(None);
        }

        let Ok(effect) = parse_put_counters(&clause) else {
            return Ok(None);
        };
        effects.push(effect);
    }

    if effects.len() >= 2 {
        Ok(Some(effects))
    } else {
        Ok(None)
    }
}

fn is_pump_like_effect(effect: &EffectAst) -> bool {
    matches!(
        effect,
        EffectAst::Pump { .. }
            | EffectAst::PumpByLastEffect { .. }
            | EffectAst::SetBasePowerToughness { .. }
            | EffectAst::SetBasePower { .. }
    )
}

fn parse_gets_then_fights_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let mut body_tokens = tokens;
    if body_tokens
        .first()
        .is_some_and(|token| token.is_word("then"))
    {
        body_tokens = &body_tokens[1..];
    }
    if body_tokens.is_empty() {
        return Ok(None);
    }

    let Some(fight_idx) = body_tokens
        .iter()
        .position(|token| token.is_word("fight") || token.is_word("fights"))
    else {
        return Ok(None);
    };
    if fight_idx == 0 || fight_idx + 1 >= body_tokens.len() {
        return Ok(None);
    }

    let mut left_tokens = trim_commas(&body_tokens[..fight_idx]).to_vec();
    while left_tokens.last().is_some_and(|token| token.is_word("and")) {
        left_tokens.pop();
    }
    let left_tokens = trim_commas(&left_tokens);
    let right_tokens = trim_commas(&body_tokens[fight_idx + 1..]);
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return Ok(None);
    }

    let Some(get_idx) = left_tokens
        .iter()
        .position(|token| token.is_word("get") || token.is_word("gets"))
    else {
        return Ok(None);
    };
    if get_idx == 0 {
        return Ok(None);
    }

    let pump_effect = parse_effect_clause(&left_tokens)?;
    if !is_pump_like_effect(&pump_effect) {
        return Ok(None);
    }

    let subject_tokens = trim_commas(&left_tokens[..get_idx]);
    if subject_tokens.is_empty() {
        return Ok(None);
    }
    let creature1 = parse_target_phrase(&subject_tokens)?;
    let creature2 = parse_target_phrase(&right_tokens)?;
    if matches!(
        creature1,
        TargetAst::Player(_, _) | TargetAst::PlayerOrPlaneswalker(_, _)
    ) || matches!(
        creature2,
        TargetAst::Player(_, _) | TargetAst::PlayerOrPlaneswalker(_, _)
    ) {
        return Err(CardTextError::ParseError(format!(
            "fight target must be a creature (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    Ok(Some(vec![
        pump_effect,
        EffectAst::Fight {
            creature1,
            creature2,
        },
    ]))
}

fn parse_sentence_gets_then_fights(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_gets_then_fights_sentence(tokens)
}

fn parse_return_with_counters_on_it_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens.first().is_some_and(|token| token.is_word("return")) {
        return Ok(None);
    }

    let Some(to_idx) = tokens.iter().rposition(|token| token.is_word("to")) else {
        return Ok(None);
    };
    if to_idx <= 1 {
        return Ok(None);
    }

    let target_tokens = trim_commas(&tokens[1..to_idx]);
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing return target before destination (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let destination_tokens = trim_commas(&tokens[to_idx + 1..]);
    if destination_tokens.is_empty() {
        return Ok(None);
    }
    if !words(&destination_tokens).contains(&"battlefield") {
        return Ok(None);
    }

    let Some(with_idx) = destination_tokens
        .iter()
        .position(|token| token.is_word("with"))
    else {
        return Ok(None);
    };
    if with_idx + 1 >= destination_tokens.len() {
        return Ok(None);
    }

    let base_destination_words: Vec<&str> = words(&destination_tokens[..with_idx])
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    let Some(battlefield_idx) = base_destination_words
        .iter()
        .position(|word| *word == "battlefield")
    else {
        return Ok(None);
    };
    let tapped = base_destination_words.contains(&"tapped");
    let destination_tail: Vec<&str> = base_destination_words[battlefield_idx + 1..]
        .iter()
        .copied()
        .filter(|word| *word != "tapped")
        .collect();
    let battlefield_controller = if destination_tail.is_empty()
        || destination_tail == ["under", "its", "control"]
        || destination_tail == ["under", "their", "control"]
    {
        ReturnControllerAst::Preserve
    } else if destination_tail == ["under", "your", "control"] {
        ReturnControllerAst::You
    } else if destination_tail == ["under", "its", "owners", "control"]
        || destination_tail == ["under", "their", "owners", "control"]
        || destination_tail == ["under", "that", "players", "control"]
    {
        ReturnControllerAst::Owner
    } else {
        return Ok(None);
    };

    let counter_clause_tokens = trim_commas(&destination_tokens[with_idx + 1..]);
    let Some(on_idx) = counter_clause_tokens
        .iter()
        .rposition(|token| token.is_word("on"))
    else {
        return Ok(None);
    };
    if on_idx + 1 >= counter_clause_tokens.len() {
        return Ok(None);
    }

    let on_target_words = words(&counter_clause_tokens[on_idx + 1..]);
    if on_target_words != ["it"] && on_target_words != ["them"] {
        return Ok(None);
    }

    let descriptor_tokens = trim_commas(&counter_clause_tokens[..on_idx]);
    if descriptor_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing counter descriptor in return-with-counters clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let mut descriptors = Vec::new();
    for descriptor in split_on_and(&descriptor_tokens) {
        let descriptor = trim_commas(&descriptor);
        if descriptor.is_empty() {
            continue;
        }
        descriptors.push(descriptor);
    }
    if descriptors.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing counter descriptor in return-with-counters clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let mut effects = vec![EffectAst::ReturnToBattlefield {
        target: parse_target_phrase(&target_tokens)?,
        tapped,
        controller: battlefield_controller,
    }];
    let tagged_target = TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(tokens));
    for descriptor in descriptors {
        let (count, counter_type) = parse_counter_descriptor(&descriptor)?;
        effects.push(EffectAst::PutCounters {
            counter_type,
            count: Value::Fixed(count as i32),
            target: tagged_target.clone(),
            target_count: None,
            distributed: false,
        });
    }

    Ok(Some(effects))
}

fn parse_put_onto_battlefield_with_counters_on_it_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens
        .first()
        .is_some_and(|token| token.is_word("put") || token.is_word("puts"))
    {
        return Ok(None);
    }

    let Some(onto_idx) = tokens.iter().position(|token| token.is_word("onto")) else {
        return Ok(None);
    };
    if onto_idx <= 1 {
        return Ok(None);
    }

    let target_tokens = trim_commas(&tokens[1..onto_idx]);
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing put target before destination (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let destination_tokens = trim_commas(&tokens[onto_idx + 1..]);
    if destination_tokens.is_empty() {
        return Ok(None);
    }
    if !words(&destination_tokens).contains(&"battlefield") {
        return Ok(None);
    }

    let Some(with_idx) = destination_tokens
        .iter()
        .position(|token| token.is_word("with"))
    else {
        return Ok(None);
    };
    if with_idx + 1 >= destination_tokens.len() {
        return Ok(None);
    }

    let base_destination_words: Vec<&str> = words(&destination_tokens[..with_idx])
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    if base_destination_words.first() != Some(&"battlefield") {
        return Ok(None);
    }

    let destination_tail = &base_destination_words[1..];
    let supported_control_tail = destination_tail.is_empty()
        || destination_tail == ["under", "your", "control"]
        || destination_tail == ["under", "its", "owners", "control"]
        || destination_tail == ["under", "their", "owners", "control"]
        || destination_tail == ["under", "that", "players", "control"];
    if !supported_control_tail {
        return Ok(None);
    }
    let battlefield_controller = if destination_tail == ["under", "your", "control"] {
        ReturnControllerAst::You
    } else if destination_tail == ["under", "its", "owners", "control"]
        || destination_tail == ["under", "their", "owners", "control"]
        || destination_tail == ["under", "that", "players", "control"]
    {
        ReturnControllerAst::Owner
    } else {
        ReturnControllerAst::Preserve
    };

    let counter_clause_tokens = trim_commas(&destination_tokens[with_idx + 1..]);
    let Some(on_idx) = counter_clause_tokens
        .iter()
        .rposition(|token| token.is_word("on"))
    else {
        return Ok(None);
    };
    if on_idx + 1 >= counter_clause_tokens.len() {
        return Ok(None);
    }

    let on_target_words = words(&counter_clause_tokens[on_idx + 1..]);
    if on_target_words != ["it"] && on_target_words != ["them"] {
        return Ok(None);
    }

    let descriptor_tokens = trim_commas(&counter_clause_tokens[..on_idx]);
    let descriptor_words = words(&descriptor_tokens);
    if descriptor_tokens.is_empty() || !descriptor_words.contains(&"counter") {
        return Ok(None);
    }

    let mut descriptors = Vec::new();
    for descriptor in split_on_and(&descriptor_tokens) {
        let descriptor = trim_commas(&descriptor);
        if descriptor.is_empty() {
            continue;
        }
        descriptors.push(descriptor);
    }
    if descriptors.is_empty() {
        return Ok(None);
    }

    let mut effects = vec![EffectAst::MoveToZone {
        target: parse_target_phrase(&target_tokens)?,
        zone: Zone::Battlefield,
        to_top: false,
        battlefield_controller,
    }];
    let tagged_target = TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(tokens));
    for descriptor in descriptors {
        let (count, counter_type) = parse_counter_descriptor(&descriptor)?;
        effects.push(EffectAst::PutCounters {
            counter_type,
            count: Value::Fixed(count as i32),
            target: tagged_target.clone(),
            target_count: None,
            distributed: false,
        });
    }

    Ok(Some(effects))
}

fn parse_sentence_return_with_counters_on_it(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_return_with_counters_on_it_sentence(tokens)
}

fn parse_sentence_put_onto_battlefield_with_counters_on_it(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_put_onto_battlefield_with_counters_on_it_sentence(tokens)
}

fn replace_target_subtype(target: &mut TargetAst, subtype: Subtype) -> bool {
    match target {
        TargetAst::Object(filter, _, _) => {
            filter.subtypes = vec![subtype];
            true
        }
        TargetAst::WithCount(inner, _) => replace_target_subtype(inner, subtype),
        _ => false,
    }
}

fn clone_return_effect_with_subtype(base: &EffectAst, subtype: Subtype) -> Option<EffectAst> {
    match base {
        EffectAst::ReturnToHand { target, random } => {
            let mut cloned_target = target.clone();
            replace_target_subtype(&mut cloned_target, subtype).then_some(EffectAst::ReturnToHand {
                target: cloned_target,
                random: *random,
            })
        }
        EffectAst::ReturnToBattlefield {
            target,
            tapped,
            controller,
        } => {
            let mut cloned_target = target.clone();
            replace_target_subtype(&mut cloned_target, subtype).then_some(
                EffectAst::ReturnToBattlefield {
                    target: cloned_target,
                    tapped: *tapped,
                    controller: *controller,
                },
            )
        }
        EffectAst::ReturnAllToHand { filter } => {
            let mut cloned_filter = filter.clone();
            cloned_filter.subtypes = vec![subtype];
            Some(EffectAst::ReturnAllToHand {
                filter: cloned_filter,
            })
        }
        EffectAst::ReturnAllToBattlefield { filter, tapped } => {
            let mut cloned_filter = filter.clone();
            cloned_filter.subtypes = vec![subtype];
            Some(EffectAst::ReturnAllToBattlefield {
                filter: cloned_filter,
                tapped: *tapped,
            })
        }
        _ => None,
    }
}

fn parse_draw_then_connive_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some(comma_then_idx) = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
    else {
        return Ok(None);
    };

    let head_tokens = trim_commas(&tokens[..comma_then_idx]);
    let tail_tokens = trim_commas(&tokens[comma_then_idx + 2..]);
    if head_tokens.is_empty() || tail_tokens.is_empty() {
        return Ok(None);
    }

    if !tail_tokens
        .iter()
        .any(|token| token.is_word("connive") || token.is_word("connives"))
    {
        return Ok(None);
    }

    let mut head_effects = parse_effect_chain(&head_tokens)?;
    if head_effects.is_empty() {
        return Ok(None);
    }

    let Some(connive_effect) = parse_connive_clause(&tail_tokens)? else {
        return Ok(None);
    };
    head_effects.push(connive_effect);
    Ok(Some(head_effects))
}

fn parse_sentence_draw_then_connive(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_draw_then_connive_sentence(tokens)
}

fn parse_each_player_return_with_additional_counter_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    let inner_start_word_idx = if clause_words.starts_with(&["for", "each", "player"])
        || clause_words.starts_with(&["for", "each", "players"])
    {
        3
    } else if clause_words.starts_with(&["each", "player"])
        || clause_words.starts_with(&["each", "players"])
    {
        2
    } else {
        return Ok(None);
    };

    let Some(inner_start_token_idx) = token_index_for_word_index(tokens, inner_start_word_idx)
    else {
        return Ok(None);
    };
    let inner_tokens = trim_commas(&tokens[inner_start_token_idx..]);
    if inner_tokens.is_empty() {
        return Ok(None);
    }
    if !inner_tokens
        .first()
        .is_some_and(|token| token.is_word("return") || token.is_word("returns"))
    {
        return Ok(None);
    }

    let Some(with_idx) = inner_tokens.iter().rposition(|token| token.is_word("with")) else {
        return Ok(None);
    };
    if with_idx + 1 >= inner_tokens.len() {
        return Ok(None);
    }

    let return_clause_tokens = trim_commas(&inner_tokens[..with_idx]);
    if return_clause_tokens.is_empty() {
        return Ok(None);
    }

    let counter_clause_tokens = trim_commas(&inner_tokens[with_idx + 1..]);
    let Some(on_idx) = counter_clause_tokens
        .iter()
        .rposition(|token| token.is_word("on"))
    else {
        return Ok(None);
    };
    if on_idx + 1 >= counter_clause_tokens.len() {
        return Ok(None);
    }

    let on_target_words = words(&counter_clause_tokens[on_idx + 1..]);
    if on_target_words != ["it"] && on_target_words != ["them"] {
        return Ok(None);
    }

    let descriptor_tokens = trim_commas(&counter_clause_tokens[..on_idx]);
    let descriptor_words = words(&descriptor_tokens);
    if descriptor_tokens.is_empty() || !descriptor_words.contains(&"additional") {
        return Ok(None);
    }

    let (count, counter_type) = parse_counter_descriptor(&descriptor_tokens)?;
    let mut per_player_effects = parse_effect_chain_inner(&return_clause_tokens)?;
    if per_player_effects.is_empty() {
        return Ok(None);
    }
    if !per_player_effects.iter().any(|effect| {
        matches!(
            effect,
            EffectAst::ReturnToBattlefield { .. } | EffectAst::ReturnAllToBattlefield { .. }
        )
    }) {
        return Ok(None);
    }

    per_player_effects.push(EffectAst::PutCounters {
        counter_type,
        count: Value::Fixed(count as i32),
        target: TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(tokens)),
        target_count: None,
        distributed: false,
    });

    Ok(Some(vec![EffectAst::ForEachPlayer {
        effects: per_player_effects,
    }]))
}

fn parse_sentence_each_player_return_with_additional_counter(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_each_player_return_with_additional_counter_sentence(tokens)
}

fn parse_return_then_do_same_for_subtypes_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens.first().is_some_and(|token| token.is_word("return")) {
        return Ok(None);
    }
    let Some(comma_then_idx) = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
    else {
        return Ok(None);
    };

    let head_tokens = trim_commas(&tokens[..comma_then_idx]);
    let tail_tokens = trim_commas(&tokens[comma_then_idx + 2..]);
    if head_tokens.is_empty() || tail_tokens.is_empty() {
        return Ok(None);
    }

    let tail_words = words(&tail_tokens);
    if !tail_words.starts_with(&["do", "the", "same", "for"]) {
        return Ok(None);
    }
    let subtype_words = &tail_words[4..];
    if subtype_words.is_empty() {
        return Ok(None);
    }

    let mut extra_subtypes = Vec::new();
    for word in subtype_words {
        if matches!(*word, "and" | "or") {
            continue;
        }
        let Some(subtype) = parse_subtype_word(word)
            .or_else(|| word.strip_suffix('s').and_then(parse_subtype_word))
        else {
            return Ok(None);
        };
        extra_subtypes.push(subtype);
    }
    if extra_subtypes.is_empty() {
        return Ok(None);
    }

    let mut effects = parse_effect_chain(&head_tokens)?;
    if effects.len() != 1 {
        return Ok(None);
    }
    let base_effect = effects[0].clone();
    for subtype in extra_subtypes {
        let Some(cloned) = clone_return_effect_with_subtype(&base_effect, subtype) else {
            return Ok(None);
        };
        effects.push(cloned);
    }

    Ok(Some(effects))
}

fn parse_sentence_return_then_do_same_for_subtypes(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_return_then_do_same_for_subtypes_sentence(tokens)
}

fn parse_sacrifice_any_number_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let (head_tokens, tail_tokens) =
        if let Some(then_idx) = tokens.iter().position(|token| token.is_word("then")) {
            if then_idx == 0 {
                return Ok(None);
            }
            (
                trim_commas(&tokens[..then_idx]),
                Some(trim_commas(&tokens[then_idx + 1..])),
            )
        } else {
            (tokens.to_vec(), None)
        };

    if !head_tokens
        .first()
        .is_some_and(|token| token.is_word("sacrifice"))
    {
        return Ok(None);
    }

    let mut idx = 1usize;
    if !(head_tokens
        .get(idx)
        .is_some_and(|token| token.is_word("any"))
        && head_tokens
            .get(idx + 1)
            .is_some_and(|token| token.is_word("number")))
    {
        return Ok(None);
    }
    idx += 2;
    if head_tokens
        .get(idx)
        .is_some_and(|token| token.is_word("of"))
    {
        idx += 1;
    }
    if idx >= head_tokens.len() {
        return Err(CardTextError::ParseError(format!(
            "missing object after 'sacrifice any number of' (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let filter_tokens = trim_commas(&head_tokens[idx..]);
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing object after 'sacrifice any number of' (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let filter = parse_object_filter(&filter_tokens, false)?;
    let tag = TagKey::from(IT_TAG);

    let mut effects = vec![
        EffectAst::ChooseObjects {
            filter,
            count: ChoiceCount::any_number(),
            player: PlayerAst::Implicit,
            tag: tag.clone(),
        },
        EffectAst::SacrificeAll {
            filter: ObjectFilter::tagged(tag),
            player: PlayerAst::Implicit,
        },
    ];
    if let Some(tail_tokens) = tail_tokens
        && !tail_tokens.is_empty()
    {
        let mut tail_effects = parse_effect_chain(&tail_tokens)?;
        effects.append(&mut tail_effects);
    }

    Ok(Some(effects))
}

fn parse_sentence_sacrifice_any_number(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_sacrifice_any_number_sentence(tokens)
}

fn parse_sacrifice_one_or_more_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens
        .first()
        .is_some_and(|token| token.is_word("sacrifice"))
    {
        return Ok(None);
    }

    let mut idx = 1usize;
    let Some((minimum, used)) = parse_number(&tokens[idx..]) else {
        return Ok(None);
    };
    idx += used;
    if !(tokens.get(idx).is_some_and(|token| token.is_word("or"))
        && tokens
            .get(idx + 1)
            .is_some_and(|token| token.is_word("more")))
    {
        return Ok(None);
    }
    idx += 2;
    if tokens.get(idx).is_some_and(|token| token.is_word("of")) {
        idx += 1;
    }
    if idx >= tokens.len() {
        return Err(CardTextError::ParseError(format!(
            "missing object after 'sacrifice one or more' (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let filter_tokens = trim_commas(&tokens[idx..]);
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing object after 'sacrifice one or more' (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    let filter = parse_object_filter(&filter_tokens, false)?;
    let tag = TagKey::from(IT_TAG);
    Ok(Some(vec![
        EffectAst::ChooseObjects {
            filter,
            count: ChoiceCount::at_least(minimum as usize),
            player: PlayerAst::Implicit,
            tag: tag.clone(),
        },
        EffectAst::SacrificeAll {
            filter: ObjectFilter::tagged(tag),
            player: PlayerAst::Implicit,
        },
    ]))
}

fn parse_sentence_sacrifice_one_or_more(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_sacrifice_one_or_more_sentence(tokens)
}

fn parse_sentence_keyword_then_chain(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some(then_idx) = tokens.iter().position(|token| token.is_word("then")) else {
        return Ok(None);
    };
    if then_idx == 0 || then_idx + 1 >= tokens.len() {
        return Ok(None);
    }

    let head_tokens = trim_commas(&tokens[..then_idx]);
    let Some(head_effect) = parse_keyword_mechanic_clause(&head_tokens)? else {
        return Ok(None);
    };

    let tail_tokens = trim_commas(&tokens[then_idx + 1..]);
    if tail_tokens.is_empty() {
        return Ok(Some(vec![head_effect]));
    }

    let mut effects = vec![head_effect];
    if let Some(mut counter_effects) = parse_sentence_put_counter_sequence(&tail_tokens)? {
        effects.append(&mut counter_effects);
        return Ok(Some(effects));
    }

    let mut tail_effects = parse_effect_chain(&tail_tokens)?;
    effects.append(&mut tail_effects);
    Ok(Some(effects))
}

fn parse_sentence_chain_then_keyword(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let split = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
        .map(|idx| (idx, idx + 2))
        .or_else(|| {
            tokens
                .iter()
                .position(|token| token.is_word("then"))
                .and_then(|idx| (idx > 0 && idx + 1 < tokens.len()).then_some((idx, idx + 1)))
        });
    let Some((head_end, tail_start)) = split else {
        return Ok(None);
    };

    let head_tokens = trim_commas(&tokens[..head_end]);
    let tail_tokens = trim_commas(&tokens[tail_start..]);
    if head_tokens.is_empty() || tail_tokens.is_empty() {
        return Ok(None);
    }

    let Some(keyword_effect) = parse_keyword_mechanic_clause(&tail_tokens)? else {
        return Ok(None);
    };
    let mut head_effects = parse_effect_chain(&head_tokens)?;
    if head_effects.is_empty() {
        return Ok(None);
    }
    head_effects.push(keyword_effect);
    Ok(Some(head_effects))
}

fn parse_sentence_return_then_create(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let split = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
        .map(|idx| (idx, idx + 2))
        .or_else(|| {
            tokens
                .iter()
                .position(|token| token.is_word("then"))
                .and_then(|idx| (idx > 0 && idx + 1 < tokens.len()).then_some((idx, idx + 1)))
        });
    let Some((head_end, tail_start)) = split else {
        return Ok(None);
    };

    let head_tokens = trim_commas(&tokens[..head_end]);
    let tail_tokens = trim_commas(&tokens[tail_start..]);
    if head_tokens.is_empty() || tail_tokens.is_empty() {
        return Ok(None);
    }

    let head_words = words(&head_tokens);
    let tail_words = words(&tail_tokens);
    if head_words.first() != Some(&"return") || tail_words.first() != Some(&"create") {
        return Ok(None);
    }

    let mut head_effects = parse_effect_chain(&head_tokens)?;
    if head_effects.is_empty() {
        return Ok(None);
    }

    let mut tail_effects = parse_effect_chain(&tail_tokens)?;
    if tail_effects.is_empty() {
        return Ok(None);
    }

    head_effects.append(&mut tail_effects);
    Ok(Some(head_effects))
}

fn parse_sentence_exile_then_may_put_from_exile(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let split = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
        .map(|idx| (idx, idx + 2))
        .or_else(|| {
            tokens
                .iter()
                .position(|token| token.is_word("then"))
                .and_then(|idx| (idx > 0 && idx + 1 < tokens.len()).then_some((idx, idx + 1)))
        });
    let Some((head_end, tail_start)) = split else {
        return Ok(None);
    };

    let head_tokens = trim_commas(&tokens[..head_end]);
    let tail_tokens = trim_commas(&tokens[tail_start..]);
    if head_tokens.is_empty() || tail_tokens.is_empty() {
        return Ok(None);
    }

    let tail_words = words(&tail_tokens);
    if !tail_words.starts_with(&["you", "may", "put", "any", "number", "of"])
        || !tail_words.contains(&"from")
        || !tail_words.contains(&"exile")
        || !tail_words.contains(&"battlefield")
    {
        return Ok(None);
    }

    let mut head_effects = parse_effect_chain(&head_tokens)?;
    if head_effects.is_empty() {
        return Ok(None);
    }
    let mut tail_effects = parse_effect_chain(&tail_tokens)?;
    if tail_effects.is_empty() {
        return Ok(None);
    }

    head_effects.append(&mut tail_effects);
    Ok(Some(head_effects))
}

fn parse_exile_source_with_counters_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens.first().is_some_and(|token| token.is_word("exile")) {
        return Ok(None);
    }

    let Some(with_idx) = tokens.iter().position(|token| token.is_word("with")) else {
        return Ok(None);
    };
    if with_idx <= 1 || with_idx + 1 >= tokens.len() {
        return Ok(None);
    }

    let source_name_tokens = trim_commas(&tokens[1..with_idx]);
    if source_name_tokens.is_empty() {
        return Ok(None);
    }
    let source_name_words = words(&source_name_tokens);
    if !is_likely_named_or_source_reference_words(&source_name_words) {
        return Ok(None);
    }

    let counter_clause_tokens = trim_commas(&tokens[with_idx + 1..]);
    let Some(on_idx) = counter_clause_tokens
        .iter()
        .rposition(|token| token.is_word("on"))
    else {
        return Ok(None);
    };
    if on_idx + 1 >= counter_clause_tokens.len() {
        return Ok(None);
    }

    let on_target_words = words(&counter_clause_tokens[on_idx + 1..]);
    if on_target_words != ["it"] && on_target_words != ["them"] {
        return Ok(None);
    }

    let descriptor_tokens = trim_commas(&counter_clause_tokens[..on_idx]);
    if descriptor_tokens.is_empty() {
        return Ok(None);
    }
    let (count, counter_type) = parse_counter_descriptor(&descriptor_tokens)?;

    let source_target = TargetAst::Source(span_from_tokens(tokens));
    Ok(Some(vec![
        EffectAst::Exile {
            target: source_target.clone(),
            face_down: false,
        },
        EffectAst::PutCounters {
            counter_type,
            count: Value::Fixed(count as i32),
            target: source_target,
            target_count: None,
            distributed: false,
        },
    ]))
}

fn parse_sentence_exile_source_with_counters(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_exile_source_with_counters_sentence(tokens)
}

fn parse_sentence_comma_then_chain_special(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some(comma_then_idx) = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
    else {
        return Ok(None);
    };

    let head_tokens = trim_commas(&tokens[..comma_then_idx]);
    let tail_tokens = trim_commas(&tokens[comma_then_idx + 2..]);
    if head_tokens.is_empty() || tail_tokens.is_empty() {
        return Ok(None);
    }

    let head_words = words(&head_tokens);
    let tail_words = words(&tail_tokens);
    let is_that_player_tail = tail_words.starts_with(&["that", "player"]);
    let is_return_source_tail = tail_words.starts_with(&["return", "this"])
        && (tail_words.contains(&"owner") || tail_words.contains(&"owners"))
        && tail_words.contains(&"hand");
    if !is_that_player_tail && !is_return_source_tail {
        return Ok(None);
    }
    if is_return_source_tail
        && !head_words
            .first()
            .is_some_and(|word| matches!(*word, "tap" | "untap"))
    {
        return Ok(None);
    }

    let mut head_effects = parse_effect_chain(&head_tokens)?;
    if head_effects.is_empty() {
        return Ok(None);
    }

    let mut tail_effects = parse_effect_chain(&tail_tokens)?;
    if tail_effects.is_empty() {
        return Ok(None);
    }

    head_effects.append(&mut tail_effects);
    Ok(Some(head_effects))
}

fn parse_destroy_then_land_controller_graveyard_count_damage_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some(comma_then_idx) = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
    else {
        return Ok(None);
    };

    let head_tokens = trim_commas(&tokens[..comma_then_idx]);
    let tail_tokens = trim_commas(&tokens[comma_then_idx + 2..]);
    if head_tokens.is_empty() || tail_tokens.is_empty() {
        return Ok(None);
    }

    let tail_words = words(&tail_tokens);
    let suffix = [
        "damage",
        "to",
        "that",
        "lands",
        "controller",
        "equal",
        "to",
        "the",
        "number",
        "of",
        "land",
        "cards",
        "in",
        "that",
        "players",
        "graveyard",
    ];
    let Some(suffix_start) = tail_words
        .windows(suffix.len())
        .position(|window| window == suffix)
    else {
        return Ok(None);
    };
    if suffix_start == 0 || !matches!(tail_words[suffix_start - 1], "deal" | "deals") {
        return Ok(None);
    }
    if suffix_start + suffix.len() != tail_words.len() {
        return Ok(None);
    }

    let mut head_effects = parse_effect_chain(&head_tokens)?;
    if !head_effects
        .iter()
        .any(|effect| matches!(effect, EffectAst::Destroy { .. }))
    {
        return Ok(None);
    }

    let mut count_filter = ObjectFilter::default();
    count_filter.zone = Some(Zone::Graveyard);
    let tagged_ref = crate::target::ObjectRef::tagged(IT_TAG);
    count_filter.owner = Some(PlayerFilter::ControllerOf(tagged_ref.clone()));
    count_filter.card_types.push(CardType::Land);
    head_effects.push(EffectAst::DealDamage {
        amount: Value::Count(count_filter),
        target: TargetAst::Player(
            PlayerFilter::ControllerOf(tagged_ref),
            span_from_tokens(&tail_tokens),
        ),
    });
    Ok(Some(head_effects))
}

fn parse_sentence_destroy_all_attached_to_target(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let sentence_words = words(tokens);
    if sentence_words.first().copied() != Some("destroy") {
        return Ok(None);
    }
    if !tokens
        .get(1)
        .is_some_and(|token| token.is_word("all") || token.is_word("each"))
    {
        return Ok(None);
    }
    let Some(attached_idx) = tokens.iter().position(|token| token.is_word("attached")) else {
        return Ok(None);
    };
    if !tokens
        .get(attached_idx + 1)
        .is_some_and(|token| token.is_word("to"))
    {
        return Ok(None);
    }
    if attached_idx <= 2 || attached_idx + 2 >= tokens.len() {
        return Ok(None);
    }

    let mut filter_tokens = trim_commas(&tokens[2..attached_idx]).to_vec();
    while filter_tokens
        .last()
        .and_then(Token::as_word)
        .is_some_and(|word| matches!(word, "that" | "were" | "was" | "is" | "are"))
    {
        filter_tokens.pop();
    }
    let target_tokens = trim_commas(&tokens[attached_idx + 2..]);
    let target_words = words(&target_tokens);
    let has_timing_tail = target_words.iter().any(|word| {
        matches!(
            *word,
            "at" | "beginning" | "end" | "combat" | "turn" | "step" | "until"
        )
    });
    let supported_target = target_words.starts_with(&["target"])
        || target_words == ["it"]
        || target_words.starts_with(&["that", "creature"])
        || target_words.starts_with(&["that", "permanent"])
        || target_words.starts_with(&["that", "land"])
        || target_words.starts_with(&["that", "artifact"])
        || target_words.starts_with(&["that", "enchantment"]);
    if filter_tokens.is_empty() || target_tokens.is_empty() || !supported_target || has_timing_tail
    {
        return Ok(None);
    }

    let filter = parse_object_filter(&filter_tokens, false)?;
    let target = parse_target_phrase(&target_tokens)?;
    Ok(Some(vec![EffectAst::DestroyAllAttachedTo {
        filter,
        target,
    }]))
}

fn parse_sentence_destroy_then_land_controller_graveyard_count_damage(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_destroy_then_land_controller_graveyard_count_damage_sentence(tokens)
}

fn add_tagged_subtype_constraint_to_target(target: &mut TargetAst, tag: TagKey) -> bool {
    match target {
        TargetAst::Object(filter, _, _) => {
            filter.tagged_constraints.push(TaggedObjectConstraint {
                tag,
                relation: TaggedOpbjectRelation::SharesSubtypeWithTagged,
            });
            true
        }
        TargetAst::WithCount(inner, _) => add_tagged_subtype_constraint_to_target(inner, tag),
        _ => false,
    }
}

fn find_creature_type_choice_phrase(tokens: &[Token]) -> Option<(usize, usize)> {
    for idx in 0..tokens.len() {
        if tokens[idx].is_word("of")
            && tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("the"))
            && tokens
                .get(idx + 2)
                .is_some_and(|token| token.is_word("creature"))
            && tokens
                .get(idx + 3)
                .is_some_and(|token| token.is_word("type"))
            && tokens.get(idx + 4).is_some_and(|token| token.is_word("of"))
            && tokens
                .get(idx + 5)
                .is_some_and(|token| token.is_word("your"))
            && tokens
                .get(idx + 6)
                .is_some_and(|token| token.is_word("choice"))
        {
            return Some((idx, 7));
        }
        if tokens[idx].is_word("of")
            && tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("creature"))
            && tokens
                .get(idx + 2)
                .is_some_and(|token| token.is_word("type"))
            && tokens.get(idx + 3).is_some_and(|token| token.is_word("of"))
            && tokens
                .get(idx + 4)
                .is_some_and(|token| token.is_word("your"))
            && tokens
                .get(idx + 5)
                .is_some_and(|token| token.is_word("choice"))
        {
            return Some((idx, 6));
        }
    }
    None
}

fn find_color_choice_phrase(tokens: &[Token]) -> Option<(usize, usize)> {
    for idx in 0..tokens.len() {
        if tokens[idx].is_word("of")
            && tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("the"))
            && tokens
                .get(idx + 2)
                .is_some_and(|token| token.is_word("color"))
            && tokens.get(idx + 3).is_some_and(|token| token.is_word("of"))
            && (tokens
                .get(idx + 4)
                .is_some_and(|token| token.is_word("your"))
                || tokens
                    .get(idx + 4)
                    .is_some_and(|token| token.is_word("their")))
            && tokens
                .get(idx + 5)
                .is_some_and(|token| token.is_word("choice"))
        {
            return Some((idx, 6));
        }
        if tokens[idx].is_word("of")
            && tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("color"))
            && tokens.get(idx + 2).is_some_and(|token| token.is_word("of"))
            && (tokens
                .get(idx + 3)
                .is_some_and(|token| token.is_word("your"))
                || tokens
                    .get(idx + 3)
                    .is_some_and(|token| token.is_word("their")))
            && tokens
                .get(idx + 4)
                .is_some_and(|token| token.is_word("choice"))
        {
            return Some((idx, 5));
        }
    }
    None
}

fn parse_sentence_destroy_creature_type_of_choice(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["destroy", "all", "creatures"]) {
        return Ok(None);
    }
    if find_creature_type_choice_phrase(tokens).is_none() {
        return Ok(None);
    }

    let chosen_type_tag: TagKey = "chosen_creature_type_ref".into();
    let mut choose_filter = ObjectFilter::creature();
    choose_filter.controller = Some(PlayerFilter::Any);
    Ok(Some(vec![
        EffectAst::ChooseObjects {
            filter: choose_filter,
            count: ChoiceCount::exactly(1),
            player: PlayerAst::You,
            tag: chosen_type_tag.clone(),
        },
        EffectAst::DestroyAll {
            filter: ObjectFilter::creature().match_tagged(
                chosen_type_tag,
                TaggedOpbjectRelation::SharesSubtypeWithTagged,
            ),
        },
    ]))
}

fn parse_sentence_pump_creature_type_of_choice(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some(get_idx) = tokens
        .iter()
        .position(|token| token.is_word("get") || token.is_word("gets"))
    else {
        return Ok(None);
    };
    if get_idx == 0 {
        return Ok(None);
    }

    let subject_tokens = trim_commas(&tokens[..get_idx]);
    let Some((choice_idx, consumed)) = find_creature_type_choice_phrase(&subject_tokens) else {
        return Ok(None);
    };
    let trailing_subject = trim_commas(&subject_tokens[choice_idx + consumed..]);
    if !trailing_subject.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing creature-type choice subject clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    let trimmed_subject_tokens = trim_commas(&subject_tokens[..choice_idx]).to_vec();
    if trimmed_subject_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing creature subject before creature-type choice phrase (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let chosen_type_tag: TagKey = "chosen_creature_type_ref".into();
    let mut choose_filter = ObjectFilter::creature();
    choose_filter.controller = Some(PlayerFilter::Any);

    // Handle composed clauses like:
    // "Creatures of the creature type of your choice get +2/+2 and gain trample until end of turn."
    let mut gain_candidate_tokens = trimmed_subject_tokens.clone();
    gain_candidate_tokens.extend_from_slice(&tokens[get_idx..]);
    if let Some(mut gain_effects) = parse_gain_ability_sentence(&gain_candidate_tokens)? {
        let mut patched = false;
        for effect in &mut gain_effects {
            match effect {
                EffectAst::PumpAll { filter, .. }
                | EffectAst::GrantAbilitiesAll { filter, .. }
                | EffectAst::GrantAbilitiesChoiceAll { filter, .. } => {
                    if !filter.tagged_constraints.iter().any(|constraint| {
                        constraint.tag == chosen_type_tag
                            && constraint.relation == TaggedOpbjectRelation::SharesSubtypeWithTagged
                    }) {
                        filter.tagged_constraints.push(TaggedObjectConstraint {
                            tag: chosen_type_tag.clone(),
                            relation: TaggedOpbjectRelation::SharesSubtypeWithTagged,
                        });
                    }
                    patched = true;
                }
                _ => {}
            }
        }
        if patched {
            let mut effects = vec![EffectAst::ChooseObjects {
                filter: choose_filter,
                count: ChoiceCount::exactly(1),
                player: PlayerAst::You,
                tag: chosen_type_tag,
            }];
            effects.extend(gain_effects);
            return Ok(Some(effects));
        }
    }

    let mut filter_tokens = trimmed_subject_tokens;
    if filter_tokens
        .first()
        .is_some_and(|token| token.is_word("all"))
    {
        filter_tokens.remove(0);
    }
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing creature subject before creature-type choice phrase (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let mut filter = parse_object_filter(&filter_tokens, false)?;
    if !filter.card_types.contains(&CardType::Creature) {
        return Err(CardTextError::ParseError(format!(
            "creature-type choice pump subject must be creature-based (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let modifier = tokens
        .get(get_idx + 1)
        .and_then(Token::as_word)
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing power/toughness modifier in creature-type choice pump clause (clause: '{}')",
                words(tokens).join(" ")
            ))
        })?;
    let (base_power, base_toughness) = parse_pt_modifier_values(modifier).map_err(|_| {
        CardTextError::ParseError(format!(
            "invalid power/toughness modifier in creature-type choice pump clause (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;
    let (power, toughness, duration, condition) =
        parse_get_modifier_values_with_tail(&tokens[get_idx + 1..], base_power, base_toughness)?;
    if condition.is_some() {
        return Err(CardTextError::ParseError(format!(
            "unsupported conditional gets duration in creature-type choice pump clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    filter.tagged_constraints.push(TaggedObjectConstraint {
        tag: chosen_type_tag.clone(),
        relation: TaggedOpbjectRelation::SharesSubtypeWithTagged,
    });

    Ok(Some(vec![
        EffectAst::ChooseObjects {
            filter: choose_filter,
            count: ChoiceCount::exactly(1),
            player: PlayerAst::You,
            tag: chosen_type_tag,
        },
        EffectAst::PumpAll {
            filter,
            power,
            toughness,
            duration,
        },
    ]))
}

fn parse_sentence_return_targets_of_creature_type_of_choice(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens.first().is_some_and(|token| token.is_word("return")) {
        return Ok(None);
    }
    let Some(to_idx) = tokens.iter().rposition(|token| token.is_word("to")) else {
        return Ok(None);
    };
    if to_idx <= 1 {
        return Ok(None);
    }

    let destination_words = words(&tokens[to_idx + 1..]);
    if !destination_words.contains(&"hand") && !destination_words.contains(&"hands") {
        return Ok(None);
    }

    let target_tokens = &tokens[1..to_idx];
    let Some((choice_idx, consumed)) = find_creature_type_choice_phrase(target_tokens) else {
        return Ok(None);
    };

    let trimmed_target = trim_commas(&target_tokens[..choice_idx]);
    if trimmed_target.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing return target before creature-type choice phrase (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    let trailing = trim_commas(&target_tokens[choice_idx + consumed..]);
    if !trailing.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing return target clause after creature-type choice phrase (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let mut target = parse_target_phrase(&trimmed_target)?;
    let chosen_type_tag: TagKey = "chosen_creature_type_ref".into();
    if !add_tagged_subtype_constraint_to_target(&mut target, chosen_type_tag.clone()) {
        return Err(CardTextError::ParseError(format!(
            "creature-type choice return target must be object-based (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let mut choose_filter = ObjectFilter::creature();
    choose_filter.controller = Some(PlayerFilter::Any);
    Ok(Some(vec![
        EffectAst::ChooseObjects {
            filter: choose_filter,
            count: ChoiceCount::exactly(1),
            player: PlayerAst::You,
            tag: chosen_type_tag,
        },
        EffectAst::ReturnToHand {
            target,
            random: false,
        },
    ]))
}

fn return_segment_mentions_zone(tokens: &[Token]) -> bool {
    let segment_words = words(tokens);
    segment_words.contains(&"graveyard")
        || segment_words.contains(&"graveyards")
        || segment_words.contains(&"battlefield")
        || segment_words.contains(&"hand")
        || segment_words.contains(&"hands")
        || segment_words.contains(&"library")
        || segment_words.contains(&"libraries")
        || segment_words.contains(&"exile")
}

fn parse_sentence_return_multiple_targets(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens.first().is_some_and(|token| token.is_word("return")) {
        return Ok(None);
    }
    let Some(to_idx) = tokens.iter().rposition(|token| token.is_word("to")) else {
        return Ok(None);
    };
    if to_idx <= 1 {
        return Ok(None);
    }

    let destination_words = words(&tokens[to_idx + 1..]);
    let is_hand = destination_words.contains(&"hand") || destination_words.contains(&"hands");
    let is_battlefield = destination_words.contains(&"battlefield");
    let tapped = destination_words.contains(&"tapped");
    if !is_hand && !is_battlefield {
        return Ok(None);
    }

    let target_tokens = trim_commas(&tokens[1..to_idx]);
    let has_multi_separator = target_tokens.iter().any(|token| {
        token.is_word("and")
            || matches!(token, Token::Comma(_))
            || token.is_word("or")
            || token.is_word("and/or")
    });
    if !has_multi_separator {
        return Ok(None);
    }

    let mut segments: Vec<Vec<Token>> = Vec::new();
    for and_segment in split_on_and(&target_tokens) {
        for comma_segment in split_on_comma(&and_segment) {
            let trimmed = trim_commas(&comma_segment);
            if !trimmed.is_empty() {
                let trimmed_words = words(&trimmed);
                let starts_new_target = trimmed_words.first().is_some_and(|word| {
                    matches!(
                        *word,
                        "target"
                            | "up"
                            | "another"
                            | "other"
                            | "this"
                            | "that"
                            | "it"
                            | "them"
                            | "all"
                            | "each"
                    )
                });
                let mentions_target = trimmed_words.contains(&"target");
                let starts_like_zone_suffix = trimmed_words
                    .first()
                    .is_some_and(|word| matches!(*word, "from" | "to" | "in" | "on" | "under"));
                if !segments.is_empty()
                    && !starts_new_target
                    && !mentions_target
                    && !starts_like_zone_suffix
                {
                    let last = segments.last_mut().expect("segments is non-empty");
                    last.push(Token::Comma(TextSpan::synthetic()));
                    last.extend(trimmed.to_vec());
                } else {
                    segments.push(trimmed.to_vec());
                }
            }
        }
    }
    if segments.len() < 2 {
        return Ok(None);
    }

    let shared_quantifier = segments
        .first()
        .and_then(|segment| segment.first())
        .and_then(Token::as_word)
        .filter(|word| matches!(*word, "all" | "each"))
        .map(str::to_string);

    let shared_suffix = segments
        .last()
        .and_then(|segment| {
            segment
                .iter()
                .position(|token| token.is_word("from"))
                .map(|idx| segment[idx..].to_vec())
        })
        .unwrap_or_default();

    let mut effects = Vec::new();
    for mut segment in segments {
        if !return_segment_mentions_zone(&segment) && !shared_suffix.is_empty() {
            segment.extend(shared_suffix.clone());
        }
        if let Some(quantifier) = shared_quantifier.as_deref() {
            let segment_words = words(&segment);
            let has_explicit_quantifier =
                matches!(segment_words.first().copied(), Some("all" | "each"));
            let starts_like_target_reference = matches!(
                segment_words.first().copied(),
                Some("target" | "up" | "this" | "that" | "it" | "them" | "another")
            );
            if !has_explicit_quantifier
                && !starts_like_target_reference
                && !segment_words.contains(&"target")
            {
                segment.insert(
                    0,
                    Token::Word(quantifier.to_string(), TextSpan::synthetic()),
                );
            }
        }
        let segment_words = words(&segment);
        if matches!(segment_words.first().copied(), Some("all" | "each")) {
            if segment.len() < 2 {
                return Err(CardTextError::ParseError(format!(
                    "missing return-all filter (clause: '{}')",
                    words(tokens).join(" ")
                )));
            }
            let filter = parse_object_filter(&segment[1..], false)?;
            if is_battlefield {
                effects.push(EffectAst::ReturnAllToBattlefield { filter, tapped });
            } else {
                effects.push(EffectAst::ReturnAllToHand { filter });
            }
        } else {
            let target = parse_target_phrase(&segment)?;
            if is_battlefield {
                effects.push(EffectAst::ReturnToBattlefield {
                    target,
                    tapped,
                    controller: ReturnControllerAst::Preserve,
                });
            } else {
                effects.push(EffectAst::ReturnToHand {
                    target,
                    random: false,
                });
            }
        }
    }

    Ok(Some(effects))
}

fn parse_sentence_for_each_of_target_objects(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if !(clause_words.starts_with(&["for", "each"]) || clause_words.first() == Some(&"each")) {
        return Ok(None);
    }

    let Some(comma_idx) = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)))
    else {
        return Ok(None);
    };
    if comma_idx == 0 || comma_idx + 1 >= tokens.len() {
        return Ok(None);
    }

    let subject_tokens = trim_commas(&tokens[..comma_idx]);
    let Some((mut filter, count)) = parse_for_each_targeted_object_subject(&subject_tokens)? else {
        return Ok(None);
    };
    if filter.zone == Some(Zone::Battlefield)
        && filter.controller.is_none()
        && filter.tagged_constraints.is_empty()
    {
        // Keep this unrestricted to avoid implicit "you control" defaulting in ChooseObjects
        // compilation for plain "target permanent(s)" clauses.
        filter.controller = Some(PlayerFilter::Any);
    }

    let effect_tokens = trim_commas(&tokens[comma_idx + 1..]);
    if effect_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing effect after for-each target subject (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let mut per_target_effects = parse_effect_chain(&effect_tokens)?;
    for effect in &mut per_target_effects {
        bind_implicit_player_context(effect, PlayerAst::You);
    }
    if per_target_effects.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "for-each target follow-up produced no effects (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(Some(vec![
        EffectAst::ChooseObjects {
            filter,
            count,
            player: PlayerAst::Implicit,
            tag: TagKey::from(IT_TAG),
        },
        EffectAst::ForEachTagged {
            tag: TagKey::from(IT_TAG),
            effects: per_target_effects,
        },
    ]))
}

fn parse_distribute_counters_sentence(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.first().copied() != Some("distribute") {
        return Ok(None);
    }

    let (count, used) = parse_number(&tokens[1..]).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing distributed counter amount (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;
    let rest = &tokens[1 + used..];
    let counter_type = parse_counter_type_from_tokens(rest).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "unsupported distributed counter type (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;
    let among_idx = rest
        .iter()
        .position(|token| token.is_word("among"))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing distributed target clause after 'among' (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    let target_tokens = trim_commas(&rest[among_idx + 1..]);
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing distributed counter targets (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let (target_count, used_count) = parse_counter_target_count_prefix(&target_tokens)?
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing distributed target count prefix (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    let target_phrase = &target_tokens[used_count..];
    if target_phrase.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing distributed target phrase (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let target = parse_target_phrase(target_phrase)?;

    Ok(Some(EffectAst::PutCounters {
        counter_type,
        count: Value::Fixed(count as i32),
        target,
        target_count: Some(target_count),
        distributed: true,
    }))
}

fn parse_sentence_distribute_counters(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let mut head_tokens = tokens.to_vec();
    let mut tail_tokens: Vec<Token> = Vec::new();

    if let Some(comma_then_idx) = tokens
        .windows(2)
        .position(|window| matches!(window[0], Token::Comma(_)) && window[1].is_word("then"))
    {
        head_tokens = tokens[..comma_then_idx].to_vec();
        tail_tokens = trim_commas(&tokens[comma_then_idx + 2..]);
    } else if let Some(then_idx) = tokens.iter().position(|token| token.is_word("then")) {
        head_tokens = tokens[..then_idx].to_vec();
        tail_tokens = trim_commas(&tokens[then_idx + 1..]);
    }

    let Some(primary) = parse_distribute_counters_sentence(&head_tokens)? else {
        return Ok(None);
    };

    let mut effects = vec![primary];
    if !tail_tokens.is_empty() {
        effects.extend(parse_effect_chain(&tail_tokens)?);
    }

    Ok(Some(effects))
}

fn parse_sentence_exile_that_token_at_end_of_combat(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if is_exile_that_token_at_end_of_combat(tokens) {
        return Ok(Some(vec![EffectAst::ExileThatTokenAtEndOfCombat]));
    }
    Ok(None)
}

fn parse_sentence_sacrifice_that_token_at_end_of_combat(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if is_sacrifice_that_token_at_end_of_combat(tokens) {
        return Ok(Some(vec![EffectAst::SacrificeThatTokenAtEndOfCombat]));
    }
    Ok(None)
}

fn parse_sentence_take_extra_turn(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_take_extra_turn_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_earthbend(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some(earthbend) = parse_earthbend_sentence(tokens)? else {
        return Ok(None);
    };

    // Support chained text like "earthbend 8, then untap that land."
    let Some((_, used)) = parse_number(&tokens[1..]) else {
        return Ok(Some(vec![earthbend]));
    };
    let mut tail = trim_commas(&tokens[1 + used..]).to_vec();
    while tail.first().is_some_and(|token| token.is_word("then")) {
        tail.remove(0);
    }
    if tail.is_empty() {
        return Ok(Some(vec![earthbend]));
    }

    let mut effects = vec![earthbend];
    effects.extend(parse_effect_chain(&tail)?);
    Ok(Some(effects))
}

fn parse_sentence_enchant(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_enchant_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_cant_effect(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_cant_effect_sentence(tokens)
}

fn parse_sentence_prevent_damage(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_prevent_damage_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_gain_ability_to_source(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_gain_ability_to_source_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_gain_ability(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_gain_ability_sentence(tokens)
}

fn parse_sentence_you_and_each_opponent_voted_with_you(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_you_and_each_opponent_voted_with_you_sentence(tokens)
}

fn parse_sentence_gain_life_equal_to_power(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_gain_life_equal_to_power_sentence(tokens)
}

fn parse_sentence_gain_x_plus_life(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_gain_x_plus_life_sentence(tokens)
}

fn parse_sentence_for_each_exiled_this_way(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_for_each_exiled_this_way_sentence(tokens)
}

fn parse_sentence_each_player_put_permanent_cards_exiled_with_source(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_each_player_put_permanent_cards_exiled_with_source_sentence(tokens)
}

fn parse_sentence_for_each_destroyed_this_way(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_for_each_destroyed_this_way_sentence(tokens)
}

fn parse_sentence_exile_then_return_same_object(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_exile_then_return_same_object_sentence(tokens)
}

fn parse_sentence_search_library(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_search_library_sentence(tokens)
}

fn parse_sentence_shuffle_graveyard_into_library(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_shuffle_graveyard_into_library_sentence(tokens)
}

fn parse_sentence_exile_hand_and_graveyard_bundle(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_exile_hand_and_graveyard_bundle_sentence(tokens)
}

fn parse_sentence_target_player_exiles_creature_and_graveyard(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_target_player_exiles_creature_and_graveyard_sentence(tokens)
}

fn parse_sentence_play_from_graveyard(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_play_from_graveyard_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_look_at_hand(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_look_at_hand_sentence(tokens)
}

fn parse_sentence_look_at_top_then_exile_one(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_look_at_top_then_exile_one_sentence(tokens)
}

fn parse_sentence_gain_life_equal_to_age(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_gain_life_equal_to_age_sentence(tokens)
}

fn parse_sentence_for_each_opponent_doesnt(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_for_each_opponent_doesnt(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_for_each_player_doesnt(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_for_each_player_doesnt(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_each_opponent_loses_x_and_you_gain_x(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let sentence_words = words(tokens);
    if !(sentence_words.starts_with(&["each", "opponent"])
        || sentence_words.starts_with(&["each", "opponents"]))
    {
        return Ok(None);
    }

    let has_lose_x = sentence_words.windows(3).any(|window| {
        (window[0] == "lose" || window[0] == "loses") && window[1] == "x" && window[2] == "life"
    });
    let has_gain_x = sentence_words
        .windows(4)
        .any(|window| window == ["you", "gain", "x", "life"]);
    let Some(where_idx) = sentence_words
        .windows(3)
        .position(|window| window == ["where", "x", "is"])
    else {
        return Ok(None);
    };
    if !has_lose_x || !has_gain_x {
        return Ok(None);
    }

    let where_token_idx = token_index_for_word_index(tokens, where_idx).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing where-x clause in opponent life-drain clause (clause: '{}')",
            sentence_words.join(" ")
        ))
    })?;
    let where_tokens = &tokens[where_token_idx..];
    let where_value = parse_where_x_value_clause(where_tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "unsupported where-x value in opponent life-drain clause (clause: '{}')",
            sentence_words.join(" ")
        ))
    })?;

    Ok(Some(vec![
        EffectAst::ForEachOpponent {
            effects: vec![EffectAst::LoseLife {
                amount: where_value.clone(),
                player: PlayerAst::Implicit,
            }],
        },
        EffectAst::GainLife {
            amount: where_value,
            player: PlayerAst::You,
        },
    ]))
}

fn parse_sentence_vote_start(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_vote_start_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_for_each_vote_clause(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_for_each_vote_clause(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_vote_extra(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_vote_extra_sentence(tokens).map(|effect| vec![effect]))
}

fn parse_sentence_after_turn(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    Ok(parse_after_turn_sentence(tokens)?.map(|effect| vec![effect]))
}

fn parse_sentence_same_name_target_fanout(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_same_name_target_fanout_sentence(tokens)
}

fn parse_sentence_shared_color_target_fanout(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_shared_color_target_fanout_sentence(tokens)
}

fn parse_sentence_same_name_gets_fanout(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_same_name_gets_fanout_sentence(tokens)
}

fn parse_sentence_delayed_until_next_end_step(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_delayed_until_next_end_step_sentence(tokens)
}

fn parse_sentence_destroy_or_exile_all_split(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_destroy_or_exile_all_split_sentence(tokens)
}

fn parse_sentence_exile_up_to_one_each_target_type(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    parse_exile_up_to_one_each_target_type_sentence(tokens)
}

fn parse_sentence_exile_multi_target(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.first() != Some(&"exile") || clause_words.contains(&"unless") {
        return Ok(None);
    }

    let mut split_idx = None;
    for (idx, token) in tokens.iter().enumerate() {
        if !token.is_word("and") || idx == 0 || idx + 1 >= tokens.len() {
            continue;
        }
        let tail_words = words(&tokens[idx + 1..]);
        let starts_second_target = tail_words.first() == Some(&"target")
            || (tail_words.starts_with(&["up", "to"]) && tail_words.contains(&"target"));
        if starts_second_target {
            split_idx = Some(idx);
            break;
        }
    }

    let Some(and_idx) = split_idx else {
        return Ok(None);
    };

    let first_tokens = trim_commas(&tokens[1..and_idx]);
    let second_tokens = trim_commas(&tokens[and_idx + 1..]);
    if first_tokens.is_empty() || second_tokens.is_empty() {
        return Ok(None);
    }

    let first_words = words(&first_tokens);
    let second_words = words(&second_tokens);
    let first_is_explicit_target = first_words.first() == Some(&"target")
        || (first_words.starts_with(&["up", "to"]) && first_words.contains(&"target"));
    let second_is_explicit_target = second_words.first() == Some(&"target")
        || (second_words.starts_with(&["up", "to"]) && second_words.contains(&"target"));

    let mut first_target = match parse_target_phrase(&first_tokens) {
        Ok(target) => target,
        Err(_) if !first_is_explicit_target && is_likely_named_or_source_reference_words(&first_words) => {
            TargetAst::Source(span_from_tokens(&first_tokens))
        }
        Err(err) => return Err(err),
    };
    let mut second_target = parse_target_phrase(&second_tokens)?;

    if first_is_explicit_target
        && second_is_explicit_target
        && let (Some((mut first_filter, first_count)), Some((mut second_filter, second_count))) = (
            object_target_with_count(&first_target),
            object_target_with_count(&second_target),
        )
        && first_filter.zone == Some(Zone::Graveyard)
        && second_filter.zone == Some(Zone::Graveyard)
    {
        if first_filter.controller.is_none() {
            first_filter.controller = Some(PlayerFilter::Any);
        }
        if second_filter.controller.is_none() {
            second_filter.controller = Some(PlayerFilter::Any);
        }
        let tag = TagKey::from("exiled_0");
        return Ok(Some(vec![
            EffectAst::ChooseObjects {
                filter: first_filter,
                count: first_count,
                player: PlayerAst::You,
                tag: tag.clone(),
            },
            EffectAst::ChooseObjects {
                filter: second_filter,
                count: second_count,
                player: PlayerAst::You,
                tag: tag.clone(),
            },
            EffectAst::Exile {
                target: TargetAst::Tagged(tag, None),
                face_down: false,
            },
        ]));
    }

    apply_exile_subject_hand_owner_context(&mut first_target, None);
    apply_exile_subject_hand_owner_context(&mut second_target, None);
    Ok(Some(vec![
        EffectAst::Exile {
            target: first_target,
            face_down: false,
        },
        EffectAst::Exile {
            target: second_target,
            face_down: false,
        },
    ]))
}

fn split_destroy_target_segments(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut raw_segments: Vec<Vec<Token>> = Vec::new();
    for and_segment in split_on_and(tokens) {
        for comma_segment in split_on_comma(&and_segment) {
            let trimmed = trim_commas(&comma_segment);
            if !trimmed.is_empty() {
                raw_segments.push(trimmed.to_vec());
            }
        }
    }

    let mut segments = Vec::new();
    for segment in raw_segments {
        let split_starts = segment
            .iter()
            .enumerate()
            .filter_map(|(idx, token)| {
                if idx >= 3
                    && token.is_word("target")
                    && segment[idx - 3].is_word("up")
                    && segment[idx - 2].is_word("to")
                    && segment[idx - 1].is_word("one")
                {
                    Some(idx - 3)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if split_starts.len() <= 1 {
            segments.push(segment);
            continue;
        }

        for (idx, start) in split_starts.iter().enumerate() {
            let end = split_starts.get(idx + 1).copied().unwrap_or(segment.len());
            let trimmed = trim_commas(&segment[*start..end]);
            if !trimmed.is_empty() {
                segments.push(trimmed.to_vec());
            }
        }
    }

    segments
}

fn parse_sentence_destroy_multi_target(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.first() != Some(&"destroy") {
        return Ok(None);
    }
    if clause_words.get(1).is_some_and(|word| matches!(*word, "all" | "each")) {
        return Ok(None);
    }
    if clause_words.contains(&"unless") || clause_words.contains(&"if") {
        return Ok(None);
    }

    let target_tokens = trim_commas(&tokens[1..]);
    if target_tokens.is_empty() {
        return Ok(None);
    }

    let has_separator = target_tokens.iter().any(|token| {
        token.is_word("and") || matches!(token, Token::Comma(_))
    });
    let has_repeated_up_to_one_targets = target_tokens
        .windows(4)
        .filter(|window| {
            window[0].is_word("up")
                && window[1].is_word("to")
                && window[2].is_word("one")
                && window[3].is_word("target")
        })
        .count()
        >= 2;
    if !has_separator && !has_repeated_up_to_one_targets {
        return Ok(None);
    }

    let segments = split_destroy_target_segments(&target_tokens);
    if segments.len() < 2 {
        return Ok(None);
    }

    let mut effects = Vec::new();
    for segment in segments {
        let segment_words = words(&segment);
        if segment_words.iter().any(|word| {
            matches!(
                *word,
                "then" | "if" | "unless" | "where" | "when" | "whenever"
            )
        }) {
            return Ok(None);
        }
        let is_explicit_target = segment_words.first() == Some(&"target")
            || (segment_words.starts_with(&["up", "to"]) && segment_words.contains(&"target"));
        if !is_explicit_target && !is_likely_named_or_source_reference_words(&segment_words) {
            return Ok(None);
        }
        let target = match parse_target_phrase(&segment) {
            Ok(target) => target,
            Err(_) if !is_explicit_target && is_likely_named_or_source_reference_words(&segment_words) => {
                TargetAst::Source(span_from_tokens(&segment))
            }
            Err(err) => return Err(err),
        };
        effects.push(EffectAst::Destroy { target });
    }

    if effects.len() < 2 {
        return Ok(None);
    }
    Ok(Some(effects))
}

fn parse_sentence_reveal_selected_cards_in_your_hand(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.first() != Some(&"reveal") {
        return Ok(None);
    }
    if clause_words
        .iter()
        .any(|word| matches!(*word, "then" | "if" | "unless" | "where" | "when" | "whenever"))
    {
        return Ok(None);
    }

    let Some(in_idx) = tokens.iter().position(|token| token.is_word("in")) else {
        return Ok(None);
    };
    if in_idx == 0 || in_idx + 2 >= tokens.len() {
        return Ok(None);
    }
    if !tokens.get(in_idx + 1).is_some_and(|token| token.is_word("your"))
        || !tokens
            .get(in_idx + 2)
            .is_some_and(|token| token.is_word("hand") || token.is_word("hands"))
    {
        return Ok(None);
    }

    let mut descriptor_tokens = trim_commas(&tokens[1..in_idx]);
    if descriptor_tokens.is_empty() {
        return Ok(None);
    }

    let mut count = ChoiceCount::exactly(1);
    let descriptor_words = words(&descriptor_tokens);
    if descriptor_words.starts_with(&["any", "number", "of"]) {
        count = ChoiceCount::any_number();
        descriptor_tokens = trim_commas(&descriptor_tokens[3..]);
    } else if descriptor_words.starts_with(&["up", "to"]) {
        if let Some((value, used)) = parse_number(&descriptor_tokens[2..]) {
            count = ChoiceCount::up_to(value as usize);
            descriptor_tokens = trim_commas(&descriptor_tokens[2 + used..]);
            if descriptor_tokens
                .first()
                .is_some_and(|token| token.is_word("of"))
            {
                descriptor_tokens = trim_commas(&descriptor_tokens[1..]);
            }
        } else {
            return Ok(None);
        }
    } else if descriptor_words.first() == Some(&"x") {
        count = ChoiceCount::any_number();
        descriptor_tokens = trim_commas(&descriptor_tokens[1..]);
    } else if descriptor_words
        .first()
        .is_some_and(|word| matches!(*word, "a" | "an" | "one"))
    {
        descriptor_tokens = trim_commas(&descriptor_tokens[1..]);
    } else if descriptor_words
        .first()
        .is_some_and(|word| matches!(*word, "all" | "each"))
    {
        return Ok(None);
    }

    if descriptor_tokens.is_empty() {
        return Ok(None);
    }

    let mut filter = match parse_object_filter(&descriptor_tokens, false) {
        Ok(filter) => filter,
        Err(_) => {
            let descriptor_words = words(&descriptor_tokens);
            let mut filter = ObjectFilter::default();
            let mut idx = 0usize;
            if let Some(color) = descriptor_words.get(idx).and_then(|word| parse_color(word)) {
                filter.colors = Some(color.into());
                idx += 1;
            }
            if !descriptor_words
                .get(idx)
                .is_some_and(|word| matches!(*word, "card" | "cards"))
            {
                return Err(CardTextError::ParseError(format!(
                    "unsupported reveal-hand clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            filter
        }
    };
    filter.zone = Some(Zone::Hand);
    filter.owner = Some(PlayerFilter::You);

    let tag = TagKey::from("revealed_0");
    Ok(Some(vec![
        EffectAst::ChooseObjects {
            filter,
            count,
            player: PlayerAst::You,
            tag: tag.clone(),
        },
        EffectAst::RevealTagged { tag },
    ]))
}

fn object_target_with_count(target: &TargetAst) -> Option<(ObjectFilter, ChoiceCount)> {
    match target {
        TargetAst::Object(filter, _, _) => Some((filter.clone(), ChoiceCount::exactly(1))),
        TargetAst::WithCount(inner, count) => match inner.as_ref() {
            TargetAst::Object(filter, _, _) => Some((filter.clone(), count.clone())),
            _ => None,
        },
        _ => None,
    }
}

fn is_likely_named_or_source_reference_words(words: &[&str]) -> bool {
    if words.is_empty() {
        return false;
    }
    if is_source_reference_words(words) {
        return true;
    }
    if words.iter().any(|word| {
        matches!(
            *word,
            "then"
                | "if"
                | "unless"
                | "where"
                | "when"
                | "whenever"
                | "for"
                | "each"
                | "search"
                | "destroy"
                | "exile"
                | "draw"
                | "gain"
                | "lose"
                | "counter"
                | "put"
                | "return"
                | "create"
                | "sacrifice"
                | "deal"
                | "populate"
        )
    }) {
        return false;
    }
    !words.iter().any(|word| {
        matches!(
            *word,
            "a" | "an"
                | "the"
                | "this"
                | "that"
                | "those"
                | "it"
                | "them"
                | "target"
                | "all"
                | "any"
                | "each"
                | "another"
                | "other"
                | "up"
                | "to"
                | "card"
                | "cards"
                | "creature"
                | "creatures"
                | "permanent"
                | "permanents"
                | "artifact"
                | "artifacts"
                | "enchantment"
                | "enchantments"
                | "land"
                | "lands"
                | "planeswalker"
                | "planeswalkers"
                | "spell"
                | "spells"
        )
    })
}

fn parse_sentence_damage_unless_controller_has_source_deal_damage(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some(unless_idx) = tokens.iter().position(|token| token.is_word("unless")) else {
        return Ok(None);
    };
    if unless_idx == 0 || unless_idx + 1 >= tokens.len() {
        return Ok(None);
    }

    let before_tokens = trim_commas(&tokens[..unless_idx]);
    if before_tokens.is_empty() {
        return Ok(None);
    }
    let effects = parse_effect_chain(&before_tokens)?;
    if effects.len() != 1 {
        return Ok(None);
    }
    let Some(main_damage) = effects.first() else {
        return Ok(None);
    };
    let EffectAst::DealDamage {
        amount: main_amount,
        target: main_target,
    } = main_damage
    else {
        return Ok(None);
    };
    if !matches!(
        main_target,
        TargetAst::Object(_, _, _) | TargetAst::WithCount(_, _)
    ) {
        return Ok(None);
    }

    let after_unless = trim_commas(&tokens[unless_idx + 1..]);
    let after_words = words(&after_unless);
    let has_controller_clause = after_words.starts_with(&["that"])
        && after_words
            .iter()
            .any(|word| *word == "controller" || *word == "controllers");
    if !has_controller_clause {
        return Ok(None);
    }
    let Some(has_idx) = after_unless
        .iter()
        .position(|token| token.is_word("has") || token.is_word("have"))
    else {
        return Ok(None);
    };
    if has_idx + 1 >= after_unless.len() {
        return Ok(None);
    }

    let alt_tokens = &after_unless[has_idx + 1..];
    let Some(deal_idx) = alt_tokens
        .iter()
        .position(|token| token.is_word("deal") || token.is_word("deals"))
    else {
        return Ok(None);
    };
    let deal_tail = &alt_tokens[deal_idx..];
    let Some((alt_amount, used)) = parse_value(&deal_tail[1..]) else {
        return Ok(None);
    };
    if !deal_tail
        .get(1 + used)
        .is_some_and(|token| token.is_word("damage"))
    {
        return Ok(None);
    }

    let mut alt_target_tokens = &deal_tail[2 + used..];
    if alt_target_tokens
        .first()
        .is_some_and(|token| token.is_word("to"))
    {
        alt_target_tokens = &alt_target_tokens[1..];
    }
    let alt_target_words = words(alt_target_tokens);
    if !matches!(alt_target_words.as_slice(), ["them"] | ["that", "player"]) {
        return Ok(None);
    }

    let alternative = EffectAst::DealDamage {
        amount: alt_amount,
        target: TargetAst::Player(
            PlayerFilter::ControllerOf(crate::filter::ObjectRef::Target),
            None,
        ),
    };
    let unless = EffectAst::UnlessAction {
        effects: vec![EffectAst::DealDamage {
            amount: main_amount.clone(),
            target: main_target.clone(),
        }],
        alternative: vec![alternative],
        player: PlayerAst::ItsController,
    };
    Ok(Some(vec![unless]))
}

fn parse_sentence_unless_pays(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    // Find "unless" in the token stream
    let unless_idx = match tokens.iter().position(|t| t.is_word("unless")) {
        Some(idx) => idx,
        None => return Ok(None),
    };

    // Leading form: "Unless you pay ..., <effects>."
    // Rewrite by parsing the effect tail after the first comma and wrapping it
    // in the parsed unless-payment clause.
    if unless_idx == 0 {
        let comma_idx = match tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)))
        {
            Some(idx) => idx,
            None => return Ok(None),
        };
        if comma_idx + 1 >= tokens.len() {
            return Ok(None);
        }

        let effects = parse_effect_chain(&tokens[comma_idx + 1..])?;
        if effects.is_empty() {
            return Ok(None);
        }

        let unless_clause = &tokens[..comma_idx];
        if let Some(unless_effect) = try_build_unless(effects, unless_clause, 0)? {
            return Ok(Some(vec![unless_effect]));
        }
        return Ok(None);
    }

    // Need at least something before "unless" and something after.
    let before_words: Vec<&str> = tokens[..unless_idx]
        .iter()
        .filter_map(Token::as_word)
        .collect();

    // Skip "counter ... unless" - already handled by parse_counter via CounterUnlessPays
    if before_words.first() == Some(&"counter") {
        return Ok(None);
    }
    // Ignore "unless ... pays" that appears inside quoted token rules text.
    // Example: create token with "{1}, Sacrifice this token: Counter ... unless ...".
    if before_words.first() == Some(&"create")
        && before_words.contains(&"token")
        && before_words.contains(&"sacrifice")
        && before_words.contains(&"counter")
    {
        return Ok(None);
    }

    // Handle "each opponent/player ... unless" by wrapping in ForEachOpponent/ForEachPlayer.
    // Structure: ForEachOpponent { [UnlessPays/UnlessAction { per-player effects }] }
    let each_prefix = if before_words.starts_with(&["each", "opponent"])
        || before_words.starts_with(&["each", "opponents"])
    {
        Some("opponent")
    } else if before_words.starts_with(&["each", "player"]) {
        Some("player")
    } else {
        None
    };
    if let Some(prefix_kind) = each_prefix {
        // Tokens between "each opponent/player" and "unless" form the per-player effect
        let inner_token_start = tokens
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.as_word().map(|_| i))
            .nth(2) // skip "each" and "opponent"/"player"
            .unwrap_or(2);
        let inner_tokens = &tokens[inner_token_start..unless_idx];
        if let Ok(inner_effects) = parse_effect_chain(inner_tokens) {
            if !inner_effects.is_empty() {
                if let Some(unless_effect) = try_build_unless(inner_effects, tokens, unless_idx)? {
                    let wrapper = match prefix_kind {
                        "opponent" => EffectAst::ForEachOpponent {
                            effects: vec![unless_effect],
                        },
                        _ => EffectAst::ForEachPlayer {
                            effects: vec![unless_effect],
                        },
                    };
                    return Ok(Some(vec![wrapper]));
                }
            }
        }
        return Ok(None);
    }

    // Normal path: parse effects before "unless", then build unless wrapper
    let effect_tokens = &tokens[..unless_idx];
    let effects = parse_effect_chain(&effect_tokens)?;
    if effects.is_empty() {
        return Ok(None);
    }

    if let Some(unless_effect) = try_build_unless(effects, tokens, unless_idx)? {
        return Ok(Some(vec![unless_effect]));
    }

    Ok(None)
}

/// Try to build an UnlessPays or UnlessAction AST from the tokens after "unless".
/// Returns the unless wrapper containing the given `effects` as the main effects.
fn try_build_unless(
    effects: Vec<EffectAst>,
    tokens: &[Token],
    unless_idx: usize,
) -> Result<Option<EffectAst>, CardTextError> {
    let after_unless = &tokens[unless_idx + 1..];
    let after_words: Vec<&str> = after_unless.iter().filter_map(Token::as_word).collect();

    // Determine the player from the "unless" clause
    let (player, action_token_start) = if after_words.starts_with(&["you"]) {
        (PlayerAst::You, 1)
    } else if after_words.starts_with(&["target", "opponent"]) {
        (PlayerAst::TargetOpponent, 2)
    } else if after_words.starts_with(&["target", "player"]) {
        (PlayerAst::Target, 2)
    } else if after_words.starts_with(&["any", "player"]) {
        (PlayerAst::Any, 2)
    } else if after_words.len() >= 6
        && after_words[0] == "that"
        && matches!(
            after_words[1],
            "creature" | "creatures" | "permanent" | "permanents" | "source" | "sources"
        )
        && matches!(after_words[2], "controller" | "controllers")
        && after_words[3] == "or"
        && after_words[4] == "that"
        && after_words[5] == "player"
    {
        (PlayerAst::ItsController, 6)
    } else if after_words.len() >= 3
        && after_words[0] == "that"
        && matches!(
            after_words[1],
            "creature" | "creatures" | "permanent" | "permanents" | "source" | "sources"
        )
        && matches!(after_words[2], "controller" | "controllers")
    {
        (PlayerAst::ItsController, 3)
    } else if after_words.starts_with(&["they"]) {
        (PlayerAst::That, 1)
    } else if after_words.starts_with(&["defending", "player"]) {
        (PlayerAst::Defending, 2)
    } else if after_words.starts_with(&["that", "player"]) {
        (PlayerAst::That, 2)
    } else if after_words.starts_with(&["its", "controller"]) {
        (PlayerAst::ItsController, 2)
    } else if after_words.starts_with(&["their", "controller"]) {
        (PlayerAst::ItsController, 2)
    } else if after_words.starts_with(&["its", "owner"]) {
        (PlayerAst::ItsOwner, 2)
    } else if after_words.starts_with(&["their", "owner"]) {
        (PlayerAst::ItsOwner, 2)
    } else {
        return Ok(None);
    };

    // Find the token position corresponding to action_token_start words in
    let mut action_token_idx = 0;
    let mut wc = 0;
    for (i, token) in after_unless.iter().enumerate() {
        if token.as_word().is_some() {
            wc += 1;
            if wc == action_token_start {
                action_token_idx = i + 1;
                break;
            }
        }
    }

    let action_tokens = &after_unless[action_token_idx..];
    let action_words: Vec<&str> = action_tokens.iter().filter_map(Token::as_word).collect();

    // "unless [player] pays N life" should compile as an unless-action branch
    // where the deciding player loses life.
    if action_words.first() == Some(&"pay") || action_words.first() == Some(&"pays") {
        let life_tokens = &action_tokens[1..];
        if let Some((amount, used)) = parse_value(life_tokens)
            && life_tokens
                .get(used)
                .is_some_and(|token| token.is_word("life"))
            && life_tokens
                .get(used + 1)
                .map_or(true, |token| matches!(token, Token::Period(_)))
        {
            return Ok(Some(EffectAst::UnlessAction {
                effects,
                alternative: vec![EffectAst::LoseLife { amount, player }],
                player,
            }));
        }
    }

    // Try mana payment first: "pay(s) {mana} [optional trailing condition]"
    // Uses greedy mana parsing — collects mana symbols until first non-mana word,
    // then categorizes remaining tokens to decide whether to accept.
    if action_words.first() == Some(&"pay") || action_words.first() == Some(&"pays") {
        // Skip any non-word tokens between "pay" and mana
        let mana_start = action_tokens
            .iter()
            .skip(1)
            .position(|t| t.as_word().is_some())
            .map(|p| p + 1)
            .unwrap_or(1);
        let mana_tokens = &action_tokens[mana_start..];
        let mut mana = Vec::new();
        let mut remaining_idx = mana_tokens.len();
        for (i, token) in mana_tokens.iter().enumerate() {
            if let Some(word) = token.as_word() {
                match parse_mana_symbol(word) {
                    Ok(symbol) => mana.push(symbol),
                    Err(_) => {
                        remaining_idx = i;
                        break;
                    }
                }
            }
        }

        if !mana.is_empty() {
            // Check what follows the mana symbols
            let remaining_words: Vec<&str> = mana_tokens[remaining_idx..]
                .iter()
                .filter_map(Token::as_word)
                .collect();

            let accept = if remaining_words.is_empty() {
                // Pure mana payment (e.g., "pays {2}")
                true
            } else if remaining_words.first() == Some(&"life") {
                // "pay N life" — not a mana payment, it's a life cost
                false
            } else if remaining_words.first() == Some(&"before") {
                // Timing condition like "before that step" — accept, drop condition
                true
            } else {
                // Unknown trailing tokens (for each, where X is, etc.) — skip for now
                false
            };

            if accept {
                return Ok(Some(EffectAst::UnlessPays {
                    effects,
                    player,
                    mana,
                }));
            }

            if !remaining_words.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "unsupported trailing unless-payment clause (clause: '{}')",
                    words(tokens).join(" ")
                )));
            }
        }
    }

    // Try full-clause parsing first to preserve existing behavior for explicit
    // player phrasing such as "unless that player ...".
    if let Ok(mut alternative) = parse_effect_chain(after_unless) {
        if !alternative.is_empty() {
            for effect in &mut alternative {
                bind_implicit_player_context(effect, player);
            }
            return Ok(Some(EffectAst::UnlessAction {
                effects,
                alternative,
                player,
            }));
        }
    }

    Ok(None)
}

const PRE_CONDITIONAL_SENTENCE_PRIMITIVES: &[SentencePrimitive] = &[
    SentencePrimitive {
        name: "put-multiple-counters-on-target",
        parser: parse_sentence_put_multiple_counters_on_target,
    },
    SentencePrimitive {
        name: "you-and-target-player-each-draw",
        parser: parse_sentence_you_and_target_player_each_draw,
    },
    SentencePrimitive {
        name: "you-and-attacking-player-each-draw-and-lose",
        parser: parse_sentence_you_and_attacking_player_each_draw_and_lose,
    },
    SentencePrimitive {
        name: "sacrifice-it-next-end-step",
        parser: parse_sentence_sacrifice_it_next_end_step,
    },
    SentencePrimitive {
        name: "sacrifice-at-end-of-combat",
        parser: parse_sentence_sacrifice_at_end_of_combat,
    },
    SentencePrimitive {
        name: "token-copy-modifier",
        parser: parse_sentence_token_copy_modifier,
    },
    SentencePrimitive {
        name: "each-player-choose-keep-rest-sacrifice",
        parser: parse_sentence_each_player_choose_and_sacrifice_rest,
    },
    SentencePrimitive {
        name: "target-player-choose-then-put-on-top-library",
        parser: parse_sentence_target_player_chooses_then_puts_on_top_of_library,
    },
    SentencePrimitive {
        name: "target-player-choose-then-you-put-it-onto-battlefield",
        parser: parse_sentence_target_player_chooses_then_you_put_it_onto_battlefield,
    },
    SentencePrimitive {
        name: "exile-instead-of-graveyard",
        parser: parse_sentence_exile_instead_of_graveyard,
    },
];

const POST_CONDITIONAL_SENTENCE_PRIMITIVES: &[SentencePrimitive] = &[
    SentencePrimitive {
        name: "exile-target-creature-with-greatest-power",
        parser: parse_sentence_exile_target_creature_with_greatest_power,
    },
    SentencePrimitive {
        name: "counter-target-spell-thats-second-cast-this-turn",
        parser: parse_sentence_counter_target_spell_thats_second_cast_this_turn,
    },
    SentencePrimitive {
        name: "counter-target-spell-if-it-was-kicked",
        parser: parse_sentence_counter_target_spell_if_it_was_kicked,
    },
    SentencePrimitive {
        name: "destroy-creature-type-of-choice",
        parser: parse_sentence_destroy_creature_type_of_choice,
    },
    SentencePrimitive {
        name: "pump-creature-type-of-choice",
        parser: parse_sentence_pump_creature_type_of_choice,
    },
    SentencePrimitive {
        name: "return-multiple-targets",
        parser: parse_sentence_return_multiple_targets,
    },
    SentencePrimitive {
        name: "for-each-of-target-objects",
        parser: parse_sentence_for_each_of_target_objects,
    },
    SentencePrimitive {
        name: "return-creature-type-of-choice",
        parser: parse_sentence_return_targets_of_creature_type_of_choice,
    },
    SentencePrimitive {
        name: "distribute-counters",
        parser: parse_sentence_distribute_counters,
    },
    SentencePrimitive {
        name: "keyword-then-chain",
        parser: parse_sentence_keyword_then_chain,
    },
    SentencePrimitive {
        name: "chain-then-keyword",
        parser: parse_sentence_chain_then_keyword,
    },
    SentencePrimitive {
        name: "exile-then-may-put-from-exile",
        parser: parse_sentence_exile_then_may_put_from_exile,
    },
    SentencePrimitive {
        name: "exile-source-with-counters",
        parser: parse_sentence_exile_source_with_counters,
    },
    SentencePrimitive {
        name: "destroy-all-attached-to-target",
        parser: parse_sentence_destroy_all_attached_to_target,
    },
    SentencePrimitive {
        name: "comma-then-chain-special",
        parser: parse_sentence_comma_then_chain_special,
    },
    SentencePrimitive {
        name: "destroy-then-land-controller-graveyard-count-damage",
        parser: parse_sentence_destroy_then_land_controller_graveyard_count_damage,
    },
    SentencePrimitive {
        name: "draw-then-connive",
        parser: parse_sentence_draw_then_connive,
    },
    SentencePrimitive {
        name: "return-then-do-same-for-subtypes",
        parser: parse_sentence_return_then_do_same_for_subtypes,
    },
    SentencePrimitive {
        name: "return-then-create",
        parser: parse_sentence_return_then_create,
    },
    SentencePrimitive {
        name: "put-counter-sequence",
        parser: parse_sentence_put_counter_sequence,
    },
    SentencePrimitive {
        name: "gets-then-fights",
        parser: parse_sentence_gets_then_fights,
    },
    SentencePrimitive {
        name: "return-with-counters-on-it",
        parser: parse_sentence_return_with_counters_on_it,
    },
    SentencePrimitive {
        name: "each-player-return-with-additional-counter",
        parser: parse_sentence_each_player_return_with_additional_counter,
    },
    SentencePrimitive {
        name: "sacrifice-any-number",
        parser: parse_sentence_sacrifice_any_number,
    },
    SentencePrimitive {
        name: "sacrifice-one-or-more",
        parser: parse_sentence_sacrifice_one_or_more,
    },
    SentencePrimitive {
        name: "monstrosity",
        parser: parse_sentence_monstrosity,
    },
    SentencePrimitive {
        name: "for-each-counter-removed",
        parser: parse_sentence_for_each_counter_removed,
    },
    SentencePrimitive {
        name: "exile-that-token-end-of-combat",
        parser: parse_sentence_exile_that_token_at_end_of_combat,
    },
    SentencePrimitive {
        name: "sacrifice-that-token-end-of-combat",
        parser: parse_sentence_sacrifice_that_token_at_end_of_combat,
    },
    SentencePrimitive {
        name: "take-extra-turn",
        parser: parse_sentence_take_extra_turn,
    },
    SentencePrimitive {
        name: "earthbend",
        parser: parse_sentence_earthbend,
    },
    SentencePrimitive {
        name: "enchant",
        parser: parse_sentence_enchant,
    },
    SentencePrimitive {
        name: "cant-effect",
        parser: parse_sentence_cant_effect,
    },
    SentencePrimitive {
        name: "prevent-damage",
        parser: parse_sentence_prevent_damage,
    },
    SentencePrimitive {
        name: "shared-color-target-fanout",
        parser: parse_sentence_shared_color_target_fanout,
    },
    SentencePrimitive {
        name: "gain-ability-to-source",
        parser: parse_sentence_gain_ability_to_source,
    },
    SentencePrimitive {
        name: "gain-ability",
        parser: parse_sentence_gain_ability,
    },
    SentencePrimitive {
        name: "vote-with-you",
        parser: parse_sentence_you_and_each_opponent_voted_with_you,
    },
    SentencePrimitive {
        name: "gain-life-equal-to-power",
        parser: parse_sentence_gain_life_equal_to_power,
    },
    SentencePrimitive {
        name: "gain-x-plus-life",
        parser: parse_sentence_gain_x_plus_life,
    },
    SentencePrimitive {
        name: "for-each-exiled-this-way",
        parser: parse_sentence_for_each_exiled_this_way,
    },
    SentencePrimitive {
        name: "each-player-put-permanent-cards-exiled-with-source",
        parser: parse_sentence_each_player_put_permanent_cards_exiled_with_source,
    },
    SentencePrimitive {
        name: "for-each-destroyed-this-way",
        parser: parse_sentence_for_each_destroyed_this_way,
    },
    SentencePrimitive {
        name: "exile-then-return-same-object",
        parser: parse_sentence_exile_then_return_same_object,
    },
    SentencePrimitive {
        name: "search-library",
        parser: parse_sentence_search_library,
    },
    SentencePrimitive {
        name: "shuffle-graveyard-into-library",
        parser: parse_sentence_shuffle_graveyard_into_library,
    },
    SentencePrimitive {
        name: "exile-hand-and-graveyard-bundle",
        parser: parse_sentence_exile_hand_and_graveyard_bundle,
    },
    SentencePrimitive {
        name: "target-player-exiles-creature-and-graveyard",
        parser: parse_sentence_target_player_exiles_creature_and_graveyard,
    },
    SentencePrimitive {
        name: "play-from-graveyard",
        parser: parse_sentence_play_from_graveyard,
    },
    SentencePrimitive {
        name: "look-at-top-then-exile-one",
        parser: parse_sentence_look_at_top_then_exile_one,
    },
    SentencePrimitive {
        name: "look-at-hand",
        parser: parse_sentence_look_at_hand,
    },
    SentencePrimitive {
        name: "gain-life-equal-to-age",
        parser: parse_sentence_gain_life_equal_to_age,
    },
    SentencePrimitive {
        name: "for-each-player-doesnt",
        parser: parse_sentence_for_each_player_doesnt,
    },
    SentencePrimitive {
        name: "for-each-opponent-doesnt",
        parser: parse_sentence_for_each_opponent_doesnt,
    },
    SentencePrimitive {
        name: "each-opponent-loses-x-and-you-gain-x",
        parser: parse_sentence_each_opponent_loses_x_and_you_gain_x,
    },
    SentencePrimitive {
        name: "vote-start",
        parser: parse_sentence_vote_start,
    },
    SentencePrimitive {
        name: "for-each-vote-clause",
        parser: parse_sentence_for_each_vote_clause,
    },
    SentencePrimitive {
        name: "vote-extra",
        parser: parse_sentence_vote_extra,
    },
    SentencePrimitive {
        name: "after-turn",
        parser: parse_sentence_after_turn,
    },
    SentencePrimitive {
        name: "same-name-target-fanout",
        parser: parse_sentence_same_name_target_fanout,
    },
    SentencePrimitive {
        name: "same-name-gets-fanout",
        parser: parse_sentence_same_name_gets_fanout,
    },
    SentencePrimitive {
        name: "delayed-next-end-step",
        parser: parse_sentence_delayed_until_next_end_step,
    },
    SentencePrimitive {
        name: "delayed-trigger-this-turn",
        parser: parse_sentence_delayed_trigger_this_turn,
    },
    SentencePrimitive {
        name: "delayed-when-that-dies-this-turn",
        parser: parse_delayed_when_that_dies_this_turn_sentence,
    },
    SentencePrimitive {
        name: "destroy-or-exile-all-split",
        parser: parse_sentence_destroy_or_exile_all_split,
    },
    SentencePrimitive {
        name: "exile-up-to-one-each-target-type",
        parser: parse_sentence_exile_up_to_one_each_target_type,
    },
    SentencePrimitive {
        name: "exile-multi-target",
        parser: parse_sentence_exile_multi_target,
    },
    SentencePrimitive {
        name: "destroy-multi-target",
        parser: parse_sentence_destroy_multi_target,
    },
    SentencePrimitive {
        name: "reveal-selected-cards-in-your-hand",
        parser: parse_sentence_reveal_selected_cards_in_your_hand,
    },
    SentencePrimitive {
        name: "damage-unless-controller-has-source-deal-damage",
        parser: parse_sentence_damage_unless_controller_has_source_deal_damage,
    },
    SentencePrimitive {
        name: "unless-pays",
        parser: parse_sentence_unless_pays,
    },
];

fn parse_effect_sentence(tokens: &[Token]) -> Result<Vec<EffectAst>, CardTextError> {
    // Generic support for trailing "where X is ..." clauses.
    //
    // Many Oracle texts define a computed X (not cost-derived X) using:
    //   "... X ..., where X is <expression>."
    //
    // We parse the where-X value, strip the suffix clause for normal parsing,
    // then substitute the parsed `Value::X` occurrences with that value.
    let clause_words = words(tokens);
    let Some(where_idx) = clause_words
        .windows(3)
        .position(|window| window == ["where", "x", "is"])
    else {
        return parse_effect_sentence_inner(tokens);
    };
    let Some(where_token_idx) = token_index_for_word_index(tokens, where_idx) else {
        return Err(CardTextError::ParseError(format!(
            "unsupported where-x clause (clause: '{}')",
            clause_words.join(" ")
        )));
    };
    let where_tokens = &tokens[where_token_idx..];

    let stripped = trim_edge_punctuation(&tokens[..where_token_idx]);
    let stripped_words = words(&stripped);
    let where_words = words(where_tokens);

    // Special-case common "where X is its power/toughness/mana value" patterns, because
    // resolving "its" depends on whether the main clause is targeting something.
    let where_value = match where_words.get(3..) {
        Some(["its", "power"]) => {
            if stripped_words.iter().any(|w| *w == "target") {
                Value::PowerOf(Box::new(crate::target::ChooseSpec::target(
                    crate::target::ChooseSpec::Object(ObjectFilter::default()),
                )))
            } else {
                Value::SourcePower
            }
        }
        Some(["its", "toughness"]) => {
            if stripped_words.iter().any(|w| *w == "target") {
                Value::ToughnessOf(Box::new(crate::target::ChooseSpec::target(
                    crate::target::ChooseSpec::Object(ObjectFilter::default()),
                )))
            } else {
                Value::SourceToughness
            }
        }
        Some(["its", "mana", "value"]) => {
            Value::ManaValueOf(Box::new(if stripped_words.iter().any(|w| *w == "target") {
                crate::target::ChooseSpec::target(crate::target::ChooseSpec::Object(
                    ObjectFilter::default(),
                ))
            } else {
                crate::target::ChooseSpec::Source
            }))
        }
        Some(["this", "creatures", "power"]) => Value::SourcePower,
        Some(["this", "creatures", "toughness"]) => Value::SourceToughness,
        Some(["this", "creatures", "mana", "value"]) => {
            Value::ManaValueOf(Box::new(crate::target::ChooseSpec::Source))
        }
        Some(["that", "creatures", "power"]) => {
            Value::PowerOf(Box::new(crate::target::ChooseSpec::target(
                crate::target::ChooseSpec::Object(ObjectFilter::default()),
            )))
        }
        Some(["that", "creatures", "toughness"]) => {
            Value::ToughnessOf(Box::new(crate::target::ChooseSpec::target(
                crate::target::ChooseSpec::Object(ObjectFilter::default()),
            )))
        }
        Some(["that", "creatures", "mana", "value"]) => {
            Value::ManaValueOf(Box::new(crate::target::ChooseSpec::target(
                crate::target::ChooseSpec::Object(ObjectFilter::default()),
            )))
        }
        _ => parse_where_x_value_clause(where_tokens).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported where-x clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?,
    };

    let mut effects = parse_effect_sentence_inner(&stripped)?;
    replace_unbound_x_in_effects_anywhere(&mut effects, &where_value, &clause_words.join(" "))?;
    Ok(effects)
}

fn parse_effect_sentence_inner(tokens: &[Token]) -> Result<Vec<EffectAst>, CardTextError> {
    parser_trace("parse_effect_sentence:entry", tokens);
    let sentence_words = words(tokens);
    if let Some(effects) = parse_redirect_next_damage_sentence(tokens)? {
        return Ok(effects);
    }
    if let Some(effects) = parse_prevent_next_time_damage_sentence(tokens)? {
        return Ok(effects);
    }
    if is_activate_only_restriction_sentence(tokens) {
        return Ok(Vec::new());
    }
    if is_trigger_only_restriction_sentence(tokens) {
        return Ok(Vec::new());
    }
    let is_each_player_lose_discard_sacrifice_chain = sentence_words
        .starts_with(&["each", "player"])
        && sentence_words.contains(&"then")
        && (sentence_words.contains(&"lose") || sentence_words.contains(&"loses"))
        && (sentence_words.contains(&"discard") || sentence_words.contains(&"discards"))
        && (sentence_words.contains(&"sacrifice") || sentence_words.contains(&"sacrifices"));
    if is_each_player_lose_discard_sacrifice_chain {
        return Err(CardTextError::ParseError(format!(
            "unsupported each-player lose/discard/sacrifice chain clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let is_each_player_exile_sacrifice_return_exiled = sentence_words
        .starts_with(&["each", "player", "exiles", "all"])
        && sentence_words.contains(&"sacrifices")
        && sentence_words.contains(&"puts")
        && sentence_words.contains(&"exiled")
        && sentence_words.contains(&"this")
        && sentence_words.contains(&"way");
    if is_each_player_exile_sacrifice_return_exiled {
        return Err(CardTextError::ParseError(format!(
            "unsupported each-player exile/sacrifice/return-this-way clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_loses_all_abilities = (sentence_words.contains(&"lose")
        || sentence_words.contains(&"loses"))
        && sentence_words
            .windows(2)
            .any(|window| window == ["all", "abilities"]);
    if has_loses_all_abilities && sentence_words.contains(&"becomes") {
        return Err(CardTextError::ParseError(format!(
            "unsupported loses-all-abilities with becomes clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    // where-X clauses are handled by the parse_effect_sentence wrapper.
    let has_spent_to_cast_this_spell = sentence_words
        .windows(6)
        .any(|window| window == ["was", "spent", "to", "cast", "this", "spell"]);
    if has_spent_to_cast_this_spell
        && !sentence_words
            .iter()
            .any(|word| matches!(*word, "if" | "unless"))
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported spent-to-cast conditional clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_would_enter_instead_replacement = sentence_words.iter().any(|word| *word == "would")
        && sentence_words
            .iter()
            .any(|word| *word == "enter" || *word == "enters")
        && sentence_words.iter().any(|word| *word == "instead");
    if has_would_enter_instead_replacement {
        return Err(CardTextError::ParseError(format!(
            "unsupported would-enter replacement clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_different_mana_value_constraint = sentence_words
        .windows(3)
        .any(|window| window == ["different", "mana", "value"]);
    if has_different_mana_value_constraint {
        return Err(CardTextError::ParseError(format!(
            "unsupported different-mana-value constraint clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_most_common_color_constraint = sentence_words
        .windows(5)
        .any(|window| window == ["most", "common", "color", "among", "all"])
        && sentence_words.contains(&"permanents");
    if has_most_common_color_constraint {
        return Err(CardTextError::ParseError(format!(
            "unsupported most-common-color constraint clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_power_vs_count_constraint = sentence_words.contains(&"power")
        && sentence_words
            .windows(8)
            .any(|window| window == ["less", "than", "or", "equal", "to", "the", "number", "of"]);
    if has_power_vs_count_constraint {
        return Err(CardTextError::ParseError(format!(
            "unsupported power-vs-count conditional clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_put_into_graveyards_from_battlefield_this_turn =
        sentence_words.windows(8).any(|window| {
            window
                == [
                    "put",
                    "into",
                    "graveyards",
                    "from",
                    "the",
                    "battlefield",
                    "this",
                    "turn",
                ]
        });
    if has_put_into_graveyards_from_battlefield_this_turn {
        return Err(CardTextError::ParseError(format!(
            "unsupported put-into-graveyards-from-battlefield count clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_phase_out_until_leaves = sentence_words
        .iter()
        .any(|word| matches!(*word, "phase" | "phases" | "phased"))
        && sentence_words.contains(&"until")
        && sentence_words
            .windows(3)
            .any(|window| window == ["leaves", "the", "battlefield"]);
    if has_phase_out_until_leaves {
        return Err(CardTextError::ParseError(format!(
            "unsupported phase-out-until-leaves clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let is_for_each_vote_investigate = sentence_words.starts_with(&["for", "each"])
        && (sentence_words
            .iter()
            .any(|word| *word == "vote" || *word == "votes"))
        && sentence_words
            .iter()
            .any(|word| *word == "investigate" || *word == "investigates");
    if !is_for_each_vote_investigate
        && sentence_words
            .iter()
            .any(|word| *word == "investigate" || *word == "investigates")
        && sentence_words
            .windows(2)
            .any(|window| window == ["for", "each"])
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported investigate-for-each clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_same_name_as_another_in_hand = sentence_words
        .windows(6)
        .any(|window| window == ["same", "name", "as", "another", "card", "in"])
        && sentence_words.contains(&"hand");
    if has_same_name_as_another_in_hand {
        return Err(CardTextError::ParseError(format!(
            "unsupported same-name-as-another-in-hand discard clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_for_each_mana_from_spent_to_cast = sentence_words
        .windows(4)
        .any(|window| window == ["for", "each", "mana", "from"])
        && sentence_words.contains(&"spent")
        && sentence_words
            .windows(4)
            .any(|window| window == ["cast", "this", "spell", "create"]);
    if has_for_each_mana_from_spent_to_cast {
        return Err(CardTextError::ParseError(format!(
            "unsupported for-each-mana-from-spent clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_when_you_sacrifice_this_way = sentence_words
        .windows(3)
        .any(|window| window == ["when", "you", "sacrifice"])
        && sentence_words
            .windows(2)
            .any(|window| window == ["this", "way"]);
    if has_when_you_sacrifice_this_way {
        return Err(CardTextError::ParseError(format!(
            "unsupported when-you-sacrifice-this-way clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_sacrifice_any_number_then_draw_that_many = sentence_words
        .iter()
        .any(|word| *word == "sacrifice" || *word == "sacrifices")
        && sentence_words
            .windows(3)
            .any(|window| window == ["any", "number", "of"])
        && sentence_words
            .iter()
            .any(|word| *word == "draw" || *word == "draws")
        && sentence_words
            .windows(2)
            .any(|window| window == ["that", "many"]);
    if has_sacrifice_any_number_then_draw_that_many {
        return Err(CardTextError::ParseError(format!(
            "unsupported sacrifice-any-number-then-draw-that-many clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_greatest_mana_value_clause = sentence_words
        .windows(3)
        .any(|window| window == ["greatest", "mana", "value"]);
    if has_greatest_mana_value_clause {
        return Err(CardTextError::ParseError(format!(
            "unsupported greatest-mana-value selection clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_least_power_among = sentence_words
        .windows(4)
        .any(|window| window == ["least", "power", "among", "creatures"]);
    if has_least_power_among {
        return Err(CardTextError::ParseError(format!(
            "unsupported least-power-among-creatures selection clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_villainous_choice =
        sentence_words.contains(&"villainous") && sentence_words.contains(&"choice");
    if has_villainous_choice {
        return Err(CardTextError::ParseError(format!(
            "unsupported villainous-choice clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_divided_evenly = sentence_words
        .windows(2)
        .any(|window| window == ["divided", "evenly"]);
    if has_divided_evenly {
        return Err(CardTextError::ParseError(format!(
            "unsupported divided-evenly damage clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_with_different_names = sentence_words
        .windows(2)
        .any(|window| window == ["different", "names"]);
    if has_with_different_names {
        return Err(CardTextError::ParseError(format!(
            "unsupported different-names selection clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_chosen_at_random = sentence_words
        .windows(3)
        .any(|window| window == ["chosen", "at", "random"]);
    if has_chosen_at_random {
        return Err(CardTextError::ParseError(format!(
            "unsupported chosen-at-random clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_for_each_card_exiled_from_hand_this_way = sentence_words
        .windows(4)
        .any(|window| window == ["for", "each", "card", "exiled"])
        && sentence_words
            .windows(3)
            .any(|window| window == ["hand", "this", "way"]);
    if has_for_each_card_exiled_from_hand_this_way {
        return Err(CardTextError::ParseError(format!(
            "unsupported draw-for-each-card-exiled-from-hand clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_defending_players_choice_clause = sentence_words.contains(&"defending")
        && sentence_words
            .windows(3)
            .any(|window| window == ["player's", "choice", "target"])
        || sentence_words
            .windows(3)
            .any(|window| window == ["defending", "player's", "choice"]);
    if has_defending_players_choice_clause {
        return Err(CardTextError::ParseError(format!(
            "unsupported defending-players-choice clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_target_creature_token_player_planeswalker_clause = sentence_words.contains(&"target")
        && sentence_words.contains(&"creature")
        && sentence_words.contains(&"token")
        && sentence_words.contains(&"player")
        && sentence_words.contains(&"planeswalker");
    if has_target_creature_token_player_planeswalker_clause {
        return Err(CardTextError::ParseError(format!(
            "unsupported creature-token/player/planeswalker target clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_if_you_sacrifice_an_island_this_way = sentence_words
        .windows(5)
        .any(|window| window == ["if", "you", "sacrifice", "an", "island"])
        && sentence_words
            .windows(2)
            .any(|window| window == ["this", "way"]);
    if has_if_you_sacrifice_an_island_this_way {
        return Err(CardTextError::ParseError(format!(
            "unsupported if-you-sacrifice-an-island-this-way clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_commander_cast_count_clause = sentence_words
        .windows(3)
        .any(|window| window == ["for", "each", "time"])
        && sentence_words.contains(&"cast")
        && sentence_words.contains(&"commander")
        && sentence_words
            .windows(4)
            .any(|window| window == ["from", "the", "command", "zone"]);
    if has_commander_cast_count_clause {
        return Err(CardTextError::ParseError(format!(
            "unsupported commander-cast-count clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_spent_to_cast_clause = sentence_words
        .windows(3)
        .any(|window| window == ["spent", "to", "cast"]);
    if has_spent_to_cast_clause {
        return Err(CardTextError::ParseError(format!(
            "unsupported spent-to-cast condition clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_face_down_clause = sentence_words
        .windows(2)
        .any(|window| window == ["face", "down"]);
    if has_face_down_clause {
        return Err(CardTextError::ParseError(format!(
            "unsupported face-down clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_copy_spell_legendary_exception = sentence_words.contains(&"copy")
        && sentence_words.contains(&"spell")
        && sentence_words.contains(&"legendary")
        && (sentence_words.contains(&"except") || sentence_words.contains(&"isnt"));
    if has_copy_spell_legendary_exception {
        return Err(CardTextError::ParseError(format!(
            "unsupported copy-spell legendary-exception clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    let has_return_each_creature_that_isnt_list = sentence_words
        .starts_with(&["return", "each", "creature", "that", "isnt"])
        && sentence_words.iter().filter(|word| **word == "or").count() >= 1;
    if has_return_each_creature_that_isnt_list {
        return Err(CardTextError::ParseError(format!(
            "unsupported return-each-creature-that-isnt-list clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    if sentence_words.starts_with(&["round", "up", "each", "time"]) {
        // "Round up each time." is reminder text for half P/T copy effects.
        // The semantic behavior is represented by the underlying token-copy primitive.
        parser_trace("parse_effect_sentence:round-up-reminder", tokens);
        return Ok(Vec::new());
    }
    if let Some(stripped) = strip_labeled_conditional_prefix(tokens) {
        parser_trace("parse_effect_sentence:conditional-labeled", stripped);
        return parse_conditional_sentence(stripped);
    }
    if tokens.first().is_some_and(|token| token.is_word("then"))
        && tokens.get(1).is_some_and(|token| token.is_word("if"))
    {
        parser_trace("parse_effect_sentence:conditional-then", &tokens[1..]);
        return parse_conditional_sentence(&tokens[1..]);
    }
    if tokens.first().is_some_and(|token| token.is_word("then")) && tokens.len() > 1 {
        parser_trace("parse_effect_sentence:leading-then", &tokens[1..]);
        return parse_effect_sentence(&tokens[1..]);
    }
    if let Some(effects) = run_sentence_primitives(tokens, PRE_CONDITIONAL_SENTENCE_PRIMITIVES)? {
        return Ok(effects);
    }
    if tokens.first().is_some_and(|token| token.is_word("if")) {
        parser_trace("parse_effect_sentence:conditional", tokens);
        return parse_conditional_sentence(tokens);
    }
    if let Some(effects) = run_sentence_primitives(tokens, POST_CONDITIONAL_SENTENCE_PRIMITIVES)? {
        return Ok(effects);
    }
    if is_negated_untap_clause(&sentence_words) {
        return Err(CardTextError::ParseError(format!(
            "unsupported negated untap clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }

    if is_ring_tempts_sentence(tokens) {
        return Err(CardTextError::ParseError(format!(
            "unsupported ring tempts clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }
    if is_enters_as_copy_clause(&sentence_words) {
        return Err(CardTextError::ParseError(format!(
            "unsupported enters-as-copy replacement clause (clause: '{}')",
            sentence_words.join(" ")
        )));
    }

    let mut effects = parse_effect_chain(tokens)?;
    apply_where_x_to_damage_amounts(tokens, &mut effects)?;
    Ok(effects)
}

fn is_enters_as_copy_clause(words: &[&str]) -> bool {
    let has_enter_before_as_copy = words
        .windows(3)
        .position(|window| window == ["as", "a", "copy"] || window == ["as", "an", "copy"])
        .is_some_and(|idx| {
            words[..idx]
                .iter()
                .any(|word| *word == "enter" || *word == "enters")
        });
    let has_enter_before_as_copy_no_article = words
        .windows(2)
        .position(|window| window == ["as", "copy"])
        .is_some_and(|idx| {
            words[..idx]
                .iter()
                .any(|word| *word == "enter" || *word == "enters")
        });
    has_enter_before_as_copy || has_enter_before_as_copy_no_article
}

fn strip_labeled_conditional_prefix(tokens: &[Token]) -> Option<&[Token]> {
    let if_idx = tokens.iter().position(|token| token.is_word("if"))?;
    if !(1..=3).contains(&if_idx) {
        return None;
    }
    if !tokens[..if_idx]
        .iter()
        .all(|token| matches!(token, Token::Word(_, _)))
    {
        return None;
    }

    let prefix_words = words(&tokens[..if_idx]);
    if prefix_words.is_empty() {
        return None;
    }
    let is_known_label = matches!(
        prefix_words[0],
        "adamant"
            | "addendum"
            | "ascend"
            | "battalion"
            | "delirium"
            | "domain"
            | "ferocious"
            | "formidable"
            | "hellbent"
            | "metalcraft"
            | "morbid"
            | "raid"
            | "revolt"
            | "spectacle"
            | "spell"
            | "surge"
            | "threshold"
            | "undergrowth"
    );
    if !is_known_label {
        return None;
    }

    Some(&tokens[if_idx..])
}

fn is_negated_untap_clause(words: &[&str]) -> bool {
    if words.len() < 3 {
        return false;
    }
    let has_untap = words.contains(&"untap") || words.contains(&"untaps");
    let has_negation = words.contains(&"doesnt")
        || words.contains(&"dont")
        || words.windows(2).any(|pair| pair == ["does", "not"])
        || words.windows(2).any(|pair| pair == ["do", "not"])
        || words.contains(&"cant")
        || words.windows(2).any(|pair| pair == ["can", "not"]);
    has_untap && has_negation
}

fn parse_token_copy_modifier_sentence(tokens: &[Token]) -> Option<EffectAst> {
    let filtered: Vec<&str> = words(tokens)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();

    let is_gain_haste_until_eot = matches!(
        filtered.as_slice(),
        ["it", "gains", "haste", "until", "end", "of", "turn"]
            | ["they", "gain", "haste", "until", "end", "of", "turn"]
    );
    if is_gain_haste_until_eot {
        return Some(EffectAst::TokenCopyGainHasteUntilEot);
    }

    let is_has_haste = matches!(
        filtered.as_slice(),
        ["it", "has", "haste"] | ["they", "have", "haste"]
    );
    if is_has_haste {
        return Some(EffectAst::TokenCopyHasHaste);
    }

    if filtered.starts_with(&["sacrifice", "it"]) || filtered.starts_with(&["sacrifice", "them"]) {
        let has_next_end_step = filtered
            .windows(6)
            .any(|window| window == ["at", "beginning", "of", "next", "end", "step"]);
        if has_next_end_step {
            return Some(EffectAst::TokenCopySacrificeAtNextEndStep);
        }
    }
    if filtered.starts_with(&["exile", "it"]) || filtered.starts_with(&["exile", "them"]) {
        let has_next_end_step = filtered
            .windows(6)
            .any(|window| window == ["at", "beginning", "of", "next", "end", "step"]);
        if has_next_end_step {
            return Some(EffectAst::TokenCopyExileAtNextEndStep);
        }
    }

    let starts_delayed_end_step_sacrifice = filtered.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "the",
        "end",
        "step",
        "sacrifice",
    ]) || filtered.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "the",
        "next",
        "end",
        "step",
        "sacrifice",
    ]) || filtered.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "next",
        "end",
        "step",
        "sacrifice",
    ]);
    if starts_delayed_end_step_sacrifice {
        return Some(EffectAst::TokenCopySacrificeAtNextEndStep);
    }
    let starts_delayed_end_step_exile = filtered.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "the",
        "end",
        "step",
        "exile",
    ]) || filtered.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "the",
        "next",
        "end",
        "step",
        "exile",
    ]) || filtered.starts_with(&[
        "at",
        "the",
        "beginning",
        "of",
        "next",
        "end",
        "step",
        "exile",
    ]);
    if starts_delayed_end_step_exile {
        return Some(EffectAst::TokenCopyExileAtNextEndStep);
    }

    None
}

fn parse_delayed_until_next_end_step_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if tokens.is_empty() {
        return Ok(None);
    }

    let mut idx = 0usize;
    if !tokens.get(idx).is_some_and(|token| token.is_word("at")) {
        return Ok(None);
    }
    idx += 1;

    if tokens.get(idx).is_some_and(|token| token.is_word("the")) {
        idx += 1;
    }
    if !tokens
        .get(idx)
        .is_some_and(|token| token.is_word("beginning"))
    {
        return Ok(None);
    }
    idx += 1;
    if !tokens.get(idx).is_some_and(|token| token.is_word("of")) {
        return Ok(None);
    }
    idx += 1;

    if tokens.get(idx).is_some_and(|token| token.is_word("the")) {
        idx += 1;
    }

    let mut player = if tokens.get(idx).is_some_and(|token| token.is_word("your")) {
        idx += 1;
        PlayerFilter::You
    } else {
        PlayerFilter::Any
    };
    let mut start_next_turn = false;

    if tokens.get(idx).is_some_and(|token| token.is_word("next")) {
        if !tokens
            .get(idx + 1)
            .is_some_and(|token| token.is_word("end"))
            || !tokens
                .get(idx + 2)
                .is_some_and(|token| token.is_word("step"))
        {
            return Ok(None);
        }
        idx += 3;
    } else {
        if !tokens.get(idx).is_some_and(|token| token.is_word("end"))
            || !tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("step"))
        {
            return Ok(None);
        }
        idx += 2;
    }

    if tokens.get(idx).is_some_and(|token| token.is_word("of")) {
        idx += 1;
        if tokens.get(idx).is_some_and(|token| token.is_word("that"))
            && tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("player") || token.is_word("players"))
        {
            player = PlayerFilter::IteratedPlayer;
            idx += 2;
        } else if tokens.get(idx).is_some_and(|token| token.is_word("your")) {
            player = PlayerFilter::You;
            idx += 1;
        } else if tokens.get(idx).is_some_and(|token| token.is_word("target"))
            && tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("player"))
        {
            player = PlayerFilter::Target(Box::new(PlayerFilter::Any));
            idx += 2;
        } else {
            return Ok(None);
        }

        if !tokens.get(idx).is_some_and(|token| token.is_word("next"))
            || !tokens
                .get(idx + 1)
                .is_some_and(|token| token.is_word("turn"))
        {
            return Ok(None);
        }
        idx += 2;
        start_next_turn = true;
    }

    if matches!(tokens.get(idx), Some(Token::Comma(_))) {
        idx += 1;
    }
    let remainder = trim_commas(&tokens[idx..]);
    if remainder.is_empty() {
        return Err(CardTextError::ParseError(
            "missing delayed end-step effect clause".to_string(),
        ));
    }

    let delayed_effects = parse_effect_chain(&remainder)?;
    if delayed_effects.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing delayed end-step effect clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    if start_next_turn {
        let player_ast = match player {
            PlayerFilter::You => PlayerAst::You,
            PlayerFilter::IteratedPlayer => PlayerAst::That,
            PlayerFilter::Target(_) => PlayerAst::Target,
            PlayerFilter::Opponent => PlayerAst::Opponent,
            _ => PlayerAst::Any,
        };
        Ok(Some(vec![EffectAst::DelayedUntilEndStepOfExtraTurn {
            player: player_ast,
            effects: delayed_effects,
        }]))
    } else {
        Ok(Some(vec![EffectAst::DelayedUntilNextEndStep {
            player,
            effects: delayed_effects,
        }]))
    }
}

fn parse_sentence_delayed_trigger_this_turn(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if !tokens
        .first()
        .is_some_and(|token| token.is_word("when") || token.is_word("whenever"))
    {
        return Ok(None);
    }

    let Some(comma_idx) = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)))
    else {
        return Ok(None);
    };

    let mut trigger_tokens = trim_commas(&tokens[..comma_idx]);
    if trigger_tokens
        .first()
        .is_some_and(|token| token.is_word("when") || token.is_word("whenever"))
    {
        trigger_tokens = trigger_tokens[1..].to_vec();
    }
    if trigger_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing delayed trigger clause before comma (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let trigger_words = words(&trigger_tokens);
    if trigger_words.len() < 3 || !trigger_words.ends_with(&["this", "turn"]) {
        return Ok(None);
    }

    let trim_start = token_index_for_word_index(&trigger_tokens, trigger_words.len() - 2)
        .unwrap_or(trigger_tokens.len());
    let trigger_core_tokens = trim_commas(&trigger_tokens[..trim_start]);
    if trigger_core_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing delayed trigger clause before 'this turn' (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    let trigger = parse_trigger_clause(&trigger_core_tokens)?;
    if matches!(trigger, TriggerSpec::Custom(_)) {
        return Err(CardTextError::ParseError(format!(
            "unsupported delayed trigger clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let remainder = trim_commas(&tokens[comma_idx + 1..]);
    if remainder.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing delayed trigger effect clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let delayed_effects = parse_effect_chain(&remainder)?;
    if delayed_effects.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing delayed trigger effect clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    Ok(Some(vec![EffectAst::DelayedTriggerThisTurn {
        trigger,
        effects: delayed_effects,
    }]))
}

fn parse_delayed_when_that_dies_this_turn_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.len() < 6 {
        return Ok(None);
    }
    if !matches!(
        clause_words.first().copied(),
        Some("when" | "whenever" | "if")
    ) {
        return Ok(None);
    }
    let mut delayed_filter: Option<ObjectFilter> = None;
    let split_after_word_idx = if clause_words.get(1) == Some(&"that") {
        let Some(dies_idx) = clause_words.iter().position(|word| *word == "dies") else {
            return Ok(None);
        };
        if clause_words.get(dies_idx + 1) != Some(&"this")
            || clause_words.get(dies_idx + 2) != Some(&"turn")
        {
            return Ok(None);
        }
        dies_idx + 2
    } else if let Some(dealt_idx) = clause_words
        .windows(7)
        .position(|window| window == ["dealt", "damage", "this", "way", "dies", "this", "turn"])
    {
        if dealt_idx <= 1 {
            return Ok(None);
        }
        let subject_start = token_index_for_word_index(tokens, 1).unwrap_or(tokens.len());
        let subject_end = token_index_for_word_index(tokens, dealt_idx).unwrap_or(tokens.len());
        if subject_start >= subject_end {
            return Ok(None);
        }
        let mut subject_tokens = trim_edge_punctuation(&tokens[subject_start..subject_end]);
        if subject_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing object filter in delayed dies-this-way clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let stripped_subject = strip_leading_articles(&subject_tokens);
        if !stripped_subject.is_empty() {
            subject_tokens = stripped_subject;
        }
        delayed_filter = Some(parse_object_filter(&subject_tokens, false).map_err(|_| {
            CardTextError::ParseError(format!(
                "unsupported object filter in delayed dies-this-way clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?);
        dealt_idx + 6
    } else {
        return Ok(None);
    };
    let split_idx =
        token_index_for_word_index(tokens, split_after_word_idx + 1).unwrap_or(tokens.len());
    let mut remainder = &tokens[split_idx..];
    if matches!(remainder.first(), Some(Token::Comma(_))) {
        remainder = &remainder[1..];
    }
    let remainder = trim_commas(remainder);
    if remainder.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing delayed dies-this-turn effect clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let delayed_effects = parse_effect_chain(&remainder)?;
    if delayed_effects.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing delayed dies-this-turn effect clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(Some(vec![EffectAst::DelayedWhenLastObjectDiesThisTurn {
        filter: delayed_filter,
        effects: delayed_effects,
    }]))
}

fn parse_each_player_choose_and_sacrifice_rest(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let all_words = words(tokens);
    if all_words.len() < 6 {
        return Ok(None);
    }

    if !all_words.starts_with(&["each", "player", "chooses"])
        && !all_words.starts_with(&["each", "player", "choose"])
    {
        return Ok(None);
    }

    let then_idx = tokens.iter().position(|token| token.is_word("then"));
    let Some(then_idx) = then_idx else {
        return Ok(None);
    };

    let after_then = &tokens[then_idx + 1..];
    let after_words = words(after_then);
    if !(after_words.starts_with(&["sacrifice", "the", "rest"])
        || after_words.starts_with(&["sacrifices", "the", "rest"]))
    {
        return Ok(None);
    }

    let choose_tokens = &tokens[3..then_idx];
    if choose_tokens.is_empty() {
        return Ok(None);
    }

    let from_idx = find_from_among(choose_tokens);
    let Some(from_idx) = from_idx else {
        return Ok(None);
    };

    let (list_tokens, base_tokens) = if from_idx == 0 {
        let list_start = find_list_start(&choose_tokens[2..])
            .map(|idx| idx + 2)
            .ok_or_else(|| {
                CardTextError::ParseError("missing choice list after 'from among'".to_string())
            })?;
        (
            choose_tokens.get(list_start..).unwrap_or_default(),
            choose_tokens.get(2..list_start).unwrap_or_default(),
        )
    } else {
        (
            choose_tokens.get(..from_idx).unwrap_or_default(),
            choose_tokens.get(from_idx + 2..).unwrap_or_default(),
        )
    };

    let list_tokens = trim_commas(list_tokens);
    let base_tokens = trim_commas(base_tokens);
    if list_tokens.is_empty() || base_tokens.is_empty() {
        return Ok(None);
    }

    let mut base_filter = parse_object_filter(&base_tokens, false).map_err(|_| {
        CardTextError::ParseError(format!(
            "unsupported base filter in choose-and-sacrifice clause (clause: '{}')",
            all_words.join(" ")
        ))
    })?;
    if base_filter.controller.is_none() {
        base_filter.controller = Some(PlayerFilter::IteratedPlayer);
    }

    let mut effects = Vec::new();
    let keep_tag: TagKey = "keep".into();

    for segment in split_choose_list(&list_tokens) {
        let segment = strip_leading_articles(&segment);
        if segment.is_empty() {
            continue;
        }
        let segment_filter = parse_object_filter(&segment, false).map_err(|_| {
            CardTextError::ParseError(format!(
                "unsupported choice filter in choose-and-sacrifice clause (clause: '{}')",
                all_words.join(" ")
            ))
        })?;
        let mut combined = merge_filters(&base_filter, &segment_filter);
        combined = combined.not_tagged(keep_tag.clone());
        effects.push(EffectAst::ChooseObjects {
            filter: combined,
            count: ChoiceCount::exactly(1),
            player: PlayerAst::Implicit,
            tag: keep_tag.clone(),
        });
    }

    if effects.is_empty() {
        return Ok(None);
    }

    let sacrifice_filter = base_filter.clone().not_tagged(keep_tag.clone());
    effects.push(EffectAst::SacrificeAll {
        filter: sacrifice_filter,
        player: PlayerAst::Implicit,
    });

    Ok(Some(EffectAst::ForEachPlayer { effects }))
}

fn find_from_among(tokens: &[Token]) -> Option<usize> {
    tokens.iter().enumerate().find_map(|(idx, token)| {
        if token.is_word("from") && tokens.get(idx + 1).is_some_and(|t| t.is_word("among")) {
            Some(idx)
        } else {
            None
        }
    })
}

fn find_list_start(tokens: &[Token]) -> Option<usize> {
    for (idx, token) in tokens.iter().enumerate() {
        let Some(word) = token.as_word() else {
            continue;
        };
        if is_article(word) {
            if tokens
                .get(idx + 1)
                .and_then(Token::as_word)
                .and_then(parse_card_type)
                .is_some()
            {
                return Some(idx);
            }
        } else if parse_card_type(word).is_some() {
            return Some(idx);
        }
    }
    None
}

fn trim_commas(tokens: &[Token]) -> Vec<Token> {
    let mut start = 0usize;
    let mut end = tokens.len();
    while start < end && matches!(tokens[start], Token::Comma(_)) {
        start += 1;
    }
    while end > start && matches!(tokens[end - 1], Token::Comma(_)) {
        end -= 1;
    }
    tokens[start..end].to_vec()
}

fn trim_edge_punctuation(tokens: &[Token]) -> Vec<Token> {
    let mut start = 0usize;
    let mut end = tokens.len();
    while start < end
        && matches!(
            tokens[start],
            Token::Comma(_) | Token::Period(_) | Token::Semicolon(_)
        )
    {
        start += 1;
    }
    while end > start
        && matches!(
            tokens[end - 1],
            Token::Comma(_) | Token::Period(_) | Token::Semicolon(_)
        )
    {
        end -= 1;
    }
    tokens[start..end].to_vec()
}

fn strip_leading_articles(tokens: &[Token]) -> Vec<Token> {
    let mut start = 0usize;
    while start < tokens.len() {
        if let Some(word) = tokens[start].as_word()
            && is_article(word)
        {
            start += 1;
            continue;
        }
        break;
    }
    tokens[start..].to_vec()
}

fn split_choose_list(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut segments = Vec::new();
    for segment in split_on_and(tokens) {
        for sub in split_on_comma(&segment) {
            let trimmed = trim_commas(&sub);
            if !trimmed.is_empty() {
                segments.push(trimmed);
            }
        }
    }
    segments
}

fn merge_filters(base: &ObjectFilter, specific: &ObjectFilter) -> ObjectFilter {
    let mut merged = base.clone();

    if !specific.card_types.is_empty() {
        merged.card_types = specific.card_types.clone();
    }
    if !specific.all_card_types.is_empty() {
        merged.all_card_types = specific.all_card_types.clone();
    }
    if !specific.subtypes.is_empty() {
        merged.subtypes.extend(specific.subtypes.clone());
    }
    if !specific.excluded_card_types.is_empty() {
        merged
            .excluded_card_types
            .extend(specific.excluded_card_types.clone());
    }
    if !specific.excluded_colors.is_empty() {
        merged.excluded_colors = merged.excluded_colors.union(specific.excluded_colors);
    }
    if let Some(colors) = specific.colors {
        merged.colors = Some(
            merged
                .colors
                .map_or(colors, |existing| existing.union(colors)),
        );
    }
    if merged.zone.is_none() {
        merged.zone = specific.zone;
    }
    if merged.controller.is_none() {
        merged.controller = specific.controller.clone();
    }
    if merged.owner.is_none() {
        merged.owner = specific.owner.clone();
    }
    merged.other |= specific.other;
    merged.token |= specific.token;
    merged.nontoken |= specific.nontoken;
    merged.tapped |= specific.tapped;
    merged.untapped |= specific.untapped;
    merged.attacking |= specific.attacking;
    merged.nonattacking |= specific.nonattacking;
    merged.blocking |= specific.blocking;
    merged.nonblocking |= specific.nonblocking;
    merged.blocked |= specific.blocked;
    merged.unblocked |= specific.unblocked;
    merged.is_commander |= specific.is_commander;
    merged.noncommander |= specific.noncommander;
    merged.colorless |= specific.colorless;
    merged.multicolored |= specific.multicolored;
    merged.monocolored |= specific.monocolored;

    if let Some(mv) = &specific.mana_value {
        merged.mana_value = Some(mv.clone());
    }
    if let Some(power) = &specific.power {
        merged.power = Some(power.clone());
        merged.power_reference = specific.power_reference;
    }
    if let Some(toughness) = &specific.toughness {
        merged.toughness = Some(toughness.clone());
        merged.toughness_reference = specific.toughness_reference;
    }
    if specific.has_mana_cost {
        merged.has_mana_cost = true;
    }
    if specific.no_x_in_cost {
        merged.no_x_in_cost = true;
    }
    if merged.with_counter.is_none() {
        merged.with_counter = specific.with_counter;
    }
    if merged.without_counter.is_none() {
        merged.without_counter = specific.without_counter;
    }
    if merged.alternative_cast.is_none() {
        merged.alternative_cast = specific.alternative_cast;
    }
    for ability_id in &specific.static_abilities {
        if !merged.static_abilities.contains(ability_id) {
            merged.static_abilities.push(*ability_id);
        }
    }
    for ability_id in &specific.excluded_static_abilities {
        if !merged.excluded_static_abilities.contains(ability_id) {
            merged.excluded_static_abilities.push(*ability_id);
        }
    }
    for marker in &specific.custom_static_markers {
        if !merged
            .custom_static_markers
            .iter()
            .any(|value| value.eq_ignore_ascii_case(marker))
        {
            merged.custom_static_markers.push(marker.clone());
        }
    }
    for marker in &specific.excluded_custom_static_markers {
        if !merged
            .excluded_custom_static_markers
            .iter()
            .any(|value| value.eq_ignore_ascii_case(marker))
        {
            merged.excluded_custom_static_markers.push(marker.clone());
        }
    }

    merged
}

fn parse_monstrosity_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words = words(tokens);
    if words.first().copied() != Some("monstrosity") {
        return Ok(None);
    }

    let amount_tokens = &tokens[1..];
    let (amount, _) = parse_value(amount_tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing monstrosity amount (clause: '{}')",
            words.join(" ")
        ))
    })?;

    Ok(Some(EffectAst::Monstrosity { amount }))
}

fn parse_for_each_counter_removed_sentence(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let words_all = words(tokens);
    if words_all.len() < 6 {
        return Ok(None);
    }
    if !words_all.starts_with(&["for", "each", "counter", "removed", "this", "way"]) {
        return Ok(None);
    }

    let comma_idx = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)));
    let remainder = if let Some(idx) = comma_idx {
        &tokens[idx + 1..]
    } else {
        &tokens[6..]
    };

    let remainder_words = words(remainder);
    if remainder_words.is_empty() {
        return Ok(None);
    }

    let gets_idx = remainder_words
        .iter()
        .position(|word| *word == "gets" || *word == "get");
    let Some(gets_idx) = gets_idx else {
        return Ok(None);
    };

    let subject_tokens = &remainder[..gets_idx];
    let subject = parse_subject(subject_tokens);
    let target = match subject {
        SubjectAst::This => TargetAst::Source(None),
        _ => return Ok(None),
    };

    let after_gets = &remainder[gets_idx + 1..];
    let modifier_token = after_gets.first().and_then(Token::as_word).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing power/toughness modifier (clause: '{}')",
            remainder_words.join(" ")
        ))
    })?;
    let (power, toughness) = parse_pt_modifier(modifier_token)?;

    let duration = if remainder_words.contains(&"until")
        && remainder_words.contains(&"end")
        && remainder_words.contains(&"turn")
    {
        Until::EndOfTurn
    } else {
        Until::EndOfTurn
    };

    Ok(Some(EffectAst::PumpByLastEffect {
        power,
        toughness,
        target,
        duration,
    }))
}

fn is_exile_that_token_at_end_of_combat(tokens: &[Token]) -> bool {
    let words = words(tokens);
    if words.len() != 7 && words.len() != 8 {
        return false;
    }
    if words.first().copied() != Some("exile") || words.get(3).copied() != Some("at") {
        return false;
    }
    if !matches!(words.get(1).copied(), Some("that" | "the" | "those")) {
        return false;
    }
    if !matches!(words.get(2).copied(), Some("token" | "tokens")) {
        return false;
    }
    words[4..] == ["end", "of", "combat"] || words[4..] == ["the", "end", "of", "combat"]
}

fn is_sacrifice_that_token_at_end_of_combat(tokens: &[Token]) -> bool {
    let words = words(tokens);
    if words.len() != 7 && words.len() != 8 {
        return false;
    }
    if words.first().copied() != Some("sacrifice") || words.get(3).copied() != Some("at") {
        return false;
    }
    if !matches!(words.get(1).copied(), Some("that" | "the" | "those")) {
        return false;
    }
    if !matches!(words.get(2).copied(), Some("token" | "tokens")) {
        return false;
    }
    words[4..] == ["end", "of", "combat"] || words[4..] == ["the", "end", "of", "combat"]
}

fn parse_take_extra_turn_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words = words(tokens);
    if words.as_slice() == ["take", "an", "extra", "turn", "after", "this", "one"] {
        return Ok(Some(EffectAst::ExtraTurnAfterTurn {
            player: PlayerAst::You,
        }));
    }
    Ok(None)
}

fn is_ring_tempts_sentence(tokens: &[Token]) -> bool {
    let words = words(tokens);
    words.as_slice() == ["the", "ring", "tempts", "you"]
}

fn find_same_name_reference_span(
    tokens: &[Token],
) -> Result<Option<(usize, usize)>, CardTextError> {
    for idx in 0..tokens.len() {
        if !tokens[idx].is_word("with") {
            continue;
        }
        if idx + 6 < tokens.len()
            && tokens[idx + 1].is_word("the")
            && tokens[idx + 2].is_word("same")
            && tokens[idx + 3].is_word("name")
            && tokens[idx + 4].is_word("as")
            && tokens[idx + 5].is_word("that")
        {
            return Ok(Some((idx, idx + 7)));
        }
        if idx + 5 < tokens.len()
            && tokens[idx + 1].is_word("same")
            && tokens[idx + 2].is_word("name")
            && tokens[idx + 3].is_word("as")
            && tokens[idx + 4].is_word("that")
        {
            return Ok(Some((idx, idx + 6)));
        }
        if idx + 4 < tokens.len()
            && tokens[idx + 1].is_word("the")
            && tokens[idx + 2].is_word("same")
            && tokens[idx + 3].is_word("name")
            && tokens[idx + 4].is_word("as")
        {
            return Err(CardTextError::ParseError(format!(
                "missing 'that <object>' in same-name clause (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        if idx + 3 < tokens.len()
            && tokens[idx + 1].is_word("same")
            && tokens[idx + 2].is_word("name")
            && tokens[idx + 3].is_word("as")
        {
            return Err(CardTextError::ParseError(format!(
                "missing 'that <object>' in same-name clause (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
    }
    Ok(None)
}

fn strip_same_controller_reference(tokens: &[Token]) -> (Vec<Token>, bool) {
    let mut cleaned = Vec::with_capacity(tokens.len());
    let mut idx = 0usize;
    let mut same_controller = false;
    while idx < tokens.len() {
        if idx + 2 < tokens.len()
            && tokens[idx].is_word("that")
            && tokens[idx + 1].is_word("player")
            && (tokens[idx + 2].is_word("control") || tokens[idx + 2].is_word("controls"))
        {
            same_controller = true;
            idx += 3;
            continue;
        }
        if idx + 2 < tokens.len()
            && tokens[idx].is_word("its")
            && tokens[idx + 1].is_word("controller")
            && (tokens[idx + 2].is_word("control") || tokens[idx + 2].is_word("controls"))
        {
            same_controller = true;
            idx += 3;
            continue;
        }
        if idx + 3 < tokens.len()
            && tokens[idx].is_word("that")
            && (tokens[idx + 1].is_word("creature")
                || tokens[idx + 1].is_word("permanent")
                || tokens[idx + 1].is_word("card"))
            && tokens[idx + 2].is_word("controller")
            && (tokens[idx + 3].is_word("control") || tokens[idx + 3].is_word("controls"))
        {
            same_controller = true;
            idx += 4;
            continue;
        }

        cleaned.push(tokens[idx].clone());
        idx += 1;
    }

    (cleaned, same_controller)
}

fn parse_same_name_fanout_filter(tokens: &[Token]) -> Result<Option<ObjectFilter>, CardTextError> {
    let Some((same_start, same_end)) = find_same_name_reference_span(tokens)? else {
        return Ok(None);
    };

    let mut filter_tokens = Vec::with_capacity(tokens.len());
    filter_tokens.extend_from_slice(&tokens[..same_start]);
    filter_tokens.extend_from_slice(&tokens[same_end..]);
    let filter_tokens = trim_commas(&filter_tokens);
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing object phrase in same-name fanout clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let (cleaned_tokens, same_controller) = strip_same_controller_reference(&filter_tokens);
    let cleaned_tokens = trim_commas(&cleaned_tokens);
    if cleaned_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing base object filter in same-name fanout clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let mut filter = parse_object_filter(&cleaned_tokens, false).map_err(|_| {
        CardTextError::ParseError(format!(
            "unsupported same-name fanout filter (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;
    filter.tagged_constraints.push(TaggedObjectConstraint {
        tag: TagKey::from(IT_TAG),
        relation: TaggedOpbjectRelation::SameNameAsTagged,
    });
    filter.tagged_constraints.push(TaggedObjectConstraint {
        tag: TagKey::from(IT_TAG),
        relation: TaggedOpbjectRelation::IsNotTaggedObject,
    });
    if same_controller {
        filter.tagged_constraints.push(TaggedObjectConstraint {
            tag: TagKey::from(IT_TAG),
            relation: TaggedOpbjectRelation::SameControllerAsTagged,
        });
    }
    Ok(Some(filter))
}

fn parse_same_name_target_fanout_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let (tokens, until_source_leaves) = split_until_source_leaves_tail(tokens);
    let words_all = words(tokens);
    let Some(first_word) = words_all.first().copied() else {
        return Ok(None);
    };

    let deal_tokens: Option<&[Token]> = if first_word == "deal" {
        Some(tokens)
    } else if let Some((Verb::Deal, verb_idx)) = find_verb(tokens) {
        let subject_words: Vec<&str> = words(&tokens[..verb_idx])
            .into_iter()
            .filter(|word| !is_article(word))
            .collect();
        if is_source_reference_words(&subject_words) {
            Some(&tokens[verb_idx..])
        } else {
            None
        }
    } else {
        None
    };

    if let Some(deal_tokens) = deal_tokens {
        let deal_words = words(deal_tokens);
        let (amount, used) =
            if deal_words.get(1) == Some(&"that") && deal_words.get(2) == Some(&"much") {
                (Value::EventValue(EventValueSpec::Amount), 2usize)
            } else if let Some((value, used)) = parse_value(&deal_tokens[1..]) {
                (value, used)
            } else {
                return Ok(None);
            };

        let after_amount = &deal_tokens[1 + used..];
        if !after_amount
            .first()
            .is_some_and(|token| token.is_word("damage"))
        {
            return Ok(None);
        }

        let mut target_tokens = &after_amount[1..];
        if target_tokens
            .first()
            .is_some_and(|token| token.is_word("to"))
        {
            target_tokens = &target_tokens[1..];
        }
        if target_tokens.is_empty() {
            return Ok(None);
        }

        let split_idx = (0..target_tokens.len().saturating_sub(2)).find(|idx| {
            target_tokens[*idx].is_word("and")
                && target_tokens[*idx + 1].is_word("each")
                && target_tokens[*idx + 2].is_word("other")
        });
        let Some(split_idx) = split_idx else {
            return Ok(None);
        };
        let first_target_tokens = trim_commas(&target_tokens[..split_idx]);
        if first_target_tokens.is_empty()
            || !first_target_tokens
                .iter()
                .any(|token| token.is_word("target"))
        {
            return Ok(None);
        }

        let second_clause_tokens = target_tokens[split_idx + 3..].to_vec();
        if second_clause_tokens.is_empty() {
            return Ok(None);
        }
        let Some(filter) = parse_same_name_fanout_filter(&second_clause_tokens)? else {
            return Ok(None);
        };
        let first_target = parse_target_phrase(&first_target_tokens)?;
        return Ok(Some(vec![
            EffectAst::DealDamage {
                amount: amount.clone(),
                target: first_target,
            },
            EffectAst::DealDamageEach { amount, filter },
        ]));
    }

    let verb = first_word;
    if verb != "destroy" && verb != "exile" && verb != "return" {
        return Ok(None);
    }

    let and_idx = (0..tokens.len().saturating_sub(2)).find(|idx| {
        tokens[*idx].is_word("and")
            && tokens[*idx + 1].is_word("all")
            && tokens[*idx + 2].is_word("other")
    });
    let Some(and_idx) = and_idx else {
        return Ok(None);
    };
    if and_idx <= 1 {
        return Ok(None);
    }

    let first_target_tokens = trim_commas(&tokens[1..and_idx]);
    if first_target_tokens.is_empty()
        || !first_target_tokens
            .iter()
            .any(|token| token.is_word("target"))
    {
        return Ok(None);
    }

    let second_clause_tokens = if verb == "return" {
        let to_idx = tokens
            .iter()
            .rposition(|token| token.is_word("to"))
            .ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "missing return destination in same-name fanout clause (clause: '{}')",
                    words_all.join(" ")
                ))
            })?;
        if to_idx <= and_idx + 3 {
            return Err(CardTextError::ParseError(format!(
                "missing same-name filter before return destination (clause: '{}')",
                words_all.join(" ")
            )));
        }
        let destination_words = words(&tokens[to_idx + 1..]);
        if !destination_words.contains(&"hand") && !destination_words.contains(&"hands") {
            return Ok(None);
        }
        tokens[and_idx + 3..to_idx].to_vec()
    } else {
        tokens[and_idx + 3..].to_vec()
    };

    if second_clause_tokens.is_empty() {
        return Ok(None);
    }

    let Some(filter) = parse_same_name_fanout_filter(&second_clause_tokens)? else {
        return Ok(None);
    };

    let mut first_target = parse_target_phrase(&first_target_tokens)?;
    if verb == "return"
        && let Some(first_filter) = target_object_filter_mut(&mut first_target)
    {
        if first_filter.zone.is_none() {
            first_filter.zone = filter.zone;
            if first_filter.zone.is_none() && words_all.contains(&"graveyard") {
                first_filter.zone = Some(Zone::Graveyard);
            }
        }
        if first_filter.owner.is_none() {
            first_filter.owner = filter.owner.clone();
            if first_filter.owner.is_none()
                && words_all
                    .windows(2)
                    .any(|window| window == ["your", "graveyard"])
            {
                first_filter.owner = Some(PlayerFilter::You);
            }
        }
    }
    let first_effect = match verb {
        "destroy" => EffectAst::Destroy {
            target: first_target,
        },
        "exile" => {
            if until_source_leaves {
                EffectAst::ExileUntilSourceLeaves {
                    target: first_target,
                    face_down: false,
                }
            } else {
                EffectAst::Exile {
                    target: first_target,
                    face_down: false,
                }
            }
        }
        "return" => EffectAst::ReturnToHand {
            target: first_target,
            random: false,
        },
        _ => unreachable!("verb already filtered"),
    };
    let second_effect = match verb {
        "destroy" => EffectAst::DestroyAll { filter },
        "exile" => {
            if until_source_leaves {
                EffectAst::ExileUntilSourceLeaves {
                    target: TargetAst::Object(filter, None, None),
                    face_down: false,
                }
            } else {
                EffectAst::ExileAll {
                    filter,
                    face_down: false,
                }
            }
        }
        "return" => EffectAst::ReturnAllToHand { filter },
        _ => unreachable!("verb already filtered"),
    };

    Ok(Some(vec![first_effect, second_effect]))
}

fn find_shares_color_reference_span(
    tokens: &[Token],
) -> Result<Option<(usize, usize)>, CardTextError> {
    for idx in 0..tokens.len() {
        if !tokens[idx].is_word("that") {
            continue;
        }
        if idx + 5 < tokens.len()
            && (tokens[idx + 1].is_word("shares") || tokens[idx + 1].is_word("share"))
            && tokens[idx + 2].is_word("a")
            && tokens[idx + 3].is_word("color")
            && tokens[idx + 4].is_word("with")
            && tokens[idx + 5].is_word("it")
        {
            return Ok(Some((idx, idx + 6)));
        }
        if idx + 4 < tokens.len()
            && (tokens[idx + 1].is_word("shares") || tokens[idx + 1].is_word("share"))
            && tokens[idx + 2].is_word("a")
            && tokens[idx + 3].is_word("color")
            && tokens[idx + 4].is_word("with")
        {
            return Err(CardTextError::ParseError(format!(
                "missing 'it' in shares-color clause (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
    }
    Ok(None)
}

fn parse_shared_color_fanout_filter(
    tokens: &[Token],
) -> Result<Option<ObjectFilter>, CardTextError> {
    let Some((share_start, share_end)) = find_shares_color_reference_span(tokens)? else {
        return Ok(None);
    };

    let mut filter_tokens = Vec::with_capacity(tokens.len());
    filter_tokens.extend_from_slice(&tokens[..share_start]);
    filter_tokens.extend_from_slice(&tokens[share_end..]);
    let filter_tokens = trim_commas(&filter_tokens);
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing object phrase in shared-color fanout clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let mut filter = parse_object_filter(&filter_tokens, false).map_err(|_| {
        CardTextError::ParseError(format!(
            "unsupported shared-color fanout filter (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;
    filter.tagged_constraints.push(TaggedObjectConstraint {
        tag: TagKey::from(IT_TAG),
        relation: TaggedOpbjectRelation::SharesColorWithTagged,
    });
    filter.tagged_constraints.push(TaggedObjectConstraint {
        tag: TagKey::from(IT_TAG),
        relation: TaggedOpbjectRelation::IsNotTaggedObject,
    });
    Ok(Some(filter))
}

fn parse_shared_color_target_fanout_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words_all = words(tokens);
    let Some((verb, verb_idx)) = find_verb(tokens) else {
        return Ok(None);
    };
    let Some(verb_token_idx) = token_index_for_word_index(tokens, verb_idx) else {
        return Ok(None);
    };

    let find_and_each_other = |scope: &[Token]| {
        (0..scope.len().saturating_sub(2)).find(|idx| {
            scope[*idx].is_word("and")
                && scope[*idx + 1].is_word("each")
                && scope[*idx + 2].is_word("other")
        })
    };

    if matches!(verb, Verb::Destroy | Verb::Exile | Verb::Untap) {
        let after_verb = &tokens[verb_token_idx + 1..];
        let Some(split_idx) = find_and_each_other(after_verb) else {
            return Ok(None);
        };
        let first_target_tokens = trim_commas(&after_verb[..split_idx]);
        if first_target_tokens.is_empty()
            || !first_target_tokens
                .iter()
                .any(|token| token.is_word("target"))
        {
            return Ok(None);
        }
        let second_clause_tokens = after_verb[split_idx + 3..].to_vec();
        if second_clause_tokens.is_empty() {
            return Ok(None);
        }
        let Some(filter) = parse_shared_color_fanout_filter(&second_clause_tokens)? else {
            return Ok(None);
        };
        let first_target = parse_target_phrase(&first_target_tokens)?;
        let mut effects = Vec::with_capacity(2);
        match verb {
            Verb::Destroy => {
                effects.push(EffectAst::Destroy {
                    target: first_target,
                });
                effects.push(EffectAst::DestroyAll { filter });
            }
            Verb::Exile => {
                effects.push(EffectAst::Exile {
                    target: first_target,
                    face_down: false,
                });
                effects.push(EffectAst::ExileAll {
                    filter,
                    face_down: false,
                });
            }
            Verb::Untap => {
                effects.push(EffectAst::Untap {
                    target: first_target,
                });
                effects.push(EffectAst::UntapAll { filter });
            }
            _ => return Ok(None),
        }
        return Ok(Some(effects));
    }

    if verb == Verb::Deal {
        let after_verb = &tokens[verb_token_idx + 1..];
        let after_words = words(after_verb);
        let (amount, used) = if after_words.starts_with(&["that", "much"]) {
            (Value::EventValue(EventValueSpec::Amount), 2usize)
        } else if let Some((value, used)) = parse_value(after_verb) {
            (value, used)
        } else {
            return Ok(None);
        };

        let after_amount = &after_verb[used..];
        if !after_amount
            .first()
            .is_some_and(|token| token.is_word("damage"))
        {
            return Ok(None);
        }
        let mut target_tokens = &after_amount[1..];
        if target_tokens
            .first()
            .is_some_and(|token| token.is_word("to"))
        {
            target_tokens = &target_tokens[1..];
        }
        if target_tokens.is_empty() {
            return Ok(None);
        }
        let Some(split_idx) = find_and_each_other(target_tokens) else {
            return Ok(None);
        };
        let first_target_tokens = trim_commas(&target_tokens[..split_idx]);
        if first_target_tokens.is_empty()
            || !first_target_tokens
                .iter()
                .any(|token| token.is_word("target"))
        {
            return Ok(None);
        }
        let second_clause_tokens = target_tokens[split_idx + 3..].to_vec();
        if second_clause_tokens.is_empty() {
            return Ok(None);
        }
        let Some(filter) = parse_shared_color_fanout_filter(&second_clause_tokens)? else {
            return Ok(None);
        };
        let first_target = parse_target_phrase(&first_target_tokens)?;
        return Ok(Some(vec![
            EffectAst::DealDamage {
                amount: amount.clone(),
                target: first_target,
            },
            EffectAst::DealDamageEach { amount, filter },
        ]));
    }

    if words_all.first().copied() == Some("prevent") {
        let mut idx = verb_token_idx + 1;
        if tokens.get(idx).is_some_and(|token| token.is_word("the")) {
            idx += 1;
        }
        if !tokens.get(idx).is_some_and(|token| token.is_word("next")) {
            return Ok(None);
        }
        idx += 1;
        let amount_token = tokens.get(idx).cloned().ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing prevent damage amount (clause: '{}')",
                words_all.join(" ")
            ))
        })?;
        let Some((amount, _)) = parse_value(&[amount_token]) else {
            return Ok(None);
        };
        idx += 1;
        if !tokens.get(idx).is_some_and(|token| token.is_word("damage")) {
            return Ok(None);
        }
        idx += 1;
        if tokens.get(idx..idx + 4).is_none_or(|window| {
            !window[0].is_word("that")
                || !window[1].is_word("would")
                || !window[2].is_word("be")
                || !window[3].is_word("dealt")
        }) {
            return Ok(None);
        }
        idx += 4;
        if !tokens.get(idx).is_some_and(|token| token.is_word("to")) {
            return Ok(None);
        }
        idx += 1;

        let this_turn_rel = words(&tokens[idx..])
            .windows(2)
            .position(|window| window == ["this", "turn"]);
        let Some(this_turn_rel) = this_turn_rel else {
            return Ok(None);
        };
        let this_turn_abs = idx + this_turn_rel;
        if this_turn_abs + 2 != tokens.len() {
            return Ok(None);
        }

        let scope_tokens = &tokens[idx..this_turn_abs];
        let Some(split_idx) = find_and_each_other(scope_tokens) else {
            return Ok(None);
        };

        let first_target_tokens = trim_commas(&scope_tokens[..split_idx]);
        if first_target_tokens.is_empty()
            || !first_target_tokens
                .iter()
                .any(|token| token.is_word("target"))
        {
            return Ok(None);
        }
        let second_clause_tokens = scope_tokens[split_idx + 3..].to_vec();
        let Some(filter) = parse_shared_color_fanout_filter(&second_clause_tokens)? else {
            return Ok(None);
        };
        let first_target = parse_target_phrase(&first_target_tokens)?;

        return Ok(Some(vec![
            EffectAst::PreventDamage {
                amount: amount.clone(),
                target: first_target,
                duration: Until::EndOfTurn,
            },
            EffectAst::PreventDamageEach {
                amount,
                filter,
                duration: Until::EndOfTurn,
            },
        ]));
    }

    if matches!(verb, Verb::Get | Verb::Gain) {
        if verb_idx == 0 || verb_token_idx + 1 >= tokens.len() {
            return Ok(None);
        }

        let subject_tokens = &tokens[..verb_token_idx];
        let Some(and_idx) = find_and_each_other(subject_tokens) else {
            return Ok(None);
        };
        if and_idx == 0 {
            return Ok(None);
        }

        let first_target_tokens = trim_commas(&subject_tokens[..and_idx]);
        if first_target_tokens.is_empty()
            || !first_target_tokens
                .iter()
                .any(|token| token.is_word("target"))
        {
            return Ok(None);
        }
        let second_clause_tokens = trim_commas(&subject_tokens[and_idx + 3..]);
        if second_clause_tokens.is_empty() {
            return Ok(None);
        }
        let Some(filter) = parse_shared_color_fanout_filter(&second_clause_tokens)? else {
            return Ok(None);
        };
        let first_target = parse_target_phrase(&first_target_tokens)?;

        if verb == Verb::Get {
            let modifier_tokens = &tokens[verb_token_idx + 1..];
            let modifier_word = modifier_tokens
                .first()
                .and_then(Token::as_word)
                .ok_or_else(|| {
                    CardTextError::ParseError(format!(
                        "missing modifier in shared-color gets clause (clause: '{}')",
                        words_all.join(" ")
                    ))
                })?;
            let (power, toughness) = parse_pt_modifier(modifier_word).map_err(|_| {
                CardTextError::ParseError(format!(
                    "invalid power/toughness modifier in shared-color gets clause (clause: '{}')",
                    words_all.join(" ")
                ))
            })?;

            return Ok(Some(vec![
                EffectAst::Pump {
                    power: Value::Fixed(power),
                    toughness: Value::Fixed(toughness),
                    target: first_target,
                    duration: Until::EndOfTurn,
                    condition: None,
                },
                EffectAst::PumpAll {
                    filter,
                    power: Value::Fixed(power),
                    toughness: Value::Fixed(toughness),
                    duration: Until::EndOfTurn,
                },
            ]));
        }

        let mut first_clause = first_target_tokens.clone();
        first_clause.extend_from_slice(&tokens[verb_token_idx..]);
        let Some(first_effect) = parse_simple_gain_ability_clause(&first_clause)? else {
            return Ok(None);
        };
        if let EffectAst::GrantAbilitiesToTarget {
            abilities, duration, ..
        } = first_effect
        {
            return Ok(Some(vec![
                EffectAst::GrantAbilitiesToTarget {
                    target: first_target,
                    abilities: abilities.clone(),
                    duration: duration.clone(),
                },
                EffectAst::GrantAbilitiesAll {
                    filter,
                    abilities,
                    duration,
                },
            ]));
        }
    }

    Ok(None)
}

fn parse_same_name_gets_fanout_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some((verb, verb_idx)) = find_verb(tokens) else {
        return Ok(None);
    };
    if verb != Verb::Get || verb_idx == 0 || verb_idx + 1 >= tokens.len() {
        return Ok(None);
    }

    let subject_tokens = &tokens[..verb_idx];
    let and_idx = (0..subject_tokens.len().saturating_sub(2)).find(|idx| {
        subject_tokens[*idx].is_word("and")
            && subject_tokens[*idx + 1].is_word("all")
            && subject_tokens[*idx + 2].is_word("other")
    });
    let Some(and_idx) = and_idx else {
        return Ok(None);
    };
    if and_idx == 0 {
        return Ok(None);
    }

    let first_target_tokens = trim_commas(&subject_tokens[..and_idx]);
    if first_target_tokens.is_empty()
        || !first_target_tokens
            .iter()
            .any(|token| token.is_word("target"))
    {
        return Ok(None);
    }
    let second_clause_tokens = trim_commas(&subject_tokens[and_idx + 3..]);
    if second_clause_tokens.is_empty() {
        return Ok(None);
    }
    let Some(filter) = parse_same_name_fanout_filter(&second_clause_tokens)? else {
        return Ok(None);
    };

    let modifier_tokens = &tokens[verb_idx + 1..];
    let modifier_word = modifier_tokens
        .first()
        .and_then(Token::as_word)
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing modifier in same-name gets clause (clause: '{}')",
                words(tokens).join(" ")
            ))
        })?;
    let (power, toughness) = parse_pt_modifier(modifier_word).map_err(|_| {
        CardTextError::ParseError(format!(
            "invalid power/toughness modifier in same-name gets clause (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;
    let modifier_words = words(modifier_tokens);
    let duration = if modifier_words.contains(&"until")
        && modifier_words.contains(&"end")
        && modifier_words.contains(&"turn")
    {
        Until::EndOfTurn
    } else {
        Until::EndOfTurn
    };

    let target = parse_target_phrase(&first_target_tokens)?;
    Ok(Some(vec![
        EffectAst::Pump {
            power: Value::Fixed(power),
            toughness: Value::Fixed(toughness),
            target,
            duration: duration.clone(),
            condition: None,
        },
        EffectAst::PumpAll {
            filter,
            power: Value::Fixed(power),
            toughness: Value::Fixed(toughness),
            duration,
        },
    ]))
}

fn parse_destroy_or_exile_all_split_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words = words(tokens);
    if words.len() < 4 {
        return Ok(None);
    }

    let verb = if words[0] == "destroy" {
        Some(Verb::Destroy)
    } else if words[0] == "exile" {
        Some(Verb::Exile)
    } else {
        None
    };
    let Some(verb) = verb else {
        return Ok(None);
    };
    if words[1] != "all" || !words.contains(&"and") || words.contains(&"except") {
        return Ok(None);
    }

    let mut raw_segments = Vec::new();
    let mut current = Vec::new();
    for token in &tokens[2..] {
        if token.is_word("and") || matches!(token, Token::Comma(_)) {
            if !current.is_empty() {
                raw_segments.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(token.clone());
    }
    if !current.is_empty() {
        raw_segments.push(current);
    }

    let mut effects = Vec::new();
    for mut segment in raw_segments {
        if segment.is_empty() {
            continue;
        }
        if segment.first().is_some_and(|token| token.is_word("all")) {
            segment.remove(0);
        }
        if segment.is_empty() {
            continue;
        }
        let filter = parse_object_filter(&segment, false).map_err(|_| {
            CardTextError::ParseError(format!(
                "unsupported filter in split all clause (clause: '{}')",
                words.join(" ")
            ))
        })?;
        let effect = match verb {
            Verb::Destroy => EffectAst::DestroyAll { filter },
            Verb::Exile => EffectAst::ExileAll {
                filter,
                face_down: false,
            },
            _ => {
                return Err(CardTextError::ParseError(
                    "unsupported split all clause verb".to_string(),
                ));
            }
        };
        effects.push(effect);
    }

    if effects.len() >= 2 {
        return Ok(Some(effects));
    }
    Ok(None)
}

fn parse_exile_then_return_same_object_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    fn target_references_it_tag(target: &TargetAst) -> bool {
        match target {
            TargetAst::Tagged(tag, _) => tag.as_str() == IT_TAG,
            TargetAst::Object(filter, _, _) => filter.tagged_constraints.iter().any(|constraint| {
                constraint.tag.as_str() == IT_TAG
                    && matches!(constraint.relation, TaggedOpbjectRelation::IsTaggedObject)
            }),
            _ => false,
        }
    }

    let mut clause_tokens = tokens;
    if clause_tokens
        .first()
        .is_some_and(|token| token.is_word("you"))
        && clause_tokens
            .get(1)
            .is_some_and(|token| token.is_word("exile"))
    {
        clause_tokens = &clause_tokens[1..];
    }

    let words_all = words(clause_tokens);
    if words_all.first().copied() != Some("exile")
        || !words_all.contains(&"then")
        || !words_all.contains(&"return")
    {
        return Ok(None);
    }

    let split_idx = (0..clause_tokens.len().saturating_sub(2)).find(|idx| {
        matches!(clause_tokens[*idx], Token::Comma(_))
            && clause_tokens[*idx + 1].is_word("then")
            && clause_tokens[*idx + 2].is_word("return")
    });
    let Some(split_idx) = split_idx else {
        return Ok(None);
    };

    let first_clause = &clause_tokens[..split_idx];
    let second_clause = &clause_tokens[split_idx + 2..];
    if first_clause.is_empty() || second_clause.is_empty() {
        return Ok(None);
    }

    let mut first_effects = parse_effect_chain_inner(first_clause)?;
    if !first_effects
        .iter()
        .any(|effect| matches!(effect, EffectAst::Exile { .. }))
    {
        return Ok(None);
    }

    // Preserve return follow-up clauses (for example "with a +1/+1 counter on it")
    // while still rewriting the "it" return target to the tagged exiled object.
    let mut second_effects = parse_effect_chain_inner(second_clause)?;
    let mut rewrote_return = false;
    for effect in &mut second_effects {
        match effect {
            EffectAst::ReturnToBattlefield {
                target,
                tapped: _,
                controller: _,
            } if target_references_it_tag(target) => {
                *target = TargetAst::Tagged(TagKey::from("exiled_0"), None);
                rewrote_return = true;
            }
            EffectAst::ReturnToHand { target, random: _ } if target_references_it_tag(target) => {
                *target = TargetAst::Tagged(TagKey::from("exiled_0"), None);
                rewrote_return = true;
            }
            _ => {}
        }
    }
    if !rewrote_return {
        return Ok(None);
    }

    first_effects.extend(second_effects);
    Ok(Some(first_effects))
}

fn parse_exile_up_to_one_each_target_type_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words = words(tokens);
    if words.len() < 6 || words[0] != "exile" {
        return Ok(None);
    }
    if !words.starts_with(&["exile", "up", "to", "one", "target"]) {
        return Ok(None);
    }
    // This primitive is for repeated clauses like:
    // "Exile up to one target artifact, up to one target creature, ..."
    // Not for a single disjunctive target like:
    // "Exile up to one target artifact, creature, or enchantment ..."
    let target_positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(idx, token)| token.is_word("target").then_some(idx))
        .collect();
    if target_positions.len() < 2 {
        return Ok(None);
    }
    for pos in target_positions.iter().skip(1) {
        if *pos < 3
            || !tokens[*pos - 3].is_word("up")
            || !tokens[*pos - 2].is_word("to")
            || !tokens[*pos - 1].is_word("one")
        {
            return Ok(None);
        }
    }

    let mut raw_segments: Vec<Vec<Token>> = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    for token in &tokens[1..] {
        if matches!(token, Token::Comma(_)) || token.is_word("and") || token.is_word("or") {
            if !current.is_empty() {
                raw_segments.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(token.clone());
    }
    if !current.is_empty() {
        raw_segments.push(current);
    }

    let mut filters = Vec::new();
    for segment in raw_segments {
        let mut slice: &[Token] = &segment;
        if slice.len() >= 3
            && slice[0].is_word("up")
            && slice[1].is_word("to")
            && slice[2].is_word("one")
        {
            slice = &slice[3..];
        }
        if slice.first().is_some_and(|token| token.is_word("target")) {
            slice = &slice[1..];
        }
        if slice.is_empty() {
            continue;
        }

        let mut filter = parse_object_filter(slice, false).map_err(|_| {
            CardTextError::ParseError(format!(
                "unsupported filter in 'exile up to one each target type' clause (clause: '{}')",
                words.join(" ")
            ))
        })?;
        if filter.controller.is_none() {
            // Keep this unrestricted to avoid implicit "you control" defaulting in ChooseObjects compilation.
            filter.controller = Some(PlayerFilter::Any);
        }
        filters.push(filter);
    }

    if filters.len() < 2 {
        return Ok(None);
    }

    let tag = TagKey::from("exiled_0");
    let mut effects: Vec<EffectAst> = filters
        .into_iter()
        .map(|filter| EffectAst::ChooseObjects {
            filter,
            count: ChoiceCount::up_to(1),
            player: PlayerAst::You,
            tag: tag.clone(),
        })
        .collect();
    effects.push(EffectAst::Exile {
        target: TargetAst::Tagged(tag, None),
        face_down: false,
    });

    Ok(Some(effects))
}

fn parse_look_at_hand_sentence(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words = words(tokens);
    if words.as_slice() == ["look", "at", "target", "players", "hand"]
        || words.as_slice() == ["look", "at", "target", "player", "hand"]
    {
        let target = TargetAst::Player(PlayerFilter::target_player(), Some(TextSpan::synthetic()));
        return Ok(Some(vec![EffectAst::LookAtHand { target }]));
    }
    if words.as_slice() == ["look", "at", "target", "opponent", "hand"]
        || words.as_slice() == ["look", "at", "target", "opponents", "hand"]
    {
        let target =
            TargetAst::Player(PlayerFilter::target_opponent(), Some(TextSpan::synthetic()));
        return Ok(Some(vec![EffectAst::LookAtHand { target }]));
    }
    Ok(None)
}

fn parse_look_at_top_then_exile_one_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    let starts_with_look_top = clause_words.starts_with(&["look", "at", "the", "top"])
        || clause_words.starts_with(&["look", "at", "top"]);
    if !starts_with_look_top {
        return Ok(None);
    }

    let Some(top_idx) = tokens.iter().position(|token| token.is_word("top")) else {
        return Ok(None);
    };
    let Some((count, used_count)) = parse_number(&tokens[top_idx + 1..]) else {
        return Ok(None);
    };
    let mut idx = top_idx + 1 + used_count;
    if tokens
        .get(idx)
        .is_some_and(|token| token.is_word("card") || token.is_word("cards"))
    {
        idx += 1;
    }
    if !tokens.get(idx).is_some_and(|token| token.is_word("of")) {
        return Ok(None);
    }
    idx += 1;

    let Some(library_idx) = tokens[idx..]
        .iter()
        .position(|token| token.is_word("library"))
        .map(|offset| idx + offset)
    else {
        return Ok(None);
    };
    let owner_tokens = trim_commas(&tokens[idx..library_idx]);
    if owner_tokens.is_empty() {
        return Ok(None);
    }
    let player = match parse_subject(&owner_tokens) {
        SubjectAst::Player(player) => player,
        _ => return Ok(None),
    };

    let mut tail_tokens = trim_commas(&tokens[library_idx + 1..]).to_vec();
    while tail_tokens
        .first()
        .is_some_and(|token| token.is_word("then") || token.is_word("and"))
    {
        tail_tokens.remove(0);
    }
    let tail_words = words(&tail_tokens);
    let looks_like_exile_one_of_looked = tail_words.starts_with(&["exile", "one", "of", "them"])
        || tail_words.starts_with(&["exile", "one", "of", "those"])
        || tail_words.starts_with(&["exile", "one", "of", "those", "cards"]);
    if !looks_like_exile_one_of_looked {
        return Ok(None);
    }

    let looked_tag = TagKey::from("looked_0");
    let chosen_tag = TagKey::from("chosen_0");
    let mut looked_filter = ObjectFilter::tagged(looked_tag.clone());
    looked_filter.zone = Some(Zone::Library);

    Ok(Some(vec![
        EffectAst::LookAtTopCards {
            player,
            count: Value::Fixed(count as i32),
            tag: looked_tag,
        },
        EffectAst::ChooseObjects {
            filter: looked_filter,
            count: ChoiceCount::exactly(1),
            player: PlayerAst::You,
            tag: chosen_tag.clone(),
        },
        EffectAst::Exile {
            target: TargetAst::Tagged(chosen_tag, None),
            face_down: false,
        },
    ]))
}

fn parse_gain_life_equal_to_age_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    // Legacy fallback previously returned a hardcoded 0-life effect for age-counter clauses.
    // Let generic life parsing handle these so counter-scaled amounts compile correctly.
    let _ = tokens;
    Ok(None)
}

fn parse_you_and_each_opponent_voted_with_you_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words = words(tokens);
    let pattern = [
        "you", "and", "each", "opponent", "who", "voted", "for", "a", "choice", "you", "voted",
        "for", "may", "scry",
    ];

    if words.len() < pattern.len() {
        return Ok(None);
    }

    if !words.starts_with(&pattern) {
        return Ok(None);
    }

    let scry_index = pattern.len() - 1;
    let value_tokens = &tokens[(scry_index + 1)..];
    let Some((count, _)) = parse_value(value_tokens) else {
        return Err(CardTextError::ParseError(format!(
            "missing scry count in vote-with-you clause (clause: '{}')",
            words.join(" ")
        )));
    };

    let you_effect = EffectAst::May {
        effects: vec![EffectAst::Scry {
            count: count.clone(),
            player: PlayerAst::You,
        }],
    };

    let opponent_effect = EffectAst::ForEachTaggedPlayer {
        tag: TagKey::from("voted_with_you"),
        effects: vec![EffectAst::May {
            effects: vec![EffectAst::Scry {
                count,
                player: PlayerAst::Implicit,
            }],
        }],
    };

    Ok(Some(vec![you_effect, opponent_effect]))
}

fn parse_gain_life_equal_to_power_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words = words(tokens);
    let Some(gain_idx) = words
        .iter()
        .position(|word| *word == "gain" || *word == "gains")
    else {
        return Ok(None);
    };

    if words.get(gain_idx + 1) != Some(&"life")
        || words.get(gain_idx + 2) != Some(&"equal")
        || words.get(gain_idx + 3) != Some(&"to")
    {
        return Ok(None);
    }

    let tail = &words[gain_idx + 4..];
    let has_its_power = tail.windows(2).any(|pair| pair == ["its", "power"]);
    if !has_its_power {
        return Ok(None);
    }

    let subject = if gain_idx > 0 {
        Some(parse_subject(&tokens[..gain_idx]))
    } else {
        None
    };
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    let amount = Value::PowerOf(Box::new(ChooseSpec::Tagged(TagKey::from(IT_TAG))));
    Ok(Some(vec![EffectAst::GainLife { amount, player }]))
}

fn parse_prevent_damage_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words = words(tokens);
    let prefix = ["prevent", "all", "combat", "damage"];
    if !words.starts_with(&prefix) {
        return Ok(None);
    }

    let this_turn_positions: Vec<usize> = words
        .windows(2)
        .enumerate()
        .filter_map(|(idx, pair)| (pair == ["this", "turn"]).then_some(idx))
        .collect();
    if this_turn_positions.len() != 1 {
        return Err(CardTextError::ParseError(format!(
            "unsupported prevent-all-combat-damage duration (clause: '{}')",
            words.join(" ")
        )));
    }
    let this_turn_idx = this_turn_positions[0];
    if this_turn_idx < prefix.len() {
        return Err(CardTextError::ParseError(format!(
            "unsupported prevent-all-combat-damage duration (clause: '{}')",
            words.join(" ")
        )));
    }

    let mut core_words = Vec::with_capacity(words.len() - prefix.len() - 2);
    core_words.extend_from_slice(&words[prefix.len()..this_turn_idx]);
    core_words.extend_from_slice(&words[this_turn_idx + 2..]);
    let mut core_tokens = Vec::with_capacity(tokens.len() - prefix.len() - 2);
    core_tokens.extend_from_slice(&tokens[prefix.len()..this_turn_idx]);
    core_tokens.extend_from_slice(&tokens[this_turn_idx + 2..]);
    let core_words = core_words;
    let core_tokens = core_tokens;

    if core_words == ["that", "would", "be", "dealt"] {
        return Ok(Some(EffectAst::PreventAllCombatDamage {
            duration: Until::EndOfTurn,
        }));
    }

    if core_words.starts_with(&["that", "would", "be", "dealt", "by"]) {
        let source_tokens = &core_tokens[5..];
        let source = parse_prevent_damage_source_target(source_tokens, &words)?;
        return Ok(Some(EffectAst::PreventAllCombatDamageFromSource {
            duration: Until::EndOfTurn,
            source,
        }));
    }

    if core_words.starts_with(&["that", "would", "be", "dealt", "to"]) {
        return parse_prevent_damage_target_scope(&core_tokens[5..], &words);
    }

    if let Some(would_idx) = core_words.iter().position(|word| *word == "would")
        && core_words.get(would_idx + 1) == Some(&"deal")
    {
        let source_tokens = &core_tokens[..would_idx];
        let source = parse_prevent_damage_source_target(source_tokens, &words)?;
        return Ok(Some(EffectAst::PreventAllCombatDamageFromSource {
            duration: Until::EndOfTurn,
            source,
        }));
    }

    Err(CardTextError::ParseError(format!(
        "unsupported prevent-all-combat-damage clause tail (clause: '{}')",
        words.join(" ")
    )))
}

fn parse_prevent_damage_source_target(
    tokens: &[Token],
    clause_words: &[&str],
) -> Result<TargetAst, CardTextError> {
    if tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing prevent-all source target (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let source_words: Vec<&str> = words(tokens)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    let is_explicit_reference = source_words.contains(&"target")
        || source_words
            .first()
            .is_some_and(|word| matches!(*word, "this" | "that" | "it"));
    if !is_explicit_reference {
        return Err(CardTextError::ParseError(format!(
            "unsupported prevent-all source target '{}'",
            source_words.join(" ")
        )));
    }

    let source = parse_target_phrase(tokens)?;
    match source {
        TargetAst::Source(_) | TargetAst::Object(_, _, _) | TargetAst::Tagged(_, _) => Ok(source),
        _ => Err(CardTextError::ParseError(format!(
            "unsupported prevent-all source target '{}'",
            words(tokens).join(" ")
        ))),
    }
}

fn parse_prevent_damage_target_scope(
    tokens: &[Token],
    clause_words: &[&str],
) -> Result<Option<EffectAst>, CardTextError> {
    if tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing prevent-all target scope (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let target_words: Vec<&str> = words(tokens)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    if target_words.as_slice() == ["player"] || target_words.as_slice() == ["players"] {
        return Ok(Some(EffectAst::PreventAllCombatDamageToPlayers {
            duration: Until::EndOfTurn,
        }));
    }
    if target_words.as_slice() == ["you"] {
        return Ok(Some(EffectAst::PreventAllCombatDamageToYou {
            duration: Until::EndOfTurn,
        }));
    }

    Err(CardTextError::ParseError(format!(
        "unsupported prevent-all target scope '{}'",
        words(tokens).join(" ")
    )))
}

fn parse_gain_x_plus_life_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words = words(tokens);
    let Some(gain_idx) = words
        .iter()
        .position(|word| *word == "gain" || *word == "gains")
    else {
        return Ok(None);
    };

    if words.len() <= gain_idx + 4 {
        return Ok(None);
    }

    if words[gain_idx + 1] != "x" || words[gain_idx + 2] != "plus" {
        return Ok(None);
    }

    let (bonus, number_used) = parse_number(&tokens[gain_idx + 3..]).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing life gain amount (clause: '{}')",
            words.join(" ")
        ))
    })?;
    let life_idx = gain_idx + 3 + number_used;
    if !tokens
        .get(life_idx)
        .is_some_and(|token| token.is_word("life"))
    {
        return Err(CardTextError::ParseError(format!(
            "missing life keyword in gain-x-plus-life clause (clause: '{}')",
            words.join(" ")
        )));
    }

    let subject_tokens = &tokens[..gain_idx];
    let player = match parse_subject(subject_tokens) {
        SubjectAst::Player(player) => player,
        _ => PlayerAst::Implicit,
    };

    let trailing_tokens = trim_commas(&tokens[life_idx + 1..]);
    let x_value = if trailing_tokens.is_empty() {
        Value::X
    } else if let Some(where_x) = parse_where_x_value_clause(&trailing_tokens) {
        where_x
    } else {
        return Err(CardTextError::ParseError(format!(
            "unsupported gain-x-plus-life trailing clause (clause: '{}')",
            words.join(" ")
        )));
    };
    let amount = Value::Add(Box::new(x_value), Box::new(Value::Fixed(bonus as i32)));
    let effects = vec![EffectAst::GainLife { amount, player }];

    Ok(Some(effects))
}

fn parse_simple_ability_duration(words_after_verb: &[&str]) -> Option<(usize, usize, Until)> {
    if let Some(idx) = words_after_verb
        .windows(4)
        .position(|window| window == ["until", "end", "of", "turn"])
    {
        return Some((idx, 4, Until::EndOfTurn));
    }
    if let Some(idx) = words_after_verb.windows(4).position(|window| {
        window == ["until", "your", "next", "turn"] || window == ["until", "your", "next", "upkeep"]
    }) {
        return Some((idx, 4, Until::YourNextTurn));
    }
    if let Some(idx) = words_after_verb.windows(5).position(|window| {
        window == ["until", "your", "next", "untap", "step"]
            || window == ["during", "your", "next", "untap", "step"]
    }) {
        return Some((idx, 5, Until::YourNextTurn));
    }
    if let Some(idx) = words_after_verb
        .windows(6)
        .position(|window| window == ["for", "as", "long", "as", "you", "control"])
    {
        return Some((
            idx,
            words_after_verb.len().saturating_sub(idx),
            Until::YouStopControllingThis,
        ));
    }
    None
}

fn parse_simple_gain_ability_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    parse_simple_ability_modifier_clause(tokens, false)
}

fn parse_simple_lose_ability_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    parse_simple_ability_modifier_clause(tokens, true)
}

fn parse_simple_ability_modifier_clause(
    tokens: &[Token],
    losing: bool,
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let verb_idx = clause_words.iter().position(|word| {
        if losing {
            matches!(*word, "lose" | "loses")
        } else {
            matches!(*word, "gain" | "gains")
        }
    });
    let Some(verb_idx) = verb_idx else {
        return Ok(None);
    };
    if verb_idx == 0 {
        return Ok(None);
    }
    let Some(verb_token_idx) = token_index_for_word_index(tokens, verb_idx) else {
        return Ok(None);
    };

    if !losing && matches!(clause_words[verb_idx], "gain" | "gains") {
        let starts_with_life = clause_words
            .get(verb_idx + 1)
            .is_some_and(|word| *word == "life");
        let starts_with_control = clause_words
            .get(verb_idx + 1)
            .is_some_and(|word| *word == "control");
        if starts_with_life || starts_with_control {
            return Ok(None);
        }
    }

    let subject_tokens = trim_commas(&tokens[..verb_token_idx]);
    if subject_tokens.is_empty() {
        return Ok(None);
    }

    if !losing
        && let Some((subject_verb, _)) = find_verb(&subject_tokens)
        && subject_verb != Verb::Get
    {
        return Ok(None);
    }

    let words_after_verb = &clause_words[verb_idx + 1..];
    if words_after_verb.is_empty() {
        return Ok(None);
    }

    let duration_phrase = parse_simple_ability_duration(words_after_verb);
    let duration = duration_phrase
        .as_ref()
        .map(|(_, _, duration)| duration.clone())
        .unwrap_or(Until::Forever);

    let ability_end_word_idx = duration_phrase
        .as_ref()
        .map(|(start, _, _)| verb_idx + 1 + *start)
        .unwrap_or(clause_words.len());
    let ability_end_token_idx =
        token_index_for_word_index(tokens, ability_end_word_idx).unwrap_or(tokens.len());
    let ability_tokens = trim_commas(&tokens[verb_token_idx + 1..ability_end_token_idx]);
    if ability_tokens.is_empty() {
        return Ok(None);
    }

    let mut abilities = if let Some(actions) = parse_ability_line(&ability_tokens) {
        reject_unimplemented_keyword_actions(&actions, &clause_words.join(" "))?;
        actions
            .into_iter()
            .filter_map(keyword_action_to_static_ability)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if abilities.is_empty()
        && let Some(granted) =
            parse_granted_activated_or_triggered_ability_for_gain(&ability_tokens, &clause_words)?
    {
        abilities.push(granted);
    }
    if abilities.is_empty() {
        return Ok(None);
    }

    if let Some((start, len, _)) = duration_phrase {
        let tail_word_idx = verb_idx + 1 + start + len;
        if let Some(tail_token_idx) = token_index_for_word_index(tokens, tail_word_idx) {
            let trailing = trim_commas(&tokens[tail_token_idx..]);
            if !trailing.is_empty() {
                return Ok(None);
            }
        }
    }

    let subject_words = words(&subject_tokens);
    let is_pronoun_subject = matches!(subject_words.as_slice(), ["it"] | ["they"] | ["them"]);
    if is_pronoun_subject {
        let target = TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(&subject_tokens));
        if losing {
            return Ok(Some(EffectAst::RemoveAbilitiesFromTarget {
                target,
                abilities,
                duration,
            }));
        }
        return Ok(Some(EffectAst::GrantAbilitiesToTarget {
            target,
            abilities,
            duration,
        }));
    }

    let is_demonstrative_subject = subject_words
        .first()
        .is_some_and(|word| *word == "that" || *word == "those");
    if is_demonstrative_subject || subject_words.contains(&"target") {
        let target = parse_target_phrase(&subject_tokens)?;
        if losing {
            return Ok(Some(EffectAst::RemoveAbilitiesFromTarget {
                target,
                abilities,
                duration,
            }));
        }
        return Ok(Some(EffectAst::GrantAbilitiesToTarget {
            target,
            abilities,
            duration,
        }));
    }

    let filter = parse_object_filter(&subject_tokens, false).map_err(|_| {
        CardTextError::ParseError(format!(
            "unsupported subject in {}-ability clause (clause: '{}')",
            if losing { "lose" } else { "gain" },
            clause_words.join(" ")
        ))
    })?;
    if losing {
        return Ok(Some(EffectAst::RemoveAbilitiesAll {
            filter,
            abilities,
            duration,
        }));
    }
    Ok(Some(EffectAst::GrantAbilitiesAll {
        filter,
        abilities,
        duration,
    }))
}

fn parse_gain_ability_sentence(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let word_list = words(tokens);
    let looks_like_can_attack_no_defender = word_list
        .windows(2)
        .any(|window| window == ["can", "attack"])
        && word_list
            .windows(2)
            .any(|window| window == ["as", "though"])
        && word_list.contains(&"defender");
    if looks_like_can_attack_no_defender {
        return Ok(None);
    }
    let gain_idx = word_list
        .iter()
        .position(|word| matches!(*word, "gain" | "gains" | "has" | "have"));
    let Some(gain_idx) = gain_idx else {
        return Ok(None);
    };
    let Some(gain_token_idx) = token_index_for_word_index(tokens, gain_idx) else {
        return Ok(None);
    };

    let after_gain = &word_list[gain_idx + 1..];
    if matches!(word_list[gain_idx], "gain" | "gains") {
        let starts_with_life = after_gain.first().is_some_and(|word| *word == "life");
        let starts_with_control = after_gain.first().is_some_and(|word| *word == "control");
        if starts_with_life || starts_with_control {
            return Ok(None);
        }
    }

    let leading_duration_phrase = if word_list.starts_with(&["until", "end", "of", "turn"]) {
        Some((4usize, Until::EndOfTurn))
    } else if word_list.starts_with(&["until", "your", "next", "turn"])
        || word_list.starts_with(&["until", "your", "next", "upkeep"])
    {
        Some((4usize, Until::YourNextTurn))
    } else if word_list.starts_with(&["until", "your", "next", "untap", "step"])
        || word_list.starts_with(&["during", "your", "next", "untap", "step"])
    {
        Some((5usize, Until::YourNextTurn))
    } else {
        None
    };
    let subject_start_word_idx = leading_duration_phrase
        .as_ref()
        .map(|(len, _)| *len)
        .unwrap_or(0);
    let subject_start_token_idx = if subject_start_word_idx == 0 {
        0usize
    } else if let Some(idx) = token_index_for_word_index(tokens, subject_start_word_idx) {
        idx
    } else {
        return Ok(None);
    };
    if subject_start_token_idx < gain_token_idx
        && let Some((subject_verb, _)) = find_verb(&tokens[subject_start_token_idx..gain_token_idx])
        && subject_verb != Verb::Get
    {
        return Ok(None);
    }

    let duration_phrase = if let Some(idx) = after_gain
        .windows(4)
        .position(|window| window == ["until", "end", "of", "turn"])
    {
        Some((idx, 4usize, Until::EndOfTurn))
    } else if let Some(idx) = after_gain.windows(4).position(|window| {
        window == ["until", "your", "next", "turn"] || window == ["until", "your", "next", "upkeep"]
    }) {
        Some((idx, 4usize, Until::YourNextTurn))
    } else if let Some(idx) = after_gain.windows(5).position(|window| {
        window == ["until", "your", "next", "untap", "step"]
            || window == ["during", "your", "next", "untap", "step"]
    }) {
        Some((idx, 5usize, Until::YourNextTurn))
    } else if let Some(idx) = after_gain
        .windows(6)
        .position(|window| window == ["for", "as", "long", "as", "you", "control"])
    {
        // Consume the remainder of the phrase as the duration clause.
        Some((
            idx,
            after_gain.len().saturating_sub(idx),
            Until::YouStopControllingThis,
        ))
    } else {
        None
    };
    let duration = duration_phrase
        .as_ref()
        .map(|(_, _, duration)| duration.clone())
        .or_else(|| {
            leading_duration_phrase
                .as_ref()
                .map(|(_, duration)| duration.clone())
        })
        .unwrap_or(Until::Forever);
    let has_explicit_duration =
        duration_phrase.is_some() || leading_duration_phrase.as_ref().is_some();

    let mut trailing_tail_tokens: Vec<Token> = Vec::new();
    if let Some((start_rel, len_words, _)) = duration_phrase {
        let tail_word_idx = gain_idx + 1 + start_rel + len_words;
        if let Some(tail_token_idx) = token_index_for_word_index(tokens, tail_word_idx) {
            let mut tail_tokens = trim_commas(&tokens[tail_token_idx..]).to_vec();
            while tail_tokens
                .first()
                .is_some_and(|token| token.is_word("and") || token.is_word("then"))
            {
                tail_tokens.remove(0);
            }
            if !tail_tokens.is_empty() {
                trailing_tail_tokens = tail_tokens;
            }
        }
    }
    let mut grants_must_attack = false;
    if !trailing_tail_tokens.is_empty() {
        let mut tail_words = words(&trailing_tail_tokens);
        if tail_words.first().is_some_and(|word| *word == "and") {
            tail_words = tail_words[1..].to_vec();
        }
        if tail_words.as_slice() == ["attacks", "this", "combat", "if", "able"]
            || tail_words.as_slice() == ["attack", "this", "combat", "if", "able"]
        {
            grants_must_attack = true;
            trailing_tail_tokens.clear();
        }
    }

    let ability_end_word_idx = duration_phrase
        .as_ref()
        .map(|(start_rel, _, _)| gain_idx + 1 + *start_rel);
    let ability_end_token_idx = if let Some(end_word_idx) = ability_end_word_idx {
        token_index_for_word_index(tokens, end_word_idx).unwrap_or(tokens.len())
    } else {
        tokens.len()
    };
    let ability_start_token_idx = gain_token_idx + 1;
    if ability_start_token_idx > ability_end_token_idx || ability_start_token_idx >= tokens.len() {
        return Ok(None);
    }
    let ability_tokens = trim_commas(&tokens[ability_start_token_idx..ability_end_token_idx]);

    let mut grant_is_choice = false;
    let mut abilities = if let Some(actions) = parse_ability_line(&ability_tokens) {
        reject_unimplemented_keyword_actions(&actions, &word_list.join(" "))?;
        actions
            .into_iter()
            .filter_map(keyword_action_to_static_ability)
            .collect::<Vec<_>>()
    } else if let Some(actions) = parse_choice_of_abilities(&ability_tokens) {
        grant_is_choice = true;
        reject_unimplemented_keyword_actions(&actions, &word_list.join(" "))?;
        actions
            .into_iter()
            .filter_map(keyword_action_to_static_ability)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if abilities.is_empty()
        && let Some(granted) =
            parse_granted_activated_or_triggered_ability_for_gain(&ability_tokens, &word_list)?
    {
        abilities.push(granted);
    }
    if abilities.is_empty() && !grants_must_attack {
        return Ok(None);
    }
    if grants_must_attack {
        abilities.push(StaticAbility::must_attack());
    }

    // Check for "gets +X/+Y and gains/has ..." pattern - if there's a pump modifier
    // before the granting verb, extract it as a separate Pump/PumpAll effect.
    let before_gain = &word_list[subject_start_word_idx..gain_idx];
    let get_idx = before_gain.iter().position(|w| *w == "get" || *w == "gets");
    let pump_effect = if let Some(gi) = get_idx {
        let mod_word = before_gain.get(gi + 1).copied().unwrap_or("");
        if let Ok((power, toughness)) = parse_pt_modifier_values(mod_word) {
            Some((power, toughness, subject_start_word_idx + gi))
        } else {
            None
        }
    } else {
        None
    };
    let has_have_verb = matches!(word_list[gain_idx], "has" | "have");
    if has_have_verb && pump_effect.is_none() && !has_explicit_duration {
        return Ok(None);
    }

    // Determine the real subject (before "get"/"gets" if pump is present)
    let real_subject_end_word_idx = pump_effect
        .as_ref()
        .map(|(_, _, gi)| *gi)
        .unwrap_or(gain_idx);
    let real_subject_end_token_idx =
        token_index_for_word_index(tokens, real_subject_end_word_idx).unwrap_or(gain_token_idx);
    if subject_start_token_idx >= real_subject_end_token_idx {
        return Ok(None);
    }
    let real_subject_tokens =
        trim_commas(&tokens[subject_start_token_idx..real_subject_end_token_idx]);

    let mut effects = Vec::new();

    // Check for pronoun subjects ("it", "they") that reference a prior tagged object.
    let real_subject_words: Vec<&str> = real_subject_tokens
        .iter()
        .filter_map(Token::as_word)
        .collect();
    let is_pronoun_subject =
        real_subject_words.as_slice() == ["it"] || real_subject_words.as_slice() == ["they"];
    if is_pronoun_subject {
        let span = span_from_tokens(&real_subject_tokens);
        let target = TargetAst::Tagged(TagKey::from(IT_TAG), span);
        if let Some((power, toughness, _)) = pump_effect {
            effects.push(EffectAst::Pump {
                power,
                toughness,
                target: target.clone(),
                duration: duration.clone(),
                condition: None,
            });
        }
        if grant_is_choice {
            effects.push(EffectAst::GrantAbilitiesChoiceToTarget {
                target,
                abilities,
                duration,
            });
        } else {
            effects.push(EffectAst::GrantAbilitiesToTarget {
                target,
                abilities,
                duration,
            });
        }
        effects = append_gain_ability_trailing_effects(effects, &trailing_tail_tokens)?;
        return Ok(Some(effects));
    }

    let is_demonstrative_subject = real_subject_words
        .first()
        .is_some_and(|word| *word == "that" || *word == "those");
    if is_demonstrative_subject {
        let target = parse_target_phrase(&real_subject_tokens)?;
        if let Some((power, toughness, _)) = pump_effect {
            effects.push(EffectAst::Pump {
                power,
                toughness,
                target: target.clone(),
                duration: duration.clone(),
                condition: None,
            });
        }
        if grant_is_choice {
            effects.push(EffectAst::GrantAbilitiesChoiceToTarget {
                target,
                abilities,
                duration,
            });
        } else {
            effects.push(EffectAst::GrantAbilitiesToTarget {
                target,
                abilities,
                duration,
            });
        }
        effects = append_gain_ability_trailing_effects(effects, &trailing_tail_tokens)?;
        return Ok(Some(effects));
    }

    if before_gain.contains(&"target") {
        let has_pump_effect = pump_effect.is_some();
        let target = parse_target_phrase(&real_subject_tokens)?;
        if let Some((power, toughness, _)) = pump_effect {
            effects.push(EffectAst::Pump {
                power,
                toughness,
                target: target.clone(),
                duration: duration.clone(),
                condition: None,
            });
        }
        let grant_target = if has_pump_effect {
            TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(&real_subject_tokens))
        } else {
            target
        };
        if grant_is_choice {
            effects.push(EffectAst::GrantAbilitiesChoiceToTarget {
                target: grant_target,
                abilities,
                duration,
            });
        } else {
            effects.push(EffectAst::GrantAbilitiesToTarget {
                target: grant_target,
                abilities,
                duration,
            });
        }
        effects = append_gain_ability_trailing_effects(effects, &trailing_tail_tokens)?;
        return Ok(Some(effects));
    }

    let filter = parse_object_filter(&real_subject_tokens, false).map_err(|_| {
        CardTextError::ParseError(format!(
            "unsupported subject in gain-ability clause (clause: '{}')",
            word_list.join(" ")
        ))
    })?;

    if let Some((power, toughness, _)) = pump_effect {
        effects.push(EffectAst::PumpAll {
            filter: filter.clone(),
            power,
            toughness,
            duration: duration.clone(),
        });
    }
    if grant_is_choice {
        effects.push(EffectAst::GrantAbilitiesChoiceAll {
            filter,
            abilities,
            duration,
        });
    } else {
        effects.push(EffectAst::GrantAbilitiesAll {
            filter,
            abilities,
            duration,
        });
    }
    effects = append_gain_ability_trailing_effects(effects, &trailing_tail_tokens)?;

    Ok(Some(effects))
}

fn parse_granted_activated_or_triggered_ability_for_gain(
    ability_tokens: &[Token],
    clause_words: &[&str],
) -> Result<Option<StaticAbility>, CardTextError> {
    if ability_tokens.is_empty() {
        return Ok(None);
    }

    let has_colon = ability_tokens
        .iter()
        .any(|token| matches!(token, Token::Colon(_)));
    let looks_like_trigger = ability_tokens.first().is_some_and(|token| {
        token.is_word("when")
            || token.is_word("whenever")
            || (token.is_word("at")
                && ability_tokens
                    .get(1)
                    .is_some_and(|next| next.is_word("the")))
    });
    if !has_colon && !looks_like_trigger {
        return Ok(None);
    }

    let mut ability = if has_colon {
        let Some(parsed) = parse_activated_line(ability_tokens)? else {
            return Err(CardTextError::ParseError(format!(
                "unsupported granted activated/triggered ability clause (clause: '{}')",
                clause_words.join(" ")
            )));
        };
        parsed.ability
    } else {
        match parse_triggered_line(ability_tokens)? {
            LineAst::Triggered {
                trigger,
                effects,
                max_triggers_per_turn,
            } => {
                let (compiled_effects, choices) =
                    compile_trigger_effects(Some(&trigger), &effects)?;
                Ability {
                    kind: AbilityKind::Triggered(TriggeredAbility {
                        trigger: compile_trigger_spec(trigger),
                        effects: compiled_effects,
                        choices,
                        intervening_if: max_triggers_per_turn
                            .map(crate::ConditionExpr::MaxTimesEachTurn),
                    }),
                    functional_zones: vec![Zone::Battlefield],
                    text: None,
                }
            }
            _ => {
                return Err(CardTextError::ParseError(format!(
                    "unsupported granted activated/triggered ability clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
        }
    };

    if ability.text.is_none() {
        ability.text = Some(words(ability_tokens).join(" "));
    }

    Ok(Some(StaticAbility::grant_object_ability_for_filter(
        ObjectFilter::source(),
        ability,
        words(ability_tokens).join(" "),
    )))
}

fn append_gain_ability_trailing_effects(
    mut effects: Vec<EffectAst>,
    trailing_tokens: &[Token],
) -> Result<Vec<EffectAst>, CardTextError> {
    if trailing_tokens.is_empty() {
        return Ok(effects);
    }

    let trimmed = trim_commas(trailing_tokens);
    if trimmed.first().is_some_and(|token| token.is_word("unless")) {
        if let Some(unless_effect) = try_build_unless(effects, &trimmed, 0)? {
            return Ok(vec![unless_effect]);
        }
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing unless gain-ability clause (clause: '{}')",
            words(&trimmed).join(" ")
        )));
    }

    if let Ok(parsed_tail) = parse_effect_chain(&trimmed)
        && !parsed_tail.is_empty()
    {
        effects.extend(parsed_tail);
    }
    Ok(effects)
}

fn parse_choice_of_abilities(tokens: &[Token]) -> Option<Vec<KeywordAction>> {
    let tokens = trim_commas(tokens);
    let words = words(&tokens);
    let prefix_words = if words.starts_with(&["your", "choice", "of"]) {
        3usize
    } else if words.starts_with(&["your", "choice", "from"]) {
        3usize
    } else {
        return None;
    };
    if words.len() <= prefix_words + 1 {
        return None;
    }

    let start_idx = token_index_for_word_index(&tokens, prefix_words)?;
    let option_tokens = trim_commas(&tokens[start_idx..]);
    if option_tokens.is_empty() {
        return None;
    }

    let mut actions = Vec::new();
    for segment in split_on_or(&option_tokens) {
        let segment = trim_commas(&segment);
        if segment.is_empty() {
            continue;
        }
        let action = parse_ability_phrase(&segment)?;
        if !actions.contains(&action) {
            actions.push(action);
        }
    }

    if actions.len() < 2 {
        return None;
    }
    Some(actions)
}

fn parse_gain_ability_to_source_sentence(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let gain_idx = clause_words
        .iter()
        .position(|word| *word == "gain" || *word == "gains");
    let Some(gain_idx) = gain_idx else {
        return Ok(None);
    };

    let subject_tokens = &tokens[..gain_idx];
    let subject_words: Vec<&str> = words(subject_tokens)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    if !is_source_reference_words(&subject_words) {
        return Ok(None);
    }

    let ability_tokens = &tokens[gain_idx + 1..];
    if let Some(ability) = parse_activated_line(ability_tokens)? {
        return Ok(Some(EffectAst::GrantAbilityToSource {
            ability: ability.ability,
        }));
    }

    Ok(None)
}

fn parse_search_library_disjunction_filter(filter_tokens: &[Token]) -> Option<ObjectFilter> {
    let segments = split_on_or(filter_tokens);
    if segments.len() < 2 {
        return None;
    }

    let mut branches = Vec::new();
    for segment in segments {
        let trimmed = trim_commas(&segment);
        if trimmed.is_empty() {
            return None;
        }
        let Ok(filter) = parse_object_filter(&trimmed, false) else {
            return None;
        };
        branches.push(filter);
    }

    if branches.len() < 2 {
        return None;
    }

    let mut filter = ObjectFilter::default();
    filter.any_of = branches;
    Some(filter)
}

fn split_search_same_name_reference_filter(tokens: &[Token]) -> Option<(Vec<Token>, Vec<Token>)> {
    let words_all = words(tokens);
    let (start_word_idx, phrase_len) = if let Some(idx) = words_all
        .windows(5)
        .position(|window| window == ["with", "the", "same", "name", "as"])
    {
        (idx, 5usize)
    } else if let Some(idx) = words_all
        .windows(4)
        .position(|window| window == ["with", "same", "name", "as"])
    {
        (idx, 4usize)
    } else {
        return None;
    };

    let start_token_idx = token_index_for_word_index(tokens, start_word_idx)?;
    let end_token_idx =
        token_index_for_word_index(tokens, start_word_idx + phrase_len).unwrap_or(tokens.len());
    let base_filter_tokens = trim_commas(&tokens[..start_token_idx]);
    let reference_tokens = trim_commas(&tokens[end_token_idx..]);
    Some((base_filter_tokens, reference_tokens))
}

fn is_same_name_that_reference_words(words: &[&str]) -> bool {
    matches!(
        words,
        ["that", "card"]
            | ["that", "cards"]
            | ["that", "creature"]
            | ["that", "creatures"]
            | ["that", "artifact"]
            | ["that", "artifacts"]
            | ["that", "enchantment"]
            | ["that", "enchantments"]
            | ["that", "land"]
            | ["that", "lands"]
            | ["that", "permanent"]
            | ["that", "permanents"]
            | ["that", "spell"]
            | ["that", "spells"]
            | ["that", "object"]
            | ["that", "objects"]
            | ["those", "cards"]
            | ["those", "creatures"]
            | ["those", "artifacts"]
            | ["those", "enchantments"]
            | ["those", "lands"]
            | ["those", "permanents"]
            | ["those", "spells"]
            | ["those", "objects"]
    )
}

fn normalize_search_library_filter(filter: &mut ObjectFilter) {
    filter.zone = None;
    if filter.subtypes.iter().any(|subtype| {
        matches!(
            subtype,
            Subtype::Plains
                | Subtype::Island
                | Subtype::Swamp
                | Subtype::Mountain
                | Subtype::Forest
                | Subtype::Desert
        )
    }) && !filter.card_types.contains(&CardType::Land)
    {
        filter.card_types.push(CardType::Land);
    }

    for nested in &mut filter.any_of {
        normalize_search_library_filter(nested);
    }
}

fn parse_search_library_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words_all = words(tokens);
    let Some(search_idx) = tokens
        .iter()
        .position(|token| token.is_word("search") || token.is_word("searches"))
    else {
        return Ok(None);
    };
    if tokens[..search_idx]
        .iter()
        .any(|token| token.is_word("unless"))
    {
        return Ok(None);
    }
    // Allow "you may search ..." to parse as a search sentence, but avoid treating
    // a larger "may ... then search ..." chain as a single search clause. In those
    // cases, we need the higher-level parser to preserve conditionals like "If you do".
    let may_positions: Vec<usize> = tokens[..search_idx]
        .iter()
        .enumerate()
        .filter_map(|(idx, token)| token.is_word("may").then_some(idx))
        .collect();
    if !may_positions.is_empty() {
        let allowed_may = may_positions.len() == 1 && may_positions[0] + 1 == search_idx;
        if !allowed_may {
            return Ok(None);
        }
    }

    let mut subject_tokens = &tokens[..search_idx];
    if subject_tokens
        .last()
        .is_some_and(|token| token.is_word("may"))
    {
        subject_tokens = &subject_tokens[..subject_tokens.len().saturating_sub(1)];
    }
    let mut leading_effects = Vec::new();
    if !subject_tokens.is_empty() && find_verb(subject_tokens).is_some() {
        let mut leading_tokens = trim_commas(subject_tokens);
        while leading_tokens
            .last()
            .is_some_and(|token| token.is_word("then") || token.is_word("and"))
        {
            leading_tokens.pop();
        }
        if !leading_tokens.is_empty() {
            leading_effects = parse_effect_chain_with_sentence_primitives(&leading_tokens)?;
        }
        subject_tokens = &[];
    }
    let mut player = match parse_subject(subject_tokens) {
        SubjectAst::Player(player) => player,
        _ => PlayerAst::Implicit,
    };

    let search_tokens = &tokens[search_idx..];
    let search_words = words(search_tokens);
    let Some(search_verb) = search_words.first().copied() else {
        return Ok(None);
    };
    if !matches!(search_verb, "search" | "searches") {
        return Ok(None);
    }
    let search_body_words = &search_words[1..];
    let mut search_player_target: Option<TargetAst> = None;
    let mut forced_library_owner: Option<PlayerFilter> = None;
    let mut include_hand_and_graveyard_bundle = false;
    let mut nonlibrary_choice_zones: Vec<Zone> = Vec::new();
    if search_body_words.starts_with(&["your", "library", "for"])
        || search_body_words.starts_with(&["their", "library", "for"])
    {
        // Keep player from parsed subject/default context.
    } else if search_body_words.starts_with(&[
        "its",
        "controller",
        "graveyard",
        "hand",
        "and",
        "library",
        "for",
    ]) || search_body_words.starts_with(&[
        "its",
        "controllers",
        "graveyard",
        "hand",
        "and",
        "library",
        "for",
    ]) {
        player = PlayerAst::ItsController;
        forced_library_owner = Some(PlayerFilter::ControllerOf(crate::filter::ObjectRef::Target));
        include_hand_and_graveyard_bundle = true;
    } else if search_body_words.starts_with(&[
        "its",
        "owner",
        "graveyard",
        "hand",
        "and",
        "library",
        "for",
    ]) || search_body_words.starts_with(&[
        "its",
        "owners",
        "graveyard",
        "hand",
        "and",
        "library",
        "for",
    ]) {
        player = PlayerAst::ItsOwner;
        forced_library_owner = Some(PlayerFilter::OwnerOf(crate::filter::ObjectRef::Target));
        include_hand_and_graveyard_bundle = true;
    } else if search_body_words.starts_with(&["target", "player", "library", "for"])
        || search_body_words.starts_with(&["target", "players", "library", "for"])
    {
        search_player_target = Some(parse_target_phrase(&search_tokens[1..3])?);
        forced_library_owner = Some(PlayerFilter::target_player());
    } else if search_body_words.starts_with(&["target", "opponent", "library", "for"])
        || search_body_words.starts_with(&["target", "opponents", "library", "for"])
    {
        search_player_target = Some(parse_target_phrase(&search_tokens[1..3])?);
        forced_library_owner = Some(PlayerFilter::target_opponent());
    } else if search_body_words.starts_with(&["that", "player", "library", "for"])
        || search_body_words.starts_with(&["that", "players", "library", "for"])
    {
        player = PlayerAst::That;
    } else if search_body_words.starts_with(&["its", "controller", "library", "for"])
        || search_body_words.starts_with(&["its", "controllers", "library", "for"])
    {
        player = PlayerAst::ItsController;
    } else if search_body_words.starts_with(&["its", "owner", "library", "for"])
        || search_body_words.starts_with(&["its", "owners", "library", "for"])
    {
        player = PlayerAst::ItsOwner;
    } else {
        // Support "Search your graveyard, hand, and/or library ..." (and ordering variants)
        // by modeling non-library zones as optional alternatives before a library search.
        if search_body_words.first().copied() == Some("your")
            && let Some(for_pos) = search_body_words.iter().position(|word| *word == "for")
            && for_pos > 1
        {
            let zone_words = &search_body_words[1..for_pos];
            let has_library = zone_words
                .iter()
                .any(|word| *word == "library" || *word == "libraries");
            if !has_library {
                return Ok(None);
            }

            let has_graveyard = zone_words
                .iter()
                .any(|word| *word == "graveyard" || *word == "graveyards");
            let has_hand = zone_words
                .iter()
                .any(|word| *word == "hand" || *word == "hands");
            if has_graveyard {
                nonlibrary_choice_zones.push(Zone::Graveyard);
            }
            if has_hand {
                nonlibrary_choice_zones.push(Zone::Hand);
            }
            if nonlibrary_choice_zones.is_empty() {
                return Ok(None);
            }
        } else {
            return Ok(None);
        }
    }
    let mentions_nth_from_top = search_words
        .windows(4)
        .any(|window| window[1] == "from" && window[2] == "the" && window[3] == "top")
        && !search_words
            .windows(4)
            .any(|window| window == ["on", "top", "of", "library"]);
    if mentions_nth_from_top {
        return Err(CardTextError::ParseError(format!(
            "unsupported search-library top-position clause (clause: '{}')",
            words_all.join(" ")
        )));
    }

    let for_idx = search_tokens
        .iter()
        .position(|token| token.is_word("for"))
        .unwrap_or(3);
    let put_idx = search_tokens.iter().position(|token| token.is_word("put"));
    let exile_idx = search_tokens.windows(3).position(|window| {
        window[0].is_word("and")
            && (window[1].is_word("exile") || window[1].is_word("exiles"))
            && (window[2].is_word("them")
                || window[2].is_word("those")
                || window[2].is_word("thosecards"))
    });
    let Some(filter_boundary) = put_idx.or(exile_idx) else {
        return Err(CardTextError::ParseError(format!(
            "missing put-or-exile clause in search-library sentence (clause: '{}')",
            words_all.join(" ")
        )));
    };

    let filter_end = {
        let mut end = filter_boundary;
        for idx in (for_idx + 1)..filter_boundary {
            if !matches!(search_tokens[idx], Token::Comma(_)) {
                continue;
            }
            let next_word = search_tokens[idx + 1..].iter().find_map(Token::as_word);
            if matches!(next_word, Some("put" | "reveal" | "then")) {
                end = idx;
                break;
            }
        }
        if end == filter_boundary
            && let Some(idx) = search_tokens
                .iter()
                .position(|token| token.is_word("reveal") || token.is_word("then"))
        {
            end = end.min(idx);
        }
        end
    };

    if filter_end <= for_idx + 1 {
        return Err(CardTextError::ParseError(format!(
            "missing search filter in search-library sentence (clause: '{}')",
            words_all.join(" ")
        )));
    }

    let count_tokens = &search_tokens[for_idx + 1..filter_end];
    let mut count = ChoiceCount::up_to(1);
    let mut count_used = 0usize;

    if count_tokens.len() >= 2
        && count_tokens[0].is_word("any")
        && count_tokens[1].is_word("number")
    {
        count = ChoiceCount::any_number();
        count_used = 2;
    } else if count_tokens.len() >= 2
        && count_tokens[0].is_word("that")
        && count_tokens[1].is_word("many")
    {
        count = ChoiceCount::any_number();
        count_used = 2;
    } else if count_tokens
        .first()
        .is_some_and(|token| token.is_word("all"))
    {
        count = ChoiceCount::any_number();
        count_used = 1;
    } else if count_tokens.len() >= 2
        && count_tokens[0].is_word("up")
        && count_tokens[1].is_word("to")
    {
        if count_tokens.get(2).is_some_and(|token| token.is_word("x")) {
            count = ChoiceCount::dynamic_x();
            count_used = 3;
        } else if let Some((value, used)) = parse_number(&count_tokens[2..]) {
            count = ChoiceCount::up_to(value as usize);
            count_used = 2 + used;
        }
    } else if count_tokens.first().is_some_and(|token| token.is_word("x")) {
        count = ChoiceCount::dynamic_x();
        count_used = 1;
    } else if let Some((value, used)) = parse_number(count_tokens) {
        count = ChoiceCount::up_to(value as usize);
        count_used = used;
    }

    if count_used < count_tokens.len() && count_tokens[count_used].is_word("of") {
        count_used += 1;
    }

    let filter_start = for_idx + 1 + count_used;
    if filter_start >= filter_end {
        return Err(CardTextError::ParseError(format!(
            "missing object selector in search-library sentence (clause: '{}')",
            words_all.join(" ")
        )));
    }

    enum SameNameReference {
        TaggedIt,
        Target(TargetAst),
        Choose { filter: ObjectFilter, tag: TagKey },
    }

    let raw_filter_tokens = trim_commas(&search_tokens[filter_start..filter_end]);
    let mut filter_tokens = raw_filter_tokens.clone();
    let mut same_name_reference: Option<SameNameReference> = None;
    if let Some((base_filter_tokens, reference_tokens)) =
        split_search_same_name_reference_filter(&raw_filter_tokens)
    {
        if base_filter_tokens.is_empty() || reference_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "incomplete same-name search filter in search-library sentence (clause: '{}')",
                words_all.join(" ")
            )));
        }
        filter_tokens = base_filter_tokens;
        let reference_words = words(&reference_tokens);
        same_name_reference = if is_same_name_that_reference_words(&reference_words) {
            Some(SameNameReference::TaggedIt)
        } else if reference_words.iter().any(|word| *word == "target") {
            let target = parse_target_phrase(&reference_tokens).map_err(|_| {
                CardTextError::ParseError(format!(
                    "unsupported target same-name reference in search-library sentence (clause: '{}')",
                    words_all.join(" ")
                ))
            })?;
            Some(SameNameReference::Target(target))
        } else {
            let mut reference_filter_tokens = reference_tokens.clone();
            let mut other_reference = false;
            if reference_filter_tokens
                .first()
                .is_some_and(|token| token.is_word("another") || token.is_word("other"))
            {
                other_reference = true;
                reference_filter_tokens = trim_commas(&reference_filter_tokens[1..]);
            }
            let reference_filter =
                parse_object_filter(&reference_filter_tokens, other_reference).map_err(|_| {
                CardTextError::ParseError(format!(
                    "unsupported same-name reference filter in search-library sentence (clause: '{}')",
                    words_all.join(" ")
                ))
            })?;
            Some(SameNameReference::Choose {
                filter: reference_filter,
                tag: TagKey::from("same_name_reference"),
            })
        };
    }

    let filter_words: Vec<&str> = words(&filter_tokens)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    let mut filter = if let Some(named_idx) = filter_words.iter().position(|word| *word == "named")
    {
        let name = filter_words
            .iter()
            .skip(named_idx + 1)
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if name.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing card name in named search clause (clause: '{}')",
                words_all.join(" ")
            )));
        }
        let base_words = &filter_words[..named_idx];
        let mut base_filter = if base_words.is_empty()
            || (base_words.len() == 1 && (base_words[0] == "card" || base_words[0] == "cards"))
        {
            ObjectFilter::default()
        } else {
            let base_tokens: Vec<Token> = base_words
                .iter()
                .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
                .collect();
            parse_object_filter(&base_tokens, false).map_err(|_| {
                CardTextError::ParseError(format!(
                    "unsupported named search filter in search-library sentence (clause: '{}')",
                    words_all.join(" ")
                ))
            })?
        };
        base_filter.name = Some(name);
        base_filter
    } else if filter_words.len() == 1 && (filter_words[0] == "card" || filter_words[0] == "cards") {
        ObjectFilter::default()
    } else if filter_words.contains(&"mana")
        && filter_words.contains(&"ability")
        && filter_words.contains(&"or")
    {
        if let Some(disjunction_filter) = parse_search_library_disjunction_filter(&filter_tokens) {
            disjunction_filter
        } else {
            parse_object_filter(&filter_tokens, false).map_err(|_| {
                CardTextError::ParseError(format!(
                    "unsupported search filter in search-library sentence (clause: '{}')",
                    words_all.join(" ")
                ))
            })?
        }
    } else {
        parse_object_filter(&filter_tokens, false).map_err(|_| {
            CardTextError::ParseError(format!(
                "unsupported search filter in search-library sentence (clause: '{}')",
                words_all.join(" ")
            ))
        })?
    };
    if let Some(same_name_tag) = same_name_reference
        .as_ref()
        .map(|reference| match reference {
            SameNameReference::TaggedIt | SameNameReference::Target(_) => TagKey::from(IT_TAG),
            SameNameReference::Choose { tag, .. } => tag.clone(),
        })
    {
        filter.tagged_constraints.push(TaggedObjectConstraint {
            tag: same_name_tag.clone(),
            relation: TaggedOpbjectRelation::SameNameAsTagged,
        });
    }
    if filter.owner.is_none()
        && let Some(owner) = forced_library_owner.clone()
    {
        filter.owner = Some(owner);
    }
    normalize_search_library_filter(&mut filter);

    if words_all.contains(&"mana") && words_all.contains(&"cost") {
        filter.has_mana_cost = true;
        filter.no_x_in_cost = true;
        let mut max_value: Option<u32> = None;
        for word in words_all.iter() {
            if let Ok(value) = word.parse::<u32>() {
                max_value = Some(max_value.map_or(value, |max| max.max(value)));
            }
        }
        if let Some(max_value) = max_value {
            filter.mana_value = Some(crate::filter::Comparison::LessThanOrEqual(max_value as i32));
        }
    }

    let destination = if let Some(put_idx) = put_idx {
        let put_clause_words = words(&search_tokens[put_idx..]);
        if put_clause_words.contains(&"graveyard") {
            Zone::Graveyard
        } else if put_clause_words.contains(&"hand") {
            Zone::Hand
        } else if put_clause_words.contains(&"top") {
            Zone::Library
        } else {
            Zone::Battlefield
        }
    } else {
        Zone::Exile
    };

    let reveal = words_all.contains(&"reveal");
    let trailing_discard_before_shuffle = if let Some(put_idx) = put_idx {
        let discard_idx = search_tokens
            .iter()
            .position(|token| token.is_word("discard") || token.is_word("discards"));
        let shuffle_idx = search_tokens
            .iter()
            .rposition(|token| token.is_word("shuffle") || token.is_word("shuffles"));
        matches!(
            (discard_idx, shuffle_idx),
            (Some(discard_idx), Some(shuffle_idx)) if discard_idx > put_idx && discard_idx < shuffle_idx
        )
    } else {
        false
    };
    let shuffle = (words_all.contains(&"shuffle") && !trailing_discard_before_shuffle)
        || !nonlibrary_choice_zones.is_empty();
    let split_battlefield_and_hand = put_idx.is_some()
        && words_all.contains(&"battlefield")
        && words_all.contains(&"hand")
        && words_all.contains(&"other")
        && words_all.contains(&"one");
    let zone_bundle_filter = if include_hand_and_graveyard_bundle {
        Some(filter.clone())
    } else {
        None
    };
    let mut effects = if !nonlibrary_choice_zones.is_empty() {
        let chosen_tag: TagKey = "searched_nonlibrary".into();
        let battlefield_tapped = destination == Zone::Battlefield && words_all.contains(&"tapped");

        let mut move_effects = Vec::new();
        if reveal {
            move_effects.push(EffectAst::RevealTagged {
                tag: chosen_tag.clone(),
            });
        }
        move_effects.push(EffectAst::MoveToZone {
            target: TargetAst::Tagged(chosen_tag.clone(), span_from_tokens(tokens)),
            zone: destination,
            to_top: matches!(destination, Zone::Library),
            battlefield_controller: ReturnControllerAst::Preserve,
        });

        let mut first_filter = filter.clone();
        first_filter.zone = Some(nonlibrary_choice_zones[0]);
        if first_filter.owner.is_none() {
            first_filter.owner = Some(PlayerFilter::You);
        }

        let did_not_effects = if nonlibrary_choice_zones.len() > 1 {
            let mut second_filter = filter.clone();
            second_filter.zone = Some(nonlibrary_choice_zones[1]);
            if second_filter.owner.is_none() {
                second_filter.owner = Some(PlayerFilter::You);
            }

            vec![
                EffectAst::ChooseObjects {
                    filter: second_filter,
                    count: ChoiceCount::up_to(1),
                    player,
                    tag: chosen_tag.clone(),
                },
                EffectAst::IfResult {
                    predicate: IfResultPredicate::Did,
                    effects: move_effects.clone(),
                },
                EffectAst::IfResult {
                    predicate: IfResultPredicate::DidNot,
                    effects: vec![EffectAst::SearchLibrary {
                        filter,
                        destination,
                        player,
                        reveal,
                        shuffle,
                        count,
                        tapped: battlefield_tapped,
                    }],
                },
            ]
        } else {
            vec![EffectAst::SearchLibrary {
                filter,
                destination,
                player,
                reveal,
                shuffle,
                count,
                tapped: battlefield_tapped,
            }]
        };

        vec![
            EffectAst::ChooseObjects {
                filter: first_filter,
                count: ChoiceCount::up_to(1),
                player,
                tag: chosen_tag.clone(),
            },
            EffectAst::IfResult {
                predicate: IfResultPredicate::Did,
                effects: move_effects,
            },
            EffectAst::IfResult {
                predicate: IfResultPredicate::DidNot,
                effects: did_not_effects,
            },
        ]
    } else if split_battlefield_and_hand {
        let battlefield_tapped = words_all.contains(&"tapped");
        vec![
            EffectAst::SearchLibrary {
                filter: filter.clone(),
                destination: Zone::Battlefield,
                player,
                reveal,
                shuffle: false,
                count: ChoiceCount::up_to(1),
                tapped: battlefield_tapped,
            },
            EffectAst::SearchLibrary {
                filter,
                destination: Zone::Hand,
                player,
                reveal,
                shuffle,
                count: ChoiceCount::up_to(1),
                tapped: false,
            },
        ]
    } else {
        let battlefield_tapped = destination == Zone::Battlefield && words_all.contains(&"tapped");
        vec![EffectAst::SearchLibrary {
            filter,
            destination,
            player,
            reveal,
            shuffle,
            count,
            tapped: battlefield_tapped,
        }]
    };

    if include_hand_and_graveyard_bundle && let Some(base_filter) = zone_bundle_filter {
        for zone in [Zone::Graveyard, Zone::Hand] {
            let mut zone_filter = base_filter.clone();
            zone_filter.zone = Some(zone);
            if zone_filter.owner.is_none() {
                zone_filter.owner = forced_library_owner.clone();
            }
            effects.push(EffectAst::ExileAll {
                filter: zone_filter,
                face_down: false,
            });
        }
    }

    if trailing_discard_before_shuffle
        && let (Some(discard_idx), Some(shuffle_idx)) = (
            search_tokens
                .iter()
                .position(|token| token.is_word("discard") || token.is_word("discards")),
            search_tokens
                .iter()
                .rposition(|token| token.is_word("shuffle") || token.is_word("shuffles")),
        )
    {
        let mut discard_end = shuffle_idx;
        while discard_end > discard_idx {
            let token = &search_tokens[discard_end - 1];
            if matches!(token, Token::Comma(_)) || token.is_word("then") || token.is_word("and") {
                discard_end -= 1;
                continue;
            }
            break;
        }

        let discard_tokens = trim_commas(&search_tokens[discard_idx..discard_end]);
        if !discard_tokens.is_empty() {
            effects.push(parse_effect_clause(&discard_tokens)?);
        }
        effects.push(EffectAst::ShuffleLibrary { player });
    }

    if let Some(target) = search_player_target {
        effects.insert(0, EffectAst::TargetOnly { target });
    }

    if let Some(and_idx) = search_tokens
        .iter()
        .enumerate()
        .skip(put_idx.unwrap_or(filter_boundary))
        .find_map(|(idx, token)| token.is_word("and").then_some(idx))
    {
        let trailing_tokens = trim_commas(&search_tokens[and_idx + 1..]);
        if !trailing_tokens.is_empty() {
            let trailing_words = words(&trailing_tokens);
            let starts_with_life_clause = trailing_words.starts_with(&["you", "gain"])
                || trailing_words.starts_with(&["target", "player", "gains"])
                || trailing_words.starts_with(&["target", "player", "gain"]);
            if starts_with_life_clause {
                let trailing_effect = parse_effect_clause(&trailing_tokens)?;
                effects.push(trailing_effect);
            }
        }
    }

    if let Some(reference) = same_name_reference {
        match reference {
            SameNameReference::TaggedIt => {}
            SameNameReference::Target(target) => {
                effects.insert(0, EffectAst::TargetOnly { target });
            }
            SameNameReference::Choose { filter, tag } => {
                effects.insert(
                    0,
                    EffectAst::ChooseObjects {
                        filter,
                        count: ChoiceCount::exactly(1),
                        player,
                        tag,
                    },
                );
            }
        }
    }

    if !leading_effects.is_empty() {
        leading_effects.extend(effects);
        return Ok(Some(leading_effects));
    }

    Ok(Some(effects))
}

fn parse_shuffle_graveyard_into_library_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if tokens.is_empty() {
        return Ok(None);
    }

    let mut clause_tokens = trim_commas(tokens);
    while clause_tokens
        .first()
        .is_some_and(|token| token.is_word("then") || token.is_word("and"))
    {
        clause_tokens.remove(0);
    }
    if clause_tokens.is_empty() {
        return Ok(None);
    }

    let clause_words = words(&clause_tokens);
    if !clause_words
        .iter()
        .any(|word| *word == "shuffle" || *word == "shuffles")
        || !clause_words.contains(&"graveyard")
        || !clause_words.contains(&"library")
    {
        return Ok(None);
    }

    let Some(shuffle_idx) = clause_tokens
        .iter()
        .position(|token| token.is_word("shuffle") || token.is_word("shuffles"))
    else {
        return Ok(None);
    };

    // Keep this primitive focused on shuffle-led clauses so we don't swallow
    // earlier effects in chains like "... then shuffle your graveyard ...".
    if shuffle_idx > 3 {
        return Ok(None);
    }

    let subject_tokens = trim_commas(&clause_tokens[..shuffle_idx]);
    let each_player_subject = {
        let subject_words = words(&subject_tokens);
        subject_words.starts_with(&["each", "player"])
            || subject_words.starts_with(&["each", "players"])
    };
    let subject = if subject_tokens.is_empty() {
        SubjectAst::Player(PlayerAst::You)
    } else if each_player_subject {
        SubjectAst::Player(PlayerAst::Implicit)
    } else {
        parse_subject(&subject_tokens)
    };
    let player = match subject {
        SubjectAst::Player(player) => player,
        SubjectAst::This => return Ok(None),
    };

    let body_tokens = trim_commas(&clause_tokens[shuffle_idx + 1..]);
    if body_tokens.is_empty() {
        return Ok(None);
    }

    let Some(into_idx) = body_tokens.iter().position(|token| token.is_word("into")) else {
        return Ok(None);
    };
    if into_idx == 0 {
        return Ok(None);
    }

    let destination_tokens = trim_commas(&body_tokens[into_idx + 1..]);
    let destination_words = words(&destination_tokens);
    if !destination_words.contains(&"library") {
        return Ok(None);
    }
    let owner_library_destination = destination_words.iter().any(|word| word.contains("owner"));
    let trailing_tokens = destination_tokens
        .iter()
        .position(|token| token.is_word("library") || token.is_word("libraries"))
        .map(|idx| trim_commas(&destination_tokens[idx + 1..]).to_vec())
        .unwrap_or_default();
    let append_trailing =
        |mut effects: Vec<EffectAst>| -> Result<Option<Vec<EffectAst>>, CardTextError> {
            if trailing_tokens.is_empty() {
                return Ok(Some(effects));
            }
            let mut trailing_effects = parse_effect_chain(&trailing_tokens)?;
            if each_player_subject {
                for effect in &mut trailing_effects {
                    maybe_apply_carried_player(effect, CarryContext::ForEachPlayer);
                }
            } else {
                for effect in &mut trailing_effects {
                    maybe_apply_carried_player_with_clause(
                        effect,
                        CarryContext::Player(player),
                        &trailing_tokens,
                    );
                }
            }
            effects.extend(trailing_effects);
            Ok(Some(effects))
        };

    let target_tokens = trim_commas(&body_tokens[..into_idx]);
    if target_tokens.is_empty() {
        return Ok(None);
    }
    let target_words = words(&target_tokens);
    if !target_words.contains(&"graveyard") {
        return Ok(None);
    }

    let has_target_selector = target_words.contains(&"target");
    if !has_target_selector {
        let mut effects = Vec::new();
        let has_source_and_graveyard_clause = target_words
            .starts_with(&["this", "artifact", "and"])
            || target_words.starts_with(&["this", "permanent", "and"])
            || target_words.starts_with(&["this", "card", "and"]);
        if has_source_and_graveyard_clause {
            effects.push(EffectAst::MoveToZone {
                target: TargetAst::Source(None),
                zone: Zone::Library,
                to_top: false,
                battlefield_controller: ReturnControllerAst::Preserve,
            });
            if owner_library_destination {
                effects.push(EffectAst::ShuffleLibrary {
                    player: PlayerAst::ItsOwner,
                });
            }
        }
        if each_player_subject && target_words.contains(&"hand") {
            let mut hand_filter = ObjectFilter::default();
            hand_filter.zone = Some(Zone::Hand);
            hand_filter.owner = Some(PlayerFilter::IteratedPlayer);
            effects.push(EffectAst::MoveToZone {
                target: TargetAst::Object(hand_filter, None, None),
                zone: Zone::Library,
                to_top: false,
                battlefield_controller: ReturnControllerAst::Preserve,
            });
        }
        effects.push(EffectAst::ShuffleGraveyardIntoLibrary { player });
        if each_player_subject {
            return append_trailing(vec![EffectAst::ForEachPlayer { effects }]);
        }
        return append_trailing(effects);
    }

    let mut target = parse_target_phrase(&target_tokens)?;
    apply_shuffle_subject_graveyard_owner_context(&mut target, subject);

    append_trailing(vec![
        EffectAst::MoveToZone {
            target,
            zone: Zone::Library,
            to_top: false,
            battlefield_controller: ReturnControllerAst::Preserve,
        },
        EffectAst::ShuffleLibrary { player },
    ])
}

fn parse_exile_hand_and_graveyard_bundle_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    if tokens.is_empty() {
        return Ok(None);
    }

    let mut clause_tokens = trim_commas(tokens);
    while clause_tokens
        .first()
        .is_some_and(|token| token.is_word("then") || token.is_word("and"))
    {
        clause_tokens.remove(0);
    }
    if clause_tokens.is_empty() {
        return Ok(None);
    }

    let clause_words = words(&clause_tokens);
    if !clause_words.starts_with(&["exile", "all", "cards", "from"]) {
        return Ok(None);
    }
    if !clause_words.contains(&"hand") && !clause_words.contains(&"hands") {
        return Ok(None);
    }
    if !clause_words.contains(&"graveyard") && !clause_words.contains(&"graveyards") {
        return Ok(None);
    }

    let first_zone_idx = clause_words
        .iter()
        .position(|word| matches!(*word, "hand" | "hands" | "graveyard" | "graveyards"))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing zone in exile hand+graveyard clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    if first_zone_idx <= 4 {
        return Ok(None);
    }

    let owner_words = &clause_words[4..first_zone_idx];
    let owner = match owner_words {
        ["target", "player"] | ["target", "players"] => PlayerFilter::target_player(),
        ["target", "opponent"] | ["target", "opponents"] => PlayerFilter::target_opponent(),
        ["your"] => PlayerFilter::You,
        _ => return Ok(None),
    };

    let Some(first_zone) = parse_zone_word(clause_words[first_zone_idx]) else {
        return Ok(None);
    };
    if !matches!(first_zone, Zone::Hand | Zone::Graveyard) {
        return Ok(None);
    }

    let Some(and_word) = clause_words.get(first_zone_idx + 1) else {
        return Ok(None);
    };
    if *and_word != "and" {
        return Ok(None);
    }

    let mut second_zone_idx = first_zone_idx + 2;
    while clause_words
        .get(second_zone_idx)
        .is_some_and(|word| matches!(*word, "all" | "cards" | "from"))
    {
        second_zone_idx += 1;
    }
    let Some(second_zone_word) = clause_words.get(second_zone_idx) else {
        return Ok(None);
    };
    if clause_words.len() != second_zone_idx + 1 {
        return Ok(None);
    }
    let Some(second_zone) = parse_zone_word(second_zone_word) else {
        return Ok(None);
    };
    if !matches!(second_zone, Zone::Hand | Zone::Graveyard) || second_zone == first_zone {
        return Ok(None);
    }

    let mut first_filter = ObjectFilter::default().in_zone(first_zone);
    first_filter.owner = Some(owner.clone());
    let mut second_filter = ObjectFilter::default().in_zone(second_zone);
    second_filter.owner = Some(owner);

    Ok(Some(vec![
        EffectAst::ExileAll {
            filter: first_filter,
            face_down: false,
        },
        EffectAst::ExileAll {
            filter: second_filter,
            face_down: false,
        },
    ]))
}

fn parse_target_player_exiles_creature_and_graveyard_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_tokens = trim_commas(tokens);
    let clause_words = words(&clause_tokens);
    if clause_words.len() < 8 {
        return Ok(None);
    }

    let (subject_player, subject_filter) = if clause_words.starts_with(&["target", "opponent"]) {
        (PlayerAst::TargetOpponent, PlayerFilter::target_opponent())
    } else if clause_words.starts_with(&["target", "player"]) {
        (PlayerAst::Target, PlayerFilter::target_player())
    } else {
        return Ok(None);
    };

    let verb_idx = 2usize;
    if !matches!(
        clause_words.get(verb_idx).copied(),
        Some("exile") | Some("exiles")
    ) {
        return Ok(None);
    }

    let tail_words = &clause_words[verb_idx + 1..];
    let Some(and_idx) = tail_words.iter().position(|word| *word == "and") else {
        return Ok(None);
    };
    let creature_words = &tail_words[..and_idx];
    let graveyard_words = &tail_words[and_idx + 1..];

    if graveyard_words != ["their", "graveyard"] {
        return Ok(None);
    }

    let creature_words = if creature_words.first().is_some_and(|word| is_article(word)) {
        &creature_words[1..]
    } else {
        creature_words
    };
    let creature_clause_matches = creature_words == ["creature", "they", "control"]
        || creature_words == ["creature", "that", "player", "controls"];
    if !creature_clause_matches {
        return Ok(None);
    }

    let mut creature_filter = ObjectFilter::creature();
    creature_filter.controller = Some(subject_filter.clone());

    let mut graveyard_filter = ObjectFilter::default().in_zone(Zone::Graveyard);
    graveyard_filter.owner = Some(subject_filter);

    Ok(Some(vec![
        EffectAst::ChooseObjects {
            filter: creature_filter,
            count: ChoiceCount::exactly(1),
            player: subject_player,
            tag: TagKey::from(IT_TAG),
        },
        EffectAst::Exile {
            target: TargetAst::Tagged(TagKey::from(IT_TAG), None),
            face_down: false,
        },
        EffectAst::ExileAll {
            filter: graveyard_filter,
            face_down: false,
        },
    ]))
}

fn parse_for_each_exiled_this_way_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words_all = words(tokens);
    if !words_all.starts_with(&["for", "each", "permanent", "exiled", "this", "way"]) {
        return Ok(None);
    }
    if !words_all.contains(&"shares")
        || !words_all.contains(&"card")
        || !words_all.contains(&"type")
        || !words_all.contains(&"library")
        || !words_all.contains(&"battlefield")
    {
        return Ok(None);
    }

    let filter_tokens = tokenize_line("a permanent that shares a card type with it", 0);
    let filter = parse_object_filter(&filter_tokens, false)?;

    Ok(Some(vec![EffectAst::ForEachTagged {
        tag: "exiled_0".into(),
        effects: vec![EffectAst::SearchLibrary {
            filter,
            destination: Zone::Battlefield,
            player: PlayerAst::Implicit,
            reveal: true,
            shuffle: true,
            count: ChoiceCount::up_to(1),
            tapped: false,
        }],
    }]))
}

fn parse_each_player_put_permanent_cards_exiled_with_source_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words_all = words(tokens);
    let starts_with_each_player_turns_face_up =
        words_all.starts_with(&["each", "player", "turns", "face", "up", "all", "cards"]);
    if !starts_with_each_player_turns_face_up {
        return Ok(None);
    }
    let has_exiled_with_this = words_all
        .windows(3)
        .any(|window| window == ["exiled", "with", "this"]);
    if !has_exiled_with_this {
        return Ok(None);
    }
    let has_puts_all_permanent_cards = words_all
        .windows(5)
        .any(|window| window == ["then", "puts", "all", "permanent", "cards"]);
    let has_among_them_onto_battlefield = words_all
        .windows(4)
        .any(|window| window == ["among", "them", "onto", "battlefield"])
        || words_all
            .windows(5)
            .any(|window| window == ["among", "them", "onto", "the", "battlefield"]);
    if !has_puts_all_permanent_cards || !has_among_them_onto_battlefield {
        return Ok(None);
    }

    let mut filter = ObjectFilter::default().in_zone(Zone::Exile);
    filter.owner = Some(PlayerFilter::IteratedPlayer);
    filter.card_types = vec![
        CardType::Artifact,
        CardType::Creature,
        CardType::Enchantment,
        CardType::Land,
        CardType::Planeswalker,
        CardType::Battle,
    ];
    filter.tagged_constraints.push(TaggedObjectConstraint {
        tag: TagKey::from(crate::tag::SOURCE_EXILED_TAG),
        relation: TaggedOpbjectRelation::IsTaggedObject,
    });

    Ok(Some(vec![EffectAst::ForEachPlayer {
        effects: vec![EffectAst::ReturnAllToBattlefield {
            filter,
            tapped: false,
        }],
    }]))
}

fn parse_for_each_destroyed_this_way_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let words_all = words(tokens);
    if !words_all.starts_with(&["for", "each"]) {
        return Ok(None);
    }
    let refers_to_destroyed = words_all
        .windows(3)
        .any(|window| window == ["destroyed", "this", "way"]);
    let refers_to_died = words_all
        .windows(3)
        .any(|window| window == ["died", "this", "way"]);
    if !refers_to_destroyed && !refers_to_died {
        return Ok(None);
    }

    let comma_idx = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing comma after 'for each ... this way' clause (clause: '{}')",
                words_all.join(" ")
            ))
        })?;
    let effect_tokens = trim_commas(&tokens[comma_idx + 1..]);
    if effect_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing effect after 'for each ... this way' clause (clause: '{}')",
            words_all.join(" ")
        )));
    }
    let effects = parse_effect_chain(&effect_tokens)?;
    if effects.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "empty effect after 'for each ... this way' clause (clause: '{}')",
            words_all.join(" ")
        )));
    }

    Ok(Some(vec![EffectAst::ForEachTagged {
        tag: IT_TAG.into(),
        effects,
    }]))
}

fn parse_earthbend_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words = words(tokens);
    if words.first().copied() != Some("earthbend") {
        return Ok(None);
    }

    let count_tokens = &tokens[1..];
    let (count, _) = parse_number(count_tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing earthbend count (clause: '{}')",
            words.join(" ")
        ))
    })?;

    Ok(Some(EffectAst::Earthbend { counters: count }))
}

fn parse_enchant_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words = words(tokens);
    if words.is_empty() || words[0] != "enchant" {
        return Ok(None);
    }

    let remaining = if tokens.len() > 1 { &tokens[1..] } else { &[] };
    let filter = parse_object_filter(remaining, false)?;
    Ok(Some(EffectAst::Enchant { filter }))
}

fn parse_cant_effect_sentence(tokens: &[Token]) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let Some((duration, clause_tokens)) = parse_restriction_duration(tokens)? else {
        return Ok(None);
    };
    if clause_tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "restriction clause missing body".to_string(),
        ));
    }
    if find_negation_span(&clause_tokens).is_none() {
        return Ok(None);
    }

    let Some(restrictions) = parse_cant_restrictions(&clause_tokens)? else {
        return Err(CardTextError::ParseError(format!(
            "unsupported restriction clause body (clause: '{}')",
            words(&clause_tokens).join(" ")
        )));
    };

    let mut target: Option<TargetAst> = None;
    let mut effects = Vec::new();
    for parsed in restrictions {
        if let Some(parsed_target) = parsed.target {
            if let Some(existing) = &target {
                if *existing != parsed_target {
                    return Err(CardTextError::ParseError(format!(
                        "unsupported mixed restriction targets (clause: '{}')",
                        words(&clause_tokens).join(" ")
                    )));
                }
            } else {
                target = Some(parsed_target);
            }
        }
        effects.push(EffectAst::Cant {
            restriction: parsed.restriction,
            duration: duration.clone(),
        });
    }
    if let Some(target) = target {
        effects.insert(0, EffectAst::TargetOnly { target });
    }

    Ok(Some(effects))
}

fn parse_restriction_duration(
    tokens: &[Token],
) -> Result<Option<(crate::effect::Until, Vec<Token>)>, CardTextError> {
    use crate::effect::Until;

    let all_words = words(tokens);
    if all_words.len() < 4 {
        return Ok(None);
    }

    if all_words.starts_with(&["until", "end", "of", "turn"]) {
        let comma_idx = tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)));
        let remainder = if let Some(idx) = comma_idx {
            &tokens[idx + 1..]
        } else {
            &tokens[4..]
        };
        return Ok(Some((Until::EndOfTurn, trim_commas(remainder))));
    }

    if all_words.starts_with(&["until", "your", "next", "turn"]) {
        let comma_idx = tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)));
        let remainder = if let Some(idx) = comma_idx {
            &tokens[idx + 1..]
        } else {
            &tokens[4..]
        };
        return Ok(Some((Until::YourNextTurn, trim_commas(remainder))));
    }

    if all_words.starts_with(&["for", "as", "long", "as"]) {
        let as_long_duration = all_words.contains(&"you")
            && all_words.contains(&"control")
            && (all_words.contains(&"this")
                || all_words.contains(&"thiss")
                || all_words.contains(&"source")
                || all_words.contains(&"creature")
                || all_words.contains(&"permanent"));
        if !as_long_duration {
            return Ok(None);
        }
        let Some(comma_idx) = tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)))
        else {
            return Err(CardTextError::ParseError(
                "missing comma after duration prefix".to_string(),
            ));
        };
        let remainder = trim_commas(&tokens[comma_idx + 1..]);
        return Ok(Some((Until::YouStopControllingThis, remainder)));
    }

    if all_words.ends_with(&["until", "end", "of", "turn"]) {
        let end_idx = tokens
            .iter()
            .rposition(|token| token.is_word("until"))
            .unwrap_or(tokens.len());
        let remainder = trim_commas(&tokens[..end_idx]);
        return Ok(Some((Until::EndOfTurn, remainder)));
    }

    if all_words.ends_with(&["until", "your", "next", "turn"])
        || (all_words.ends_with(&["next", "turn"]) && all_words.contains(&"until"))
    {
        let end_idx = tokens
            .iter()
            .rposition(|token| token.is_word("until"))
            .unwrap_or(tokens.len());
        let remainder = trim_commas(&tokens[..end_idx]);
        return Ok(Some((Until::YourNextTurn, remainder)));
    }

    if all_words.ends_with(&["during", "your", "next", "untap", "step"]) {
        let during_idx = tokens
            .iter()
            .rposition(|token| token.is_word("during"))
            .unwrap_or(tokens.len());
        let remainder = trim_commas(&tokens[..during_idx]);
        if !remainder.is_empty() {
            return Ok(Some((Until::YourNextTurn, remainder)));
        }
    }

    if all_words.ends_with(&["during", "its", "controller", "next", "untap", "step"])
        || all_words.ends_with(&["during", "its", "controllers", "next", "untap", "step"])
        || all_words.ends_with(&["during", "their", "controller", "next", "untap", "step"])
        || all_words.ends_with(&["during", "their", "controllers", "next", "untap", "step"])
    {
        let during_idx = tokens
            .iter()
            .rposition(|token| token.is_word("during"))
            .unwrap_or(tokens.len());
        let remainder = trim_commas(&tokens[..during_idx]);
        if !remainder.is_empty() {
            return Ok(Some((Until::ControllersNextUntapStep, remainder)));
        }
    }

    let suffix_idx = tokens.windows(4).position(|window| {
        window[0].is_word("for")
            && window[1].is_word("as")
            && window[2].is_word("long")
            && window[3].is_word("as")
    });
    if let Some(idx) = suffix_idx {
        let suffix_words = words(&tokens[idx..]);
        let as_long_duration = suffix_words.contains(&"you")
            && suffix_words.contains(&"control")
            && (suffix_words.contains(&"this")
                || suffix_words.contains(&"thiss")
                || suffix_words.contains(&"source")
                || suffix_words.contains(&"creature")
                || suffix_words.contains(&"permanent"));
        if as_long_duration {
            let remainder = trim_commas(&tokens[..idx]);
            return Ok(Some((Until::YouStopControllingThis, remainder)));
        }
    }

    let has_this_turn = all_words
        .windows(2)
        .any(|window| window == ["this", "turn"]);
    if has_this_turn {
        let mut cleaned = Vec::new();
        let mut idx = 0usize;
        while idx < tokens.len() {
            if tokens[idx].is_word("this")
                && tokens
                    .get(idx + 1)
                    .is_some_and(|token| token.is_word("turn"))
            {
                idx += 2;
                continue;
            }
            cleaned.push(tokens[idx].clone());
            idx += 1;
        }
        let remainder = trim_commas(&cleaned).to_vec();
        if !remainder.is_empty() {
            return Ok(Some((Until::EndOfTurn, remainder)));
        }
    }

    Ok(None)
}

fn parse_play_from_graveyard_sentence(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let line_words = words(tokens);
    if line_words.len() < 8 || !line_words.starts_with(&["until", "end", "of", "turn"]) {
        return Ok(None);
    }

    let comma_idx = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)));
    let remainder = if let Some(idx) = comma_idx {
        &tokens[idx + 1..]
    } else {
        &tokens[4..]
    };

    let remaining_words: Vec<&str> = words(remainder)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();

    let expected = [
        "you",
        "may",
        "play",
        "lands",
        "and",
        "cast",
        "spells",
        "from",
        "your",
        "graveyard",
    ];

    if remaining_words == expected {
        return Ok(Some(EffectAst::PlayFromGraveyardUntilEot {
            player: PlayerAst::You,
        }));
    }

    Ok(None)
}

fn parse_exile_instead_of_graveyard_sentence(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let line_words = words(tokens);
    if line_words.first().copied() != Some("if") {
        return Ok(None);
    }

    let has_graveyard_clause = line_words
        .windows(4)
        .any(|w| w == ["into", "your", "graveyard", "from"])
        || line_words
            .windows(3)
            .any(|w| w == ["your", "graveyard", "from"])
        || (line_words.contains(&"your") && line_words.contains(&"graveyard"));
    let has_would_put = line_words
        .windows(4)
        .any(|w| w == ["card", "would", "be", "put"]);
    let has_this_turn = line_words.contains(&"this") && line_words.contains(&"turn");
    if !has_graveyard_clause || !has_would_put || !has_this_turn {
        return Ok(None);
    }

    let comma_idx = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)));
    let remainder = if let Some(idx) = comma_idx {
        &tokens[idx + 1..]
    } else {
        return Ok(None);
    };

    let remaining_words: Vec<&str> = words(remainder)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();

    let expected = ["exile", "that", "card", "instead"];
    if remaining_words == expected {
        return Ok(Some(EffectAst::ExileInsteadOfGraveyardThisTurn {
            player: PlayerAst::You,
        }));
    }

    Ok(None)
}

fn parse_scryfall_mana_cost(raw: &str) -> Result<ManaCost, CardTextError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "—" {
        return Ok(ManaCost::new());
    }

    let mut pips: Vec<Vec<ManaSymbol>> = Vec::new();
    let mut current = String::new();
    let mut in_brace = false;

    for ch in trimmed.chars() {
        if ch == '{' {
            in_brace = true;
            current.clear();
            continue;
        }
        if ch == '}' {
            if !in_brace {
                continue;
            }
            in_brace = false;
            if current.is_empty() {
                continue;
            }
            let alternatives = parse_mana_symbol_group(&current)?;
            if !alternatives.is_empty() {
                pips.push(alternatives);
            }
            continue;
        }
        if in_brace {
            current.push(ch);
        }
    }

    Ok(ManaCost::from_pips(pips))
}

fn parse_mana_symbol_group(raw: &str) -> Result<Vec<ManaSymbol>, CardTextError> {
    let mut alternatives = Vec::new();
    for part in raw.split('/') {
        let symbol = parse_mana_symbol(part)?;
        alternatives.push(symbol);
    }
    Ok(alternatives)
}

fn parse_mana_symbol(part: &str) -> Result<ManaSymbol, CardTextError> {
    let upper = part.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return Err(CardTextError::ParseError("empty mana symbol".to_string()));
    }

    if upper.chars().all(|c| c.is_ascii_digit()) {
        let value = upper.parse::<u8>().map_err(|_| {
            CardTextError::ParseError(format!("invalid generic mana symbol '{part}'"))
        })?;
        return Ok(ManaSymbol::Generic(value));
    }

    match upper.as_str() {
        "W" => Ok(ManaSymbol::White),
        "U" => Ok(ManaSymbol::Blue),
        "B" => Ok(ManaSymbol::Black),
        "R" => Ok(ManaSymbol::Red),
        "G" => Ok(ManaSymbol::Green),
        "C" => Ok(ManaSymbol::Colorless),
        "S" => Ok(ManaSymbol::Snow),
        "X" => Ok(ManaSymbol::X),
        "P" => Ok(ManaSymbol::Life(2)),
        _ => Err(CardTextError::ParseError(format!(
            "unsupported mana symbol '{part}'"
        ))),
    }
}

fn parse_type_line(
    raw: &str,
) -> Result<(Vec<Supertype>, Vec<CardType>, Vec<Subtype>), CardTextError> {
    let mut supertypes = Vec::new();
    let mut card_types = Vec::new();
    let mut subtypes = Vec::new();

    let parts: Vec<&str> = raw.split('—').collect();
    let left = parts[0].trim();
    let right = parts.get(1).map(|s| s.trim());

    for word in left.split_whitespace() {
        if let Some(supertype) = parse_supertype_word(word) {
            supertypes.push(supertype);
            continue;
        }
        if let Some(card_type) = parse_card_type(&word.to_ascii_lowercase()) {
            card_types.push(card_type);
        }
    }

    if let Some(right) = right {
        for word in right.split_whitespace() {
            if let Some(subtype) = parse_subtype_word(word) {
                subtypes.push(subtype);
            }
        }
    }

    Ok((supertypes, card_types, subtypes))
}

fn parse_supertype_word(word: &str) -> Option<Supertype> {
    match word.to_ascii_lowercase().as_str() {
        "basic" => Some(Supertype::Basic),
        "legendary" => Some(Supertype::Legendary),
        "snow" => Some(Supertype::Snow),
        "world" => Some(Supertype::World),
        _ => None,
    }
}

fn parse_subtype_word(word: &str) -> Option<Subtype> {
    match word.to_ascii_lowercase().as_str() {
        "plains" => Some(Subtype::Plains),
        "island" => Some(Subtype::Island),
        "swamp" => Some(Subtype::Swamp),
        "mountain" => Some(Subtype::Mountain),
        "forest" => Some(Subtype::Forest),
        "desert" | "deserts" => Some(Subtype::Desert),
        "urzas" => Some(Subtype::Urzas),
        "cave" | "caves" => Some(Subtype::Cave),
        "gate" | "gates" => Some(Subtype::Gate),
        "locus" | "loci" => Some(Subtype::Locus),
        "advisor" => Some(Subtype::Advisor),
        "ally" | "allies" => Some(Subtype::Ally),
        "alien" | "aliens" => Some(Subtype::Alien),
        "angel" => Some(Subtype::Angel),
        "ape" => Some(Subtype::Ape),
        "army" | "armies" => Some(Subtype::Army),
        "archer" => Some(Subtype::Archer),
        "artificer" => Some(Subtype::Artificer),
        "assassin" => Some(Subtype::Assassin),
        "astartes" => Some(Subtype::Astartes),
        "avatar" => Some(Subtype::Avatar),
        "barbarian" => Some(Subtype::Barbarian),
        "bard" => Some(Subtype::Bard),
        "bat" | "bats" => Some(Subtype::Bat),
        "bear" => Some(Subtype::Bear),
        "beast" => Some(Subtype::Beast),
        "berserker" => Some(Subtype::Berserker),
        "bird" => Some(Subtype::Bird),
        "boar" => Some(Subtype::Boar),
        "cat" => Some(Subtype::Cat),
        "centaur" => Some(Subtype::Centaur),
        "citizen" | "citizens" => Some(Subtype::Citizen),
        "coward" | "cowards" => Some(Subtype::Coward),
        "changeling" => Some(Subtype::Changeling),
        "cleric" => Some(Subtype::Cleric),
        "construct" => Some(Subtype::Construct),
        "crab" => Some(Subtype::Crab),
        "crocodile" => Some(Subtype::Crocodile),
        "dalek" => Some(Subtype::Dalek),
        "dauthi" => Some(Subtype::Dauthi),
        "detective" => Some(Subtype::Detective),
        "demon" => Some(Subtype::Demon),
        "devil" => Some(Subtype::Devil),
        "dinosaur" => Some(Subtype::Dinosaur),
        "djinn" => Some(Subtype::Djinn),
        "efreet" | "efreets" => Some(Subtype::Efreet),
        "dog" => Some(Subtype::Dog),
        "drone" | "drones" => Some(Subtype::Drone),
        "dragon" => Some(Subtype::Dragon),
        "drake" => Some(Subtype::Drake),
        "druid" => Some(Subtype::Druid),
        "dwarf" => Some(Subtype::Dwarf),
        "elder" => Some(Subtype::Elder),
        "eldrazi" => Some(Subtype::Eldrazi),
        "spawn" | "spawns" => Some(Subtype::Spawn),
        "scion" | "scions" => Some(Subtype::Scion),
        "elemental" => Some(Subtype::Elemental),
        "elephant" => Some(Subtype::Elephant),
        "elf" | "elves" => Some(Subtype::Elf),
        "faerie" => Some(Subtype::Faerie),
        "fish" => Some(Subtype::Fish),
        "fox" => Some(Subtype::Fox),
        "frog" => Some(Subtype::Frog),
        "fungus" => Some(Subtype::Fungus),
        "gargoyle" => Some(Subtype::Gargoyle),
        "giant" => Some(Subtype::Giant),
        "gnome" => Some(Subtype::Gnome),
        "glimmer" | "glimmers" => Some(Subtype::Glimmer),
        "goat" => Some(Subtype::Goat),
        "goblin" => Some(Subtype::Goblin),
        "god" => Some(Subtype::God),
        "golem" => Some(Subtype::Golem),
        "gorgon" => Some(Subtype::Gorgon),
        "germ" | "germs" => Some(Subtype::Germ),
        "gremlin" | "gremlins" => Some(Subtype::Gremlin),
        "griffin" => Some(Subtype::Griffin),
        "hag" => Some(Subtype::Hag),
        "halfling" => Some(Subtype::Halfling),
        "harpy" => Some(Subtype::Harpy),
        "hippo" => Some(Subtype::Hippo),
        "horror" => Some(Subtype::Horror),
        "homunculus" | "homunculi" => Some(Subtype::Homunculus),
        "horse" => Some(Subtype::Horse),
        "hound" => Some(Subtype::Hound),
        "human" => Some(Subtype::Human),
        "hydra" => Some(Subtype::Hydra),
        "illusion" => Some(Subtype::Illusion),
        "imp" => Some(Subtype::Imp),
        "insect" => Some(Subtype::Insect),
        "inkling" | "inklings" => Some(Subtype::Inkling),
        "jellyfish" => Some(Subtype::Jellyfish),
        "kavu" => Some(Subtype::Kavu),
        "kirin" => Some(Subtype::Kirin),
        "kithkin" => Some(Subtype::Kithkin),
        "knight" => Some(Subtype::Knight),
        "kobold" => Some(Subtype::Kobold),
        "kor" => Some(Subtype::Kor),
        "kraken" => Some(Subtype::Kraken),
        "leviathan" => Some(Subtype::Leviathan),
        "lizard" => Some(Subtype::Lizard),
        "manticore" => Some(Subtype::Manticore),
        "mercenary" => Some(Subtype::Mercenary),
        "merfolk" => Some(Subtype::Merfolk),
        "minion" => Some(Subtype::Minion),
        "mite" | "mites" => Some(Subtype::Mite),
        "minotaur" => Some(Subtype::Minotaur),
        "mole" => Some(Subtype::Mole),
        "monk" => Some(Subtype::Monk),
        "monkey" | "monkeys" => Some(Subtype::Monkey),
        "moonfolk" => Some(Subtype::Moonfolk),
        "mount" | "mounts" => Some(Subtype::Mount),
        "mouse" | "mice" => Some(Subtype::Mouse),
        "mutant" => Some(Subtype::Mutant),
        "myr" => Some(Subtype::Myr),
        "naga" => Some(Subtype::Naga),
        "necron" | "necrons" => Some(Subtype::Necron),
        "nightmare" => Some(Subtype::Nightmare),
        "ninja" => Some(Subtype::Ninja),
        "noble" => Some(Subtype::Noble),
        "octopus" | "octopuses" => Some(Subtype::Octopus),
        "ogre" => Some(Subtype::Ogre),
        "ooze" => Some(Subtype::Ooze),
        "orc" => Some(Subtype::Orc),
        "otter" => Some(Subtype::Otter),
        "ox" => Some(Subtype::Ox),
        "oyster" => Some(Subtype::Oyster),
        "peasant" => Some(Subtype::Peasant),
        "pest" => Some(Subtype::Pest),
        "pegasus" => Some(Subtype::Pegasus),
        "phyrexian" => Some(Subtype::Phyrexian),
        "phoenix" => Some(Subtype::Phoenix),
        "pincher" | "pinchers" => Some(Subtype::Pincher),
        "pilot" => Some(Subtype::Pilot),
        "pirate" => Some(Subtype::Pirate),
        "plant" => Some(Subtype::Plant),
        "praetor" => Some(Subtype::Praetor),
        "raccoon" => Some(Subtype::Raccoon),
        "rabbit" => Some(Subtype::Rabbit),
        "rat" => Some(Subtype::Rat),
        "reflection" => Some(Subtype::Reflection),
        "rebel" => Some(Subtype::Rebel),
        "rhino" => Some(Subtype::Rhino),
        "rogue" => Some(Subtype::Rogue),
        "robot" => Some(Subtype::Robot),
        "salamander" => Some(Subtype::Salamander),
        "saproling" | "saprolings" => Some(Subtype::Saproling),
        "samurai" => Some(Subtype::Samurai),
        "satyr" => Some(Subtype::Satyr),
        "scarecrow" => Some(Subtype::Scarecrow),
        "scout" => Some(Subtype::Scout),
        "servo" | "servos" => Some(Subtype::Servo),
        "serpent" => Some(Subtype::Serpent),
        "shade" => Some(Subtype::Shade),
        "shaman" => Some(Subtype::Shaman),
        "shapeshifter" => Some(Subtype::Shapeshifter),
        "shark" => Some(Subtype::Shark),
        "sheep" => Some(Subtype::Sheep),
        "skeleton" => Some(Subtype::Skeleton),
        "slith" => Some(Subtype::Slith),
        "sliver" => Some(Subtype::Sliver),
        "slug" => Some(Subtype::Slug),
        "snake" => Some(Subtype::Snake),
        "soldier" => Some(Subtype::Soldier),
        "sorcerer" => Some(Subtype::Sorcerer),
        "spacecraft" => Some(Subtype::Spacecraft),
        "sphinx" => Some(Subtype::Sphinx),
        "specter" => Some(Subtype::Specter),
        "spider" => Some(Subtype::Spider),
        "spike" => Some(Subtype::Spike),
        "splinter" | "splinters" => Some(Subtype::Splinter),
        "spirit" => Some(Subtype::Spirit),
        "sponge" => Some(Subtype::Sponge),
        "squid" => Some(Subtype::Squid),
        "squirrel" => Some(Subtype::Squirrel),
        "starfish" => Some(Subtype::Starfish),
        "surrakar" => Some(Subtype::Surrakar),
        "thopter" => Some(Subtype::Thopter),
        "thrull" => Some(Subtype::Thrull),
        "tiefling" => Some(Subtype::Tiefling),
        "tentacle" | "tentacles" => Some(Subtype::Tentacle),
        "toy" => Some(Subtype::Toy),
        "treefolk" => Some(Subtype::Treefolk),
        "triskelavite" | "triskelavites" => Some(Subtype::Triskelavite),
        "trilobite" => Some(Subtype::Trilobite),
        "troll" => Some(Subtype::Troll),
        "turtle" => Some(Subtype::Turtle),
        "unicorn" => Some(Subtype::Unicorn),
        "vampire" => Some(Subtype::Vampire),
        "vedalken" => Some(Subtype::Vedalken),
        "viashino" => Some(Subtype::Viashino),
        "villain" | "villains" => Some(Subtype::Villain),
        "wall" => Some(Subtype::Wall),
        "warlock" => Some(Subtype::Warlock),
        "warrior" => Some(Subtype::Warrior),
        "weird" => Some(Subtype::Weird),
        "werewolf" | "werewolves" => Some(Subtype::Werewolf),
        "whale" => Some(Subtype::Whale),
        "wizard" => Some(Subtype::Wizard),
        "wolf" => Some(Subtype::Wolf),
        "wolverine" => Some(Subtype::Wolverine),
        "wombat" => Some(Subtype::Wombat),
        "worm" => Some(Subtype::Worm),
        "wraith" => Some(Subtype::Wraith),
        "wurm" => Some(Subtype::Wurm),
        "yeti" => Some(Subtype::Yeti),
        "zombie" => Some(Subtype::Zombie),
        "zubera" => Some(Subtype::Zubera),
        "clue" => Some(Subtype::Clue),
        "contraption" => Some(Subtype::Contraption),
        "equipment" => Some(Subtype::Equipment),
        "food" => Some(Subtype::Food),
        "fortification" => Some(Subtype::Fortification),
        "gold" => Some(Subtype::Gold),
        "treasure" => Some(Subtype::Treasure),
        "vehicle" => Some(Subtype::Vehicle),
        "aura" => Some(Subtype::Aura),
        "background" => Some(Subtype::Background),
        "cartouche" => Some(Subtype::Cartouche),
        "class" => Some(Subtype::Class),
        "curse" => Some(Subtype::Curse),
        "role" => Some(Subtype::Role),
        "rune" => Some(Subtype::Rune),
        "saga" => Some(Subtype::Saga),
        "shard" => Some(Subtype::Shard),
        "shrine" => Some(Subtype::Shrine),
        "adventure" => Some(Subtype::Adventure),
        "arcane" => Some(Subtype::Arcane),
        "lesson" => Some(Subtype::Lesson),
        "trap" => Some(Subtype::Trap),
        "ajani" => Some(Subtype::Ajani),
        "ashiok" => Some(Subtype::Ashiok),
        "chandra" => Some(Subtype::Chandra),
        "elspeth" => Some(Subtype::Elspeth),
        "garruk" => Some(Subtype::Garruk),
        "gideon" => Some(Subtype::Gideon),
        "jace" => Some(Subtype::Jace),
        "karn" => Some(Subtype::Karn),
        "liliana" => Some(Subtype::Liliana),
        "nissa" => Some(Subtype::Nissa),
        "sorin" => Some(Subtype::Sorin),
        "teferi" => Some(Subtype::Teferi),
        "ugin" => Some(Subtype::Ugin),
        "vraska" => Some(Subtype::Vraska),
        _ => None,
    }
}

fn parse_power_toughness(raw: &str) -> Option<PowerToughness> {
    let trimmed = raw.trim();
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() != 2 {
        return None;
    }

    let power = parse_pt_value(parts[0].trim())?;
    let toughness = parse_pt_value(parts[1].trim())?;
    Some(PowerToughness::new(power, toughness))
}

fn parse_pt_value(raw: &str) -> Option<PtValue> {
    if raw == ".5" || raw == "0.5" {
        return Some(PtValue::Fixed(0));
    }
    if raw == "*" {
        return Some(PtValue::Star);
    }
    if let Some(stripped) = raw.strip_prefix("*+") {
        let value = stripped.trim().parse::<i32>().ok()?;
        return Some(PtValue::StarPlus(value));
    }
    if let Some(stripped) = raw.strip_suffix("+*") {
        let value = stripped.trim().parse::<i32>().ok()?;
        return Some(PtValue::StarPlus(value));
    }
    if let Ok(value) = raw.parse::<i32>() {
        return Some(PtValue::Fixed(value));
    }
    None
}

fn parse_for_each_opponent_doesnt(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let mut clause_tokens = tokens;
    let mut clause_words = words(clause_tokens);
    if clause_words.first().copied() == Some("then") {
        clause_tokens = &clause_tokens[1..];
        clause_words = words(clause_tokens);
    }
    if clause_words.len() < 4 {
        return Ok(None);
    }

    let start = if clause_words.starts_with(&["for", "each", "opponent"])
        || clause_words.starts_with(&["for", "each", "opponents"])
    {
        3
    } else if clause_words.starts_with(&["each", "opponent"])
        || clause_words.starts_with(&["each", "opponents"])
    {
        2
    } else {
        return Ok(None);
    };

    let inner_tokens = trim_commas(&clause_tokens[start..]);
    let inner_words = words(&inner_tokens);
    let starts_with_who = inner_words.first().copied() == Some("who");
    let Some((negation_idx, negation_len)) = negated_action_word_index(&inner_words) else {
        return Ok(None);
    };
    if !starts_with_who {
        return Ok(None);
    }

    let effect_token_start = if let Some(comma_idx) = inner_tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)))
    {
        comma_idx + 1
    } else if let Some(this_way_idx) = inner_words
        .windows(2)
        .position(|pair| pair == ["this", "way"])
    {
        token_index_for_word_index(&inner_tokens, this_way_idx + 2).unwrap_or(inner_tokens.len())
    } else {
        token_index_for_word_index(&inner_tokens, negation_idx + negation_len)
            .unwrap_or(inner_tokens.len())
    };
    let effect_tokens = trim_commas(&inner_tokens[effect_token_start..]);
    if effect_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing effect in for each opponent who doesn't clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let effects = parse_effect_chain(&effect_tokens)?;
    Ok(Some(EffectAst::ForEachOpponentDoesNot { effects }))
}

fn parse_for_each_player_doesnt(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let mut clause_tokens = tokens;
    let mut clause_words = words(clause_tokens);
    if clause_words.first().copied() == Some("then") {
        clause_tokens = &clause_tokens[1..];
        clause_words = words(clause_tokens);
    }
    if clause_words.len() < 5 {
        return Ok(None);
    }

    let start = if clause_words.starts_with(&["for", "each", "player"])
        || clause_words.starts_with(&["for", "each", "players"])
    {
        3
    } else if clause_words.starts_with(&["each", "player"])
        || clause_words.starts_with(&["each", "players"])
    {
        2
    } else {
        return Ok(None);
    };

    let inner_tokens = trim_commas(&clause_tokens[start..]);
    let inner_words = words(&inner_tokens);
    let starts_with_who = inner_words.first().copied() == Some("who");
    let Some((negation_idx, negation_len)) = negated_action_word_index(&inner_words) else {
        return Ok(None);
    };
    if !starts_with_who {
        return Ok(None);
    }

    let effect_token_start = if let Some(comma_idx) = inner_tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)))
    {
        comma_idx + 1
    } else if let Some(this_way_idx) = inner_words
        .windows(2)
        .position(|pair| pair == ["this", "way"])
    {
        token_index_for_word_index(&inner_tokens, this_way_idx + 2).unwrap_or(inner_tokens.len())
    } else {
        token_index_for_word_index(&inner_tokens, negation_idx + negation_len)
            .unwrap_or(inner_tokens.len())
    };

    let effect_tokens = trim_commas(&inner_tokens[effect_token_start..]);
    if effect_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing effect in for each player who doesn't clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let effects = parse_effect_chain(&effect_tokens)?;
    Ok(Some(EffectAst::ForEachPlayerDoesNot { effects }))
}

fn negated_action_word_index(words: &[&str]) -> Option<(usize, usize)> {
    if let Some(idx) = words
        .iter()
        .position(|word| *word == "doesnt" || *word == "didnt")
    {
        return Some((idx, 1));
    }
    for (idx, pair) in words.windows(2).enumerate() {
        if pair == ["do", "not"] || pair == ["did", "not"] {
            return Some((idx, 2));
        }
    }
    None
}

fn parse_vote_start_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words = words(tokens);
    let vote_idx = words
        .iter()
        .position(|word| *word == "vote" || *word == "votes");
    let Some(vote_idx) = vote_idx else {
        return Ok(None);
    };

    let has_each = words[..vote_idx].contains(&"each");
    let has_player = words[..vote_idx]
        .iter()
        .any(|word| *word == "player" || *word == "players");
    if !has_each || !has_player {
        return Ok(None);
    }

    let for_idx = words
        .iter()
        .position(|word| *word == "for")
        .ok_or_else(|| CardTextError::ParseError("missing 'for' in vote clause".to_string()))?;
    if for_idx < vote_idx {
        return Ok(None);
    }

    let option_words = &words[for_idx + 1..];
    let mut options = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for word in option_words {
        if *word == "or" {
            if !current.is_empty() {
                options.push(current.join(" "));
                current.clear();
            }
            continue;
        }
        if is_article(word) {
            continue;
        }
        current.push(word);
    }
    if !current.is_empty() {
        options.push(current.join(" "));
    }

    if options.len() < 2 {
        return Err(CardTextError::ParseError(
            "vote clause requires at least two options".to_string(),
        ));
    }

    Ok(Some(EffectAst::VoteStart { options }))
}

fn parse_for_each_vote_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words = words(tokens);
    if words.len() < 4 {
        return Ok(None);
    }

    if !words.starts_with(&["for", "each"]) {
        return Ok(None);
    }

    let vote_idx = words
        .iter()
        .position(|word| *word == "vote" || *word == "votes");
    let Some(vote_idx) = vote_idx else {
        return Ok(None);
    };
    if vote_idx <= 2 {
        return Err(CardTextError::ParseError(
            "missing vote option name".to_string(),
        ));
    }

    let option_words: Vec<&str> = words[2..vote_idx]
        .iter()
        .copied()
        .filter(|word| !is_article(word))
        .collect();
    if option_words.is_empty() {
        return Err(CardTextError::ParseError(
            "missing vote option name".to_string(),
        ));
    }
    let option = option_words.join(" ");

    let comma_idx = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)))
        .ok_or_else(|| {
            CardTextError::ParseError("missing comma in for each vote clause".to_string())
        })?;

    let effect_tokens = &tokens[comma_idx + 1..];
    let effects = parse_effect_chain(effect_tokens)?;
    Ok(Some(EffectAst::VoteOption { option, effects }))
}

fn parse_vote_extra_sentence(tokens: &[Token]) -> Option<EffectAst> {
    let words = words(tokens);
    if words.len() < 3 || words.first().copied() != Some("you") {
        return None;
    }

    let has_vote = words.iter().any(|word| *word == "vote" || *word == "votes");
    let has_additional = words.contains(&"additional");
    let has_time = words.iter().any(|word| *word == "time" || *word == "times");
    if !has_vote || !has_additional || !has_time {
        return None;
    }

    let optional = words.contains(&"may");
    Some(EffectAst::VoteExtra { count: 1, optional })
}

fn parse_after_turn_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let line_words = words(tokens);
    if line_words.len() < 3
        || line_words[0] != "after"
        || line_words[1] != "that"
        || line_words[2] != "turn"
    {
        return Ok(None);
    }

    let comma_idx = tokens
        .iter()
        .position(|token| matches!(token, Token::Comma(_)));
    let remainder = if let Some(idx) = comma_idx {
        &tokens[idx + 1..]
    } else {
        &tokens[3..]
    };

    let remaining_words: Vec<&str> = words(remainder)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();

    if remaining_words.len() < 4 {
        return Err(CardTextError::ParseError(
            "unsupported after turn clause".to_string(),
        ));
    }

    let player = if remaining_words.starts_with(&["that", "player"]) {
        PlayerAst::That
    } else if remaining_words.starts_with(&["target", "player"]) {
        PlayerAst::Target
    } else if remaining_words.starts_with(&["you"]) {
        PlayerAst::You
    } else {
        return Err(CardTextError::ParseError(
            "unsupported after turn player".to_string(),
        ));
    };

    if remaining_words.contains(&"extra") && remaining_words.contains(&"turn") {
        return Ok(Some(EffectAst::ExtraTurnAfterTurn { player }));
    }

    Err(CardTextError::ParseError(
        "unsupported after turn clause".to_string(),
    ))
}

fn parse_conditional_sentence(tokens: &[Token]) -> Result<Vec<EffectAst>, CardTextError> {
    let comma_indices = tokens
        .iter()
        .enumerate()
        .filter_map(|(idx, token)| matches!(token, Token::Comma(_)).then_some(idx))
        .collect::<Vec<_>>();
    if comma_indices.is_empty() {
        return Err(CardTextError::ParseError(
            "missing comma in if clause".to_string(),
        ));
    }

    // For result predicates ("if you do, ..."), always split at the first comma.
    // The effect tail frequently contains additional commas (search/reveal/put, etc.)
    // that should stay in the true branch.
    let first_comma_idx = comma_indices[0];
    if first_comma_idx > 1 {
        let predicate_tokens = &tokens[1..first_comma_idx];
        if let Some(predicate) = parse_if_result_predicate(predicate_tokens) {
            let effect_tokens = &tokens[first_comma_idx + 1..];
            let effects = parse_effect_chain(effect_tokens)?;
            return Ok(vec![EffectAst::IfResult { predicate, effects }]);
        }
        if let Ok(predicate) = parse_predicate(predicate_tokens) {
            let effect_tokens = &tokens[first_comma_idx + 1..];
            let comma_fragment_looks_like_effect = if comma_indices.len() > 1 {
                let fragment_tokens = &tokens[first_comma_idx + 1..comma_indices[1]];
                parse_effect_chain(fragment_tokens)
                    .map(|effects| !effects.is_empty())
                    .unwrap_or(false)
            } else {
                true
            };
            if comma_fragment_looks_like_effect
                && let Ok(effects) = parse_effect_chain(effect_tokens)
                && !effects.is_empty()
            {
                return Ok(vec![EffectAst::Conditional {
                    predicate,
                    if_true: effects,
                    if_false: Vec::new(),
                }]);
            }
        }
    }

    // Prefer the rightmost comma that yields a parseable effect clause so
    // predicates like "if it's an artifact, creature, enchantment, or land card,"
    // keep their internal comma-separated type list intact.
    let mut split: Option<(usize, Vec<EffectAst>)> = None;
    for idx in comma_indices.iter().rev().copied() {
        let effect_tokens = &tokens[idx + 1..];
        if effect_tokens.is_empty() {
            continue;
        }
        if let Ok(effects) = parse_effect_chain(effect_tokens)
            && !effects.is_empty()
        {
            split = Some((idx, effects));
            break;
        }
    }

    let (comma_idx, effects) = if let Some(split) = split {
        split
    } else {
        let first_idx = comma_indices[0];
        let effect_tokens = &tokens[first_idx + 1..];
        (first_idx, parse_effect_chain(effect_tokens)?)
    };
    let predicate_tokens = &tokens[1..comma_idx];

    if let Some(predicate) = parse_if_result_predicate(predicate_tokens) {
        return Ok(vec![EffectAst::IfResult { predicate, effects }]);
    }

    let predicate = parse_predicate(predicate_tokens)?;
    Ok(vec![EffectAst::Conditional {
        predicate,
        if_true: effects,
        if_false: Vec::new(),
    }])
}

fn parse_if_result_predicate(tokens: &[Token]) -> Option<IfResultPredicate> {
    let words: Vec<&str> = words(tokens)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();

    if words.len() >= 2 && words[0] == "you" && words[1] == "do" {
        return Some(IfResultPredicate::Did);
    }
    if words.len() >= 2
        && words[0] == "you"
        && (words[1] == "win" || words[1] == "won")
        && (words.len() == 2 || words.iter().any(|word| *word == "clash"))
    {
        return Some(IfResultPredicate::Did);
    }
    if words.len() >= 2 && words[0] == "they" && words[1] == "do" {
        return Some(IfResultPredicate::Did);
    }
    if words.len() >= 4
        && words[0] == "you"
        && matches!(
            words[1],
            "remove"
                | "removed"
                | "sacrifice"
                | "sacrificed"
                | "discard"
                | "discarded"
                | "exile"
                | "exiled"
        )
        && words[words.len() - 2] == "this"
        && words[words.len() - 1] == "way"
    {
        return Some(IfResultPredicate::Did);
    }
    if words.len() >= 4
        && words[0] == "they"
        && matches!(
            words[1],
            "remove"
                | "removed"
                | "sacrifice"
                | "sacrificed"
                | "discard"
                | "discarded"
                | "exile"
                | "exiled"
        )
        && words[words.len() - 2] == "this"
        && words[words.len() - 1] == "way"
    {
        return Some(IfResultPredicate::Did);
    }

    if words.len() >= 5
        && (words[0] == "that" || words[0] == "it")
        && words[1] == "spell"
        && words.iter().any(|word| *word == "countered")
        && words[words.len() - 2] == "this"
        && words[words.len() - 1] == "way"
    {
        return Some(IfResultPredicate::Did);
    }

    if words.len() >= 5
        && (words[0] == "that" || words[0] == "it")
        && (words[1] == "creature" || words[1] == "permanent" || words[1] == "card")
        && words[2] == "dies"
        && words[3] == "this"
        && words[4] == "way"
    {
        return Some(IfResultPredicate::DiesThisWay);
    }

    if words.len() >= 2 && words[0] == "you" && (words[1] == "dont" || words[1] == "do") {
        if words.len() >= 3 && words[2] == "not" {
            return Some(IfResultPredicate::DidNot);
        }
        if words[1] == "dont" {
            return Some(IfResultPredicate::DidNot);
        }
    }
    if words.len() >= 2 && words[0] == "you" && words[1] == "cant" {
        return Some(IfResultPredicate::DidNot);
    }
    if words.len() >= 3 && words[0] == "you" && words[1] == "can" && words[2] == "not" {
        return Some(IfResultPredicate::DidNot);
    }
    if words.len() >= 2 && words[0] == "they" && (words[1] == "dont" || words[1] == "do") {
        if words.len() >= 3 && words[2] == "not" {
            return Some(IfResultPredicate::DidNot);
        }
        if words[1] == "dont" {
            return Some(IfResultPredicate::DidNot);
        }
    }
    if words.len() >= 2 && words[0] == "they" && words[1] == "cant" {
        return Some(IfResultPredicate::DidNot);
    }
    if words.len() >= 3 && words[0] == "they" && words[1] == "can" && words[2] == "not" {
        return Some(IfResultPredicate::DidNot);
    }

    None
}

fn parse_predicate(tokens: &[Token]) -> Result<PredicateAst, CardTextError> {
    let mut filtered: Vec<&str> = words(tokens)
        .into_iter()
        .filter(|word| !is_article(word) && *word != "is")
        .collect();

    if filtered.is_empty() {
        return Err(CardTextError::ParseError(
            "empty predicate in if clause".to_string(),
        ));
    }

    if let Some(predicate) = parse_graveyard_threshold_predicate(&filtered)? {
        return Ok(predicate);
    }

    // Handle simple conjunction predicates like "... and have no cards in hand".
    if let Some(and_idx) = filtered.iter().position(|word| *word == "and")
        && and_idx > 0
        && and_idx + 1 < filtered.len()
    {
        let right_first = filtered.get(and_idx + 1).copied();
        if matches!(right_first, Some("have") | Some("you")) {
            let left_words = &filtered[..and_idx];
            let mut right_words = filtered[and_idx + 1..].to_vec();
            // Inherit the subject when omitted ("... and have ...").
            if right_words.first().copied() == Some("have") {
                right_words.insert(0, "you");
            }
            let left_tokens = left_words
                .iter()
                .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
                .collect::<Vec<_>>();
            let right_tokens = right_words
                .iter()
                .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
                .collect::<Vec<_>>();
            let left = parse_predicate(&left_tokens)?;
            let right = parse_predicate(&right_tokens)?;
            return Ok(PredicateAst::And(Box::new(left), Box::new(right)));
        }
    }

    if filtered.as_slice() == ["this", "tapped"]
        || filtered.as_slice() == ["thiss", "tapped"]
        || ((filtered.first().copied() == Some("this")
            || filtered.first().copied() == Some("thiss"))
            && filtered.last().copied() == Some("tapped"))
    {
        return Ok(PredicateAst::SourceIsTapped);
    }

    if filtered.starts_with(&["there", "are", "no"])
        && filtered.contains(&"counters")
        && filtered.windows(2).any(|window| window == ["on", "this"])
        && let Some(counters_idx) = filtered.iter().position(|word| *word == "counters")
        && counters_idx >= 4
        && let Some(counter_type) = parse_counter_type_word(filtered[counters_idx - 1])
    {
        return Ok(PredicateAst::SourceHasNoCounter(counter_type));
    }

    let raw_words = words(tokens);
    if raw_words.starts_with(&["there", "are"])
        && raw_words.get(3).copied() == Some("or")
        && raw_words.get(4).copied() == Some("more")
        && raw_words
            .iter()
            .any(|w| *w == "counter" || *w == "counters")
    {
        if let Some((count, used)) = parse_number(&tokens[2..]) {
            let rest = &tokens[2 + used..];
            let rest_words = words(rest);
            // Pattern: "there are <N> or more <counter> counters on this <permanent>"
            if rest_words.len() >= 4
                && rest_words[0] == "or"
                && rest_words[1] == "more"
                && (rest_words[3] == "counter" || rest_words[3] == "counters")
                && let Some(counter_type) = parse_counter_type_word(rest_words[2])
            {
                return Ok(PredicateAst::SourceHasCounterAtLeast {
                    counter_type,
                    count,
                });
            }
        }
    }

    // "there are N or more basic land types among lands that player controls"
    if filtered.len() >= 13
        && filtered[0] == "there"
        && filtered[1] == "are"
        && filtered.get(3).copied() == Some("or")
        && filtered.get(4).copied() == Some("more")
        && filtered.get(5).copied() == Some("basic")
        && filtered.get(6).copied() == Some("land")
        && matches!(filtered.get(7).copied(), Some("type" | "types"))
        && filtered.get(8).copied() == Some("among")
        && matches!(filtered.get(9).copied(), Some("land" | "lands"))
    {
        let Some(count) = parse_named_number(filtered[2]) else {
            return Err(CardTextError::ParseError(format!(
                "unsupported basic-land-types predicate count (predicate: '{}')",
                filtered.join(" ")
            )));
        };

        let tail = &filtered[10..];
        let player = if tail == ["that", "player", "controls"]
            || tail == ["that", "player", "control"]
            || tail == ["that", "players", "controls"]
        {
            PlayerAst::That
        } else if tail == ["you", "control"] || tail == ["you", "controls"] {
            PlayerAst::You
        } else {
            return Err(CardTextError::ParseError(format!(
                "unsupported basic-land-types predicate tail (predicate: '{}')",
                filtered.join(" ")
            )));
        };

        return Ok(PredicateAst::PlayerControlsBasicLandTypesAmongLandsOrMore { player, count });
    }

    let parse_graveyard_card_types_subject = |words: &[&str]| -> Option<PlayerAst> {
        match words {
            [first, second] if *first == "your" && *second == "graveyard" => Some(PlayerAst::You),
            [first, second, third]
                if *first == "that"
                    && (*second == "player" || *second == "players")
                    && *third == "graveyard" =>
            {
                Some(PlayerAst::That)
            }
            [first, second, third]
                if *first == "target"
                    && (*second == "player" || *second == "players")
                    && *third == "graveyard" =>
            {
                Some(PlayerAst::Target)
            }
            [first, second, third]
                if *first == "target"
                    && (*second == "opponent" || *second == "opponents")
                    && *third == "graveyard" =>
            {
                Some(PlayerAst::TargetOpponent)
            }
            [first, second] if (*first == "opponent" || *first == "opponents") && *second == "graveyard" => {
                Some(PlayerAst::Opponent)
            }
            _ => None,
        }
    };
    if filtered.len() >= 11 {
        let (count_idx, subject_start, constrained_player) =
            if filtered[0] == "there" && filtered[1] == "are" {
                (2usize, 10usize, None)
            } else if filtered[0] == "you" && filtered[1] == "have" {
                (2usize, 10usize, Some(PlayerAst::You))
            } else {
                (usize::MAX, usize::MAX, None)
            };
        if count_idx != usize::MAX
            && filtered.get(count_idx + 1).copied() == Some("or")
            && filtered.get(count_idx + 2).copied() == Some("more")
            && filtered.get(count_idx + 3).copied() == Some("card")
            && matches!(filtered.get(count_idx + 4).copied(), Some("type" | "types"))
            && filtered.get(count_idx + 5).copied() == Some("among")
            && matches!(filtered.get(count_idx + 6).copied(), Some("card" | "cards"))
            && filtered.get(count_idx + 7).copied() == Some("in")
            && subject_start <= filtered.len()
            && let Some(count) = parse_named_number(filtered[count_idx])
            && let Some(player) = parse_graveyard_card_types_subject(&filtered[subject_start..])
            && constrained_player.map_or(true, |expected| expected == player)
        {
            return Ok(PredicateAst::PlayerHasCardTypesInGraveyardOrMore { player, count });
        }
    }

    let parse_cards_in_hand_subject = |words: &[&str]| -> Option<(PlayerAst, usize)> {
        match words {
            [first, second, ..] if *first == "that" && *second == "player" => {
                Some((PlayerAst::That, 2))
            }
            [first, second, ..] if *first == "target" && *second == "player" => {
                Some((PlayerAst::Target, 2))
            }
            [first, second, ..] if *first == "target" && *second == "opponent" => {
                Some((PlayerAst::TargetOpponent, 2))
            }
            [first, second, ..] if *first == "each" && *second == "opponent" => {
                Some((PlayerAst::Opponent, 2))
            }
            [first, ..] if *first == "you" => Some((PlayerAst::You, 1)),
            [first, ..] if *first == "opponent" || *first == "opponents" => {
                Some((PlayerAst::Opponent, 1))
            }
            [first, second, ..] if *first == "player" && *second == "who" => {
                Some((PlayerAst::That, 1))
            }
            _ => None,
        }
    };
    if let Some((player, subject_len)) = parse_cards_in_hand_subject(&filtered)
        && filtered.get(subject_len).copied() == Some("has")
        && let Some(count_word) = filtered.get(subject_len + 1).copied()
        && let Some(count) = parse_named_number(count_word)
        && filtered.get(subject_len + 2).copied() == Some("or")
        && let Some(comp_word) = filtered.get(subject_len + 3).copied()
        && matches!(comp_word, "more" | "fewer" | "less")
        && matches!(filtered.get(subject_len + 4).copied(), Some("card" | "cards"))
        && filtered.get(subject_len + 5).copied() == Some("in")
        && filtered.get(subject_len + 6).copied() == Some("hand")
        && filtered.len() == subject_len + 7
    {
        return Ok(if comp_word == "more" {
            PredicateAst::PlayerCardsInHandOrMore { player, count }
        } else {
            PredicateAst::PlayerCardsInHandOrFewer { player, count }
        });
    }

    if filtered.as_slice() == ["you", "have", "no", "cards", "in", "hand"] {
        return Ok(PredicateAst::YouHaveNoCardsInHand);
    }

    if matches!(
        filtered.as_slice(),
        ["it", "your", "turn"] | ["its", "your", "turn"] | ["your", "turn"]
    ) {
        return Ok(PredicateAst::YourTurn);
    }

    if matches!(
        filtered.as_slice(),
        ["creature", "died", "this", "turn"] | ["creatures", "died", "this", "turn"]
    ) {
        return Ok(PredicateAst::CreatureDiedThisTurn);
    }

    if filtered.as_slice() == ["you", "attacked", "this", "turn"] {
        return Ok(PredicateAst::YouAttackedThisTurn);
    }

    if filtered.as_slice() == ["no", "spells", "were", "cast", "last", "turn"]
        || filtered.as_slice() == ["no", "spell", "was", "cast", "last", "turn"]
    {
        return Ok(PredicateAst::NoSpellsWereCastLastTurn);
    }
    if filtered.as_slice() == ["this", "spell", "was", "kicked"] {
        return Ok(PredicateAst::ThisSpellWasKicked);
    }
    if filtered.as_slice() == ["it", "was", "kicked"]
        || filtered.as_slice() == ["that", "was", "kicked"]
    {
        return Ok(PredicateAst::TargetWasKicked);
    }
    if filtered.as_slice() == ["its", "controller", "poisoned"]
        || filtered.as_slice() == ["that", "spells", "controller", "poisoned"]
    {
        return Ok(PredicateAst::TargetSpellControllerIsPoisoned);
    }
    if filtered.as_slice() == ["no", "mana", "was", "spent", "to", "cast", "it"]
        || filtered.as_slice() == ["no", "mana", "were", "spent", "to", "cast", "it"]
        || filtered.as_slice() == ["no", "mana", "was", "spent", "to", "cast", "that", "spell"]
        || filtered.as_slice() == ["no", "mana", "were", "spent", "to", "cast", "that", "spell"]
    {
        return Ok(PredicateAst::TargetSpellNoManaSpentToCast);
    }
    if filtered.as_slice()
        == [
            "you",
            "control",
            "more",
            "creatures",
            "than",
            "that",
            "spells",
            "controller",
        ]
        || filtered.as_slice()
            == [
                "you",
                "control",
                "more",
                "creatures",
                "than",
                "its",
                "controller",
            ]
    {
        return Ok(PredicateAst::YouControlMoreCreaturesThanTargetSpellController);
    }
    if filtered.len() == 7
        && matches!(filtered[0], "w" | "u" | "b" | "r" | "g" | "c")
        && filtered[1] == "was"
        && filtered[2] == "spent"
        && filtered[3] == "to"
        && filtered[4] == "cast"
        && filtered[5] == "this"
        && filtered[6] == "spell"
        && let Ok(symbol) = parse_mana_symbol(filtered[0])
    {
        return Ok(PredicateAst::ManaSpentToCastThisSpellAtLeast {
            amount: 1,
            symbol: Some(symbol),
        });
    }

    if let Some((amount, symbol)) = parse_mana_spent_to_cast_predicate(&filtered) {
        return Ok(PredicateAst::ManaSpentToCastThisSpellAtLeast { amount, symbol });
    }

    if filtered[0] == "its" {
        filtered[0] = "it";
    }

    if filtered.len() >= 2 {
        let tag = if filtered.starts_with(&["equipped", "creature"]) {
            Some("equipped")
        } else if filtered.starts_with(&["enchanted", "creature"]) {
            Some("enchanted")
        } else {
            None
        };
        if let Some(tag) = tag {
            let remainder = filtered[2..].to_vec();
            let tokens = remainder
                .iter()
                .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
                .collect::<Vec<_>>();
            let mut filter = parse_object_filter(&tokens, false)?;
            if filter.card_types.is_empty() {
                filter.card_types.push(CardType::Creature);
            }
            return Ok(PredicateAst::TaggedMatches(TagKey::from(tag), filter));
        }
    }

    let is_it = filtered.first().is_some_and(|word| *word == "it");
    let has_card = filtered.contains(&"card");

    if is_it {
        if filtered.len() >= 3 && filtered[1] == "mana" && filtered[2] == "value" {
            let mana_value_tail = &filtered[3..];
            let compares_to_colors_spent = mana_value_tail
                == [
                    "less", "than", "or", "equal", "to", "number", "of", "colors", "of", "mana",
                    "spent", "to", "cast", "this", "spell",
                ]
                || mana_value_tail
                    == [
                        "less", "than", "or", "equal", "to", "number", "of", "color", "of", "mana",
                        "spent", "to", "cast", "this", "spell",
                    ];
            if compares_to_colors_spent {
                return Ok(PredicateAst::TargetManaValueLteColorsSpentToCastThisSpell);
            }

            if let Some((cmp, _consumed)) =
                parse_filter_comparison_tokens("mana value", mana_value_tail, &filtered)?
            {
                return Ok(PredicateAst::ItMatches(ObjectFilter {
                    mana_value: Some(cmp),
                    ..Default::default()
                }));
            }
        }

        if filtered.len() >= 3 && (filtered[1] == "power" || filtered[1] == "toughness") {
            let axis = filtered[1];
            let value_tail = &filtered[2..];
            if let Some((cmp, _consumed)) =
                parse_filter_comparison_tokens(axis, value_tail, &filtered)?
            {
                let mut filter = ObjectFilter::default();
                if axis == "power" {
                    filter.power = Some(cmp);
                } else {
                    filter.toughness = Some(cmp);
                }
                return Ok(PredicateAst::ItMatches(filter));
            }
        }

        let mut card_types = Vec::new();
        for word in &filtered {
            if let Some(card_type) = parse_card_type(word)
                && !card_types.contains(&card_type)
            {
                card_types.push(card_type);
            }
        }
        let mut subtypes = Vec::new();
        for word in &filtered {
            if let Some(subtype) = parse_subtype_word(word)
                .or_else(|| word.strip_suffix('s').and_then(parse_subtype_word))
                && !subtypes.contains(&subtype)
            {
                subtypes.push(subtype);
            }
        }
        if !card_types.is_empty() || !subtypes.is_empty() {
            if has_card && card_types.len() == 1 && card_types[0] == CardType::Land {
                return Ok(PredicateAst::ItIsLandCard);
            }
            return Ok(PredicateAst::ItMatches(ObjectFilter {
                card_types,
                subtypes,
                ..Default::default()
            }));
        }
    }

    if filtered.len() >= 3
        && filtered[0] == "you"
        && (filtered[1] == "control" || filtered[1] == "controls")
        && (filtered[2] == "no" || filtered[2] == "neither")
    {
        let control_tokens = filtered[3..]
            .iter()
            .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
            .collect::<Vec<_>>();
        if let Ok(mut filter) = parse_object_filter(&control_tokens, false) {
            filter.controller = Some(PlayerFilter::You);
            if filtered[2] == "neither" {
                filter = filter
                    .match_tagged(TagKey::from(IT_TAG), TaggedOpbjectRelation::IsTaggedObject);
            }
            return Ok(PredicateAst::PlayerControlsNo {
                player: PlayerAst::You,
                filter,
            });
        }
    }

    if filtered.len() >= 7
        && filtered[0] == "you"
        && (filtered[1] == "control" || filtered[1] == "controls")
        && let Some(or_idx) = filtered.iter().position(|word| *word == "or")
        && or_idx > 2
    {
        let left_tokens = filtered[2..or_idx]
            .iter()
            .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
            .collect::<Vec<_>>();
        let mut right_words = filtered[or_idx + 1..].to_vec();
        if right_words.first().copied() == Some("there") {
            right_words = right_words[1..].to_vec();
        }
        if right_words.contains(&"graveyard") && right_words.contains(&"your") {
            let right_tokens = right_words
                .iter()
                .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
                .collect::<Vec<_>>();
            if let (Ok(mut control_filter), Ok(mut graveyard_filter)) = (
                parse_object_filter(&left_tokens, false),
                parse_object_filter(&right_tokens, false),
            ) {
                control_filter.controller = Some(PlayerFilter::You);
                if graveyard_filter.zone.is_none() {
                    graveyard_filter.zone = Some(Zone::Graveyard);
                }
                if graveyard_filter.owner.is_none() {
                    graveyard_filter.owner = Some(PlayerFilter::You);
                }
                return Ok(PredicateAst::PlayerControlsOrHasCardInGraveyard {
                    player: PlayerAst::You,
                    control_filter,
                    graveyard_filter,
                });
            }
        }
    }

    if filtered.len() >= 3
        && filtered[0] == "you"
        && (filtered[1] == "control" || filtered[1] == "controls")
    {
        let mut filter_start = 2usize;
        let mut min_count: Option<u32> = None;
        let mut exact_count: Option<u32> = None;
        if let Some(raw_count) = filtered.get(2)
            && let Some(parsed_count) = parse_named_number(raw_count)
            && filtered.get(3).copied() == Some("or")
            && filtered.get(4).copied() == Some("more")
        {
            min_count = Some(parsed_count);
            filter_start = 5;
        } else if filtered.get(2).copied() == Some("exactly")
            && let Some(raw_count) = filtered.get(3)
            && let Some(parsed_count) = parse_named_number(raw_count)
        {
            exact_count = Some(parsed_count);
            filter_start = 4;
        } else if filtered.get(2).copied() == Some("at")
            && filtered.get(3).copied() == Some("least")
            && let Some(raw_count) = filtered.get(4)
            && let Some(parsed_count) = parse_named_number(raw_count)
        {
            min_count = Some(parsed_count);
            filter_start = 5;
        }

        let mut control_words = filtered[filter_start..].to_vec();
        let mut requires_different_powers = false;
        if control_words.ends_with(&["with", "different", "powers"])
            || control_words.ends_with(&["with", "different", "power"])
        {
            requires_different_powers = true;
            control_words.truncate(control_words.len().saturating_sub(3));
        }
        let control_tokens = control_words
            .iter()
            .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
            .collect::<Vec<_>>();
        let other = control_tokens
            .first()
            .is_some_and(|token| token.is_word("another") || token.is_word("other"));
        if let Ok(mut filter) = parse_object_filter(&control_tokens, other) {
            filter.controller = Some(PlayerFilter::You);
            if let Some(count) = exact_count {
                return Ok(PredicateAst::PlayerControlsExactly {
                    player: PlayerAst::You,
                    filter,
                    count,
                });
            }
            if let Some(count) = min_count
                && count > 1
            {
                if requires_different_powers {
                    return Ok(PredicateAst::PlayerControlsAtLeastWithDifferentPowers {
                        player: PlayerAst::You,
                        filter,
                        count,
                    });
                }
                return Ok(PredicateAst::PlayerControlsAtLeast {
                    player: PlayerAst::You,
                    filter,
                    count,
                });
            }
            return Ok(PredicateAst::PlayerControls {
                player: PlayerAst::You,
                filter,
            });
        }
    }

    if filtered.len() >= 4
        && filtered[0] == "that"
        && (filtered[1] == "player" || filtered[1] == "players")
        && (filtered[2] == "control" || filtered[2] == "controls")
    {
        let mut filter_start = 3usize;
        let mut min_count: Option<u32> = None;
        let mut exact_count: Option<u32> = None;
        if let Some(raw_count) = filtered.get(3)
            && let Some(parsed_count) = parse_named_number(raw_count)
            && filtered.get(4).copied() == Some("or")
            && filtered.get(5).copied() == Some("more")
        {
            min_count = Some(parsed_count);
            filter_start = 6;
        } else if filtered.get(3).copied() == Some("exactly")
            && let Some(raw_count) = filtered.get(4)
            && let Some(parsed_count) = parse_named_number(raw_count)
        {
            exact_count = Some(parsed_count);
            filter_start = 5;
        } else if filtered.get(3).copied() == Some("at")
            && filtered.get(4).copied() == Some("least")
            && let Some(raw_count) = filtered.get(5)
            && let Some(parsed_count) = parse_named_number(raw_count)
        {
            min_count = Some(parsed_count);
            filter_start = 6;
        }

        let control_tokens = filtered[filter_start..]
            .iter()
            .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
            .collect::<Vec<_>>();
        let other = control_tokens
            .first()
            .is_some_and(|token| token.is_word("another") || token.is_word("other"));
        if let Ok(filter) = parse_object_filter(&control_tokens, other) {
            if let Some(count) = exact_count {
                return Ok(PredicateAst::PlayerControlsExactly {
                    player: PlayerAst::That,
                    filter,
                    count,
                });
            }
            if let Some(count) = min_count
                && count > 1
            {
                return Ok(PredicateAst::PlayerControlsAtLeast {
                    player: PlayerAst::That,
                    filter,
                    count,
                });
            }
            return Ok(PredicateAst::PlayerControls {
                player: PlayerAst::That,
                filter,
            });
        }
    }

    Err(CardTextError::ParseError(format!(
        "unsupported predicate (predicate: '{}')",
        filtered.join(" ")
    )))
}

fn parse_graveyard_threshold_predicate(
    filtered: &[&str],
) -> Result<Option<PredicateAst>, CardTextError> {
    let (count, tail_start, constrained_player) = if filtered.len() >= 5
        && filtered[0] == "there"
        && filtered[1] == "are"
        && filtered[3] == "or"
        && filtered[4] == "more"
    {
        let Some(count) = parse_named_number(filtered[2]) else {
            return Ok(None);
        };
        (count, 5usize, None)
    } else if filtered.len() >= 5
        && filtered[0] == "you"
        && filtered[1] == "have"
        && filtered[3] == "or"
        && filtered[4] == "more"
    {
        let Some(count) = parse_named_number(filtered[2]) else {
            return Ok(None);
        };
        (count, 5usize, Some(PlayerAst::You))
    } else {
        return Ok(None);
    };

    let tail = &filtered[tail_start..];
    let Some(in_idx) = tail.iter().rposition(|word| *word == "in") else {
        return Ok(None);
    };
    if in_idx == 0 || in_idx + 1 >= tail.len() {
        return Ok(None);
    }

    let graveyard_owner_words = &tail[in_idx + 1..];
    let player = match graveyard_owner_words {
        ["your", "graveyard"] => PlayerAst::You,
        ["that", "player", "graveyard"] | ["that", "players", "graveyard"] => PlayerAst::That,
        ["target", "player", "graveyard"] | ["target", "players", "graveyard"] => {
            PlayerAst::Target
        }
        ["target", "opponent", "graveyard"] | ["target", "opponents", "graveyard"] => {
            PlayerAst::TargetOpponent
        }
        ["opponent", "graveyard"] | ["opponents", "graveyard"] => PlayerAst::Opponent,
        _ => return Ok(None),
    };
    if constrained_player.is_some_and(|expected| expected != player) {
        return Ok(None);
    }

    let raw_filter_words = &tail[..in_idx];
    if raw_filter_words.is_empty()
        || raw_filter_words.contains(&"type")
        || raw_filter_words.contains(&"types")
    {
        return Ok(None);
    }

    let mut normalized_filter_words = Vec::with_capacity(raw_filter_words.len());
    for (idx, word) in raw_filter_words.iter().enumerate() {
        // Normalize "instant and/or sorcery" -> "instant or sorcery".
        if *word == "and"
            && raw_filter_words
                .get(idx + 1)
                .is_some_and(|next| *next == "or")
        {
            continue;
        }
        normalized_filter_words.push(*word);
    }
    if normalized_filter_words.is_empty() {
        return Ok(None);
    }

    let mut filter = if matches!(
        normalized_filter_words.as_slice(),
        ["card"] | ["cards"]
    ) {
        ObjectFilter::default()
    } else {
        let filter_tokens = normalized_filter_words
            .iter()
            .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
            .collect::<Vec<_>>();
        let Ok(filter) = parse_object_filter(&filter_tokens, false) else {
            return Ok(None);
        };
        filter
    };
    filter.zone = Some(Zone::Graveyard);

    Ok(Some(PredicateAst::PlayerControlsAtLeast {
        player,
        filter,
        count,
    }))
}

fn parse_sentence_counter_target_spell_if_it_was_kicked(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.as_slice() != ["counter", "target", "spell", "if", "it", "was", "kicked"] {
        return Ok(None);
    }

    let target = TargetAst::Spell(span_from_tokens(&tokens[1..3]));
    let counter = EffectAst::Counter { target };
    let effect = EffectAst::Conditional {
        predicate: PredicateAst::TargetWasKicked,
        if_true: vec![counter],
        if_false: Vec::new(),
    };
    Ok(Some(vec![effect]))
}

fn parse_sentence_counter_target_spell_thats_second_cast_this_turn(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    let matches = clause_words.as_slice()
        == [
            "counter", "target", "spell", "thats", "second", "spell", "cast", "this", "turn",
        ]
        || clause_words.as_slice()
            == [
                "counter", "target", "spell", "thats", "the", "second", "spell", "cast", "this",
                "turn",
            ];
    if !matches {
        return Ok(None);
    }

    let target = TargetAst::Spell(span_from_tokens(&tokens[1..3]));
    let counter = EffectAst::Counter { target };
    let effect = EffectAst::Conditional {
        predicate: PredicateAst::TargetSpellCastOrderThisTurn(2),
        if_true: vec![counter],
        if_false: Vec::new(),
    };
    Ok(Some(vec![effect]))
}

fn parse_sentence_exile_target_creature_with_greatest_power(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    let is_shape = clause_words.starts_with(&["exile", "target", "creature"])
        && contains_word_sequence(&clause_words, &["greatest", "power", "among", "creatures"])
        && (clause_words
            .windows(2)
            .any(|pair| pair == ["on", "battlefield"])
            || clause_words
                .windows(3)
                .any(|triplet| triplet == ["on", "the", "battlefield"]));
    if !is_shape {
        return Ok(None);
    }

    let target_tokens = trim_commas(&tokens[1..3]);
    let target = parse_target_phrase(&target_tokens)?;
    let exile = EffectAst::Exile {
        target: target.clone(),
        face_down: false,
    };
    let effect = EffectAst::Conditional {
        predicate: PredicateAst::TargetHasGreatestPowerAmongCreatures,
        if_true: vec![exile],
        if_false: Vec::new(),
    };
    Ok(Some(vec![effect]))
}

fn parse_mana_spent_to_cast_predicate(words: &[&str]) -> Option<(u32, Option<ManaSymbol>)> {
    if words.len() < 10 || words[0] != "at" || words[1] != "least" {
        return None;
    }

    let amount_tokens = vec![Token::Word(words[2].to_string(), TextSpan::synthetic())];
    let (amount, _) = parse_number(&amount_tokens)?;

    let mut idx = 3;
    if words.get(idx).copied() == Some("of") {
        idx += 1;
    }

    let symbol = if let Some(word) = words.get(idx).copied() {
        if let Some(parsed) = parse_mana_symbol_word(word) {
            idx += 1;
            Some(parsed)
        } else {
            None
        }
    } else {
        None
    };

    let tail = &words[idx..];
    let canonical_tail = ["mana", "was", "spent", "to", "cast", "this", "spell"];
    let plural_tail = ["mana", "were", "spent", "to", "cast", "this", "spell"];
    if tail == canonical_tail || tail == plural_tail {
        return Some((amount, symbol));
    }

    None
}

fn parse_mana_symbol_word(word: &str) -> Option<ManaSymbol> {
    match word {
        "white" => Some(ManaSymbol::White),
        "blue" => Some(ManaSymbol::Blue),
        "black" => Some(ManaSymbol::Black),
        "red" => Some(ManaSymbol::Red),
        "green" => Some(ManaSymbol::Green),
        "colorless" => Some(ManaSymbol::Colorless),
        _ => None,
    }
}

fn parse_effect_chain(tokens: &[Token]) -> Result<Vec<EffectAst>, CardTextError> {
    let words = words(tokens);
    let starts_with_each_opponent =
        words.starts_with(&["each", "opponent"]) || words.starts_with(&["each", "opponents"]);
    let starts_with_each_player =
        words.starts_with(&["each", "player"]) || words.starts_with(&["each", "players"]);

    if tokens.first().is_some_and(|token| token.is_word("they"))
        && tokens.get(1).is_some_and(|token| token.is_word("may"))
    {
        let inner_tokens = &tokens[2..];
        let effects = parse_effect_chain_with_sentence_primitives(inner_tokens)?;
        return Ok(vec![EffectAst::MayByTaggedController {
            tag: TagKey::from("triggering"),
            effects,
        }]);
    }

    if let Some(player) = parse_leading_player_may(tokens) {
        let mut stripped = remove_through_first_word(tokens, "may");
        if stripped
            .first()
            .is_some_and(|token| token.is_word("have") || token.is_word("has"))
        {
            stripped.remove(0);
        }
        let mut effects = parse_effect_chain(&stripped)?;
        for effect in &mut effects {
            bind_implicit_player_context(effect, player);
        }
        return Ok(vec![EffectAst::MayByPlayer { player, effects }]);
    }

    if tokens.first().is_some_and(|token| token.is_word("may"))
        && !starts_with_each_opponent
        && !starts_with_each_player
    {
        let stripped = remove_first_word(tokens, "may");
        let effects = parse_effect_chain(&stripped)?;
        return Ok(vec![EffectAst::May { effects }]);
    }

    if let Some(unless_action) = parse_or_action_clause(tokens)? {
        return Ok(vec![unless_action]);
    }

    parse_effect_chain_with_sentence_primitives(tokens)
}

fn parse_or_action_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.contains(&"or") {
        return Ok(None);
    }

    let mut option_tokens = split_on_or(tokens);
    if option_tokens.len() != 2 {
        return Ok(None);
    }

    let normalize_option = |mut option: Vec<Token>| {
        while option
            .first()
            .is_some_and(|token| token.is_word("and") || token.is_word("or"))
        {
            option.remove(0);
        }
        trim_commas(&option).to_vec()
    };

    let first = normalize_option(option_tokens.remove(0));
    let second = normalize_option(option_tokens.remove(0));
    if first.is_empty() || second.is_empty() {
        return Ok(None);
    }

    let first_starts_effect = find_verb(&first).is_some_and(|(_, verb_idx)| verb_idx == 0)
        || has_effect_head_without_verb(&first);
    let second_starts_effect = find_verb(&second).is_some_and(|(_, verb_idx)| verb_idx == 0)
        || has_effect_head_without_verb(&second);
    if !first_starts_effect || !second_starts_effect {
        return Ok(None);
    }

    let first_effects = match parse_effect_chain_with_sentence_primitives(&first) {
        Ok(effects) if !effects.is_empty() => effects,
        _ => return Ok(None),
    };
    let second_effects = match parse_effect_chain_with_sentence_primitives(&second) {
        Ok(effects) if !effects.is_empty() => effects,
        _ => return Ok(None),
    };

    Ok(Some(EffectAst::UnlessAction {
        effects: first_effects,
        alternative: second_effects,
        player: PlayerAst::Implicit,
    }))
}

fn parse_effect_chain_with_sentence_primitives(
    tokens: &[Token],
) -> Result<Vec<EffectAst>, CardTextError> {
    if let Some(effects) = run_sentence_primitives(tokens, PRE_CONDITIONAL_SENTENCE_PRIMITIVES)? {
        return Ok(effects);
    }
    if let Some(effects) = run_sentence_primitives(tokens, POST_CONDITIONAL_SENTENCE_PRIMITIVES)? {
        return Ok(effects);
    }
    parse_effect_chain_inner(tokens)
}

fn parse_effect_chain_inner(tokens: &[Token]) -> Result<Vec<EffectAst>, CardTextError> {
    if let Some(effects) = parse_search_library_sentence(tokens)? {
        return Ok(effects);
    }

    let mut effects = Vec::new();
    let raw_segments = split_effect_chain_on_and(tokens);
    let mut segments: Vec<Vec<Token>> = Vec::new();
    for segment in raw_segments {
        if segment.is_empty() {
            continue;
        }
        if segments.is_empty() {
            segments.push(segment);
            continue;
        }
        if !segment_has_effect_head(&segment) {
            if let Some(previous) = segments.last()
                && let Some(expanded) = expand_missing_verb_segment(previous, &segment)
            {
                segments.push(expanded);
                continue;
            }
            let last = segments.last_mut().expect("non-empty segments");
            last.push(Token::Word("and".to_string(), TextSpan::synthetic()));
            last.extend(segment);
            continue;
        }
        segments.push(segment);
    }
    while segments.len() > 1 && !segment_has_effect_head(&segments[0]) {
        let mut first = segments.remove(0);
        first.push(Token::Word("and".to_string(), TextSpan::synthetic()));
        let mut next = segments.remove(0);
        first.append(&mut next);
        segments.insert(0, first);
    }
    // Split segments on ", then" when the part after "then" doesn't
    // back-reference the first part (no "that", "it", "them", "its").
    // This handles patterns like "discard your hand, then draw four cards".
    segments = split_segments_on_comma_then(segments);
    segments = split_segments_on_comma_effect_head(segments);
    segments = expand_segments_with_comma_action_clauses(segments);
    segments = expand_segments_with_multi_create_clauses(segments);
    let mut carried_context: Option<CarryContext> = None;
    for segment in segments {
        let segment_effects =
            if let Some(effects) = parse_sentence_return_with_counters_on_it(&segment)? {
                Some(effects)
            } else if let Some(effects) =
                parse_sentence_put_onto_battlefield_with_counters_on_it(&segment)?
            {
                Some(effects)
            } else {
                parse_sentence_exile_source_with_counters(&segment)?
            };
        if let Some(segment_effects) = segment_effects {
            for mut effect in segment_effects {
                if let Some(context) = carried_context {
                    maybe_apply_carried_player_with_clause(&mut effect, context, &segment);
                }
                if let Some(context) = explicit_player_for_carry(&effect) {
                    carried_context = Some(context);
                }
                effects.push(effect);
            }
            continue;
        }
        if let Some(segment_effects) = parse_search_library_sentence(&segment)? {
            for mut effect in segment_effects {
                if let Some(context) = carried_context {
                    maybe_apply_carried_player_with_clause(&mut effect, context, &segment);
                }
                if let Some(context) = explicit_player_for_carry(&effect) {
                    carried_context = Some(context);
                }
                effects.push(effect);
            }
            continue;
        }
        let mut effect = parse_effect_clause_with_trailing_if(&segment)?;
        if let Some(context) = carried_context {
            maybe_apply_carried_player_with_clause(&mut effect, context, &segment);
        }
        if let Some(context) = explicit_player_for_carry(&effect) {
            carried_context = Some(context);
        }
        effects.push(effect);
    }
    // If an "each player ..." clause is followed by additional implicit per-player
    // clauses that reference "it/that <object>", we must keep them inside the same
    // per-player iteration. Otherwise, tag-based "it" references will be overwritten
    // across players (for example: Duskmantle Seer).
    collapse_for_each_player_it_tag_followups(&mut effects);
    collapse_token_copy_next_end_step_exile_followup(&mut effects, tokens);
    collapse_token_copy_end_of_combat_exile_followup(&mut effects, tokens);
    Ok(effects)
}

fn collapse_for_each_player_it_tag_followups(effects: &mut Vec<EffectAst>) {
    let mut idx = 0usize;
    while idx + 1 < effects.len() {
        let should_merge = match (&effects[idx], &effects[idx + 1]) {
            (
                EffectAst::ForEachPlayer { .. },
                EffectAst::ForEachPlayer {
                    effects: followup_effects,
                },
            ) => effects_reference_it_tag(followup_effects),
            _ => false,
        };

        if !should_merge {
            idx += 1;
            continue;
        }

        let followup = effects.remove(idx + 1);
        match (&mut effects[idx], followup) {
            (
                EffectAst::ForEachPlayer {
                    effects: first_effects,
                },
                EffectAst::ForEachPlayer {
                    effects: mut followup_effects,
                },
            ) => {
                first_effects.append(&mut followup_effects);
            }
            _ => {
                // Defensive: should be unreachable given should_merge checks.
            }
        }
        // Re-check this index in case we have a longer chain of followups.
    }
}

fn parse_effect_clause_with_trailing_if(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let Some(if_idx) = tokens.iter().rposition(|token| token.is_word("if")) else {
        return parse_effect_clause(tokens);
    };
    if if_idx == 0 || if_idx + 1 >= tokens.len() {
        return parse_effect_clause(tokens);
    }

    let predicate_tokens = trim_commas(&tokens[if_idx + 1..]);
    if predicate_tokens.is_empty() {
        return parse_effect_clause(tokens);
    }
    let Ok(predicate) = parse_predicate(&predicate_tokens) else {
        return parse_effect_clause(tokens);
    };
    if !matches!(
        predicate,
        PredicateAst::ManaSpentToCastThisSpellAtLeast { .. }
    ) {
        return parse_effect_clause(tokens);
    }

    let leading = trim_commas(&tokens[..if_idx]);
    if leading.is_empty() {
        return parse_effect_clause(tokens);
    }
    let base_effect = if let Ok(effect) = parse_effect_clause(&leading) {
        effect
    } else if let Some(effect) = parse_simple_lose_ability_clause(&leading)? {
        effect
    } else if let Some(effect) = parse_simple_gain_ability_clause(&leading)? {
        effect
    } else {
        return parse_effect_clause(tokens);
    };

    Ok(EffectAst::Conditional {
        predicate,
        if_true: vec![base_effect],
        if_false: Vec::new(),
    })
}

fn is_beginning_of_end_step_words(words: &[&str]) -> bool {
    words
        .windows(5)
        .any(|window| window == ["beginning", "of", "the", "end", "step"])
        || words
            .windows(5)
            .any(|window| window == ["beginning", "of", "next", "end", "step"])
        || words
            .windows(6)
            .any(|window| window == ["beginning", "of", "the", "next", "end", "step"])
}

fn is_end_of_combat_words(words: &[&str]) -> bool {
    words
        .windows(3)
        .any(|window| window == ["end", "of", "combat"])
}

fn target_is_generic_token_filter(target: &TargetAst) -> bool {
    let TargetAst::Object(filter, _, _) = target else {
        return false;
    };
    filter.token
        && filter.zone.is_none()
        && filter.card_types.is_empty()
        && filter.subtypes.is_empty()
        && filter.tagged_constraints.is_empty()
        && filter.controller.is_none()
        && filter.owner.is_none()
}

fn collapse_token_copy_next_end_step_exile_followup(
    effects: &mut Vec<EffectAst>,
    tokens: &[Token],
) {
    let chain_words = words(tokens);
    if !chain_words.contains(&"exile")
        || !chain_words.contains(&"token")
        || !is_beginning_of_end_step_words(&chain_words)
    {
        return;
    }

    let mut idx = 0usize;
    while idx + 1 < effects.len() {
        let mark_next_end_step_exile = match (&effects[idx], &effects[idx + 1]) {
            (
                EffectAst::CreateTokenCopy { .. } | EffectAst::CreateTokenCopyFromSource { .. },
                EffectAst::MoveToZone {
                    target,
                    zone: Zone::Exile,
                    ..
                },
            ) => target_is_generic_token_filter(target),
            (
                EffectAst::CreateTokenCopy { .. } | EffectAst::CreateTokenCopyFromSource { .. },
                EffectAst::Exile { target, .. },
            ) => target_is_generic_token_filter(target),
            _ => false,
        };

        if !mark_next_end_step_exile {
            idx += 1;
            continue;
        }

        match &mut effects[idx] {
            EffectAst::CreateTokenCopy {
                exile_at_next_end_step,
                ..
            }
            | EffectAst::CreateTokenCopyFromSource {
                exile_at_next_end_step,
                ..
            } => {
                *exile_at_next_end_step = true;
            }
            _ => {}
        }
        effects.remove(idx + 1);
    }
}

fn collapse_token_copy_end_of_combat_exile_followup(
    effects: &mut Vec<EffectAst>,
    tokens: &[Token],
) {
    let chain_words = words(tokens);
    if !chain_words.contains(&"exile")
        || !chain_words.contains(&"token")
        || !is_end_of_combat_words(&chain_words)
    {
        return;
    }

    let mut idx = 0usize;
    while idx + 1 < effects.len() {
        let mark_end_of_combat_exile = match (&effects[idx], &effects[idx + 1]) {
            (
                EffectAst::CreateTokenCopy { .. }
                | EffectAst::CreateTokenCopyFromSource { .. }
                | EffectAst::CreateTokenWithMods { .. },
                EffectAst::MoveToZone {
                    target,
                    zone: Zone::Exile,
                    ..
                },
            ) => target_is_generic_token_filter(target),
            (
                EffectAst::CreateTokenCopy { .. }
                | EffectAst::CreateTokenCopyFromSource { .. }
                | EffectAst::CreateTokenWithMods { .. },
                EffectAst::Exile { target, .. },
            ) => target_is_generic_token_filter(target),
            (
                EffectAst::CreateTokenCopy { .. }
                | EffectAst::CreateTokenCopyFromSource { .. }
                | EffectAst::CreateTokenWithMods { .. },
                EffectAst::ExileThatTokenAtEndOfCombat,
            ) => true,
            _ => false,
        };

        if !mark_end_of_combat_exile {
            idx += 1;
            continue;
        }

        match &mut effects[idx] {
            EffectAst::CreateTokenCopy {
                exile_at_end_of_combat,
                ..
            }
            | EffectAst::CreateTokenCopyFromSource {
                exile_at_end_of_combat,
                ..
            }
            | EffectAst::CreateTokenWithMods {
                exile_at_end_of_combat,
                ..
            } => {
                *exile_at_end_of_combat = true;
            }
            _ => {}
        }
        effects.remove(idx + 1);
    }
}

fn expand_segments_with_comma_action_clauses(segments: Vec<Vec<Token>>) -> Vec<Vec<Token>> {
    let mut expanded = Vec::new();

    for segment in segments {
        let segment_words = words(&segment);
        let looks_like_sac_discard_chain = (segment_words.contains(&"sacrifice")
            || segment_words.contains(&"sacrifices"))
            && (segment_words.contains(&"discard") || segment_words.contains(&"discards"));
        if !looks_like_sac_discard_chain {
            expanded.push(segment);
            continue;
        }

        let comma_parts = split_on_comma_or_semicolon(&segment);
        if comma_parts.len() < 2 {
            expanded.push(segment);
            continue;
        }

        let mut local_parts: Vec<Vec<Token>> = Vec::new();
        let mut valid_split = true;

        for raw_part in comma_parts {
            let mut part = trim_commas(&raw_part).to_vec();
            while part.first().is_some_and(|token| token.is_word("and")) {
                part.remove(0);
            }
            if part.is_empty() {
                continue;
            }

            if segment_has_effect_head(&part) {
                local_parts.push(part);
                continue;
            }
            if let Some(previous) = local_parts.last()
                && let Some(expanded_part) = expand_missing_verb_segment(previous, &part)
            {
                local_parts.push(expanded_part);
                continue;
            }

            valid_split = false;
            break;
        }

        if valid_split && local_parts.len() > 1 {
            expanded.extend(local_parts);
        } else {
            expanded.push(segment);
        }
    }

    expanded
}

fn starts_like_create_fragment(tokens: &[Token]) -> bool {
    let words = words(tokens);
    if words.is_empty() {
        return false;
    }
    let starts_like_count = words.first().is_some_and(|word| {
        matches!(
            *word,
            "a" | "an" | "one" | "two" | "three" | "four" | "five" | "six"
        )
    }) || words.first().is_some_and(|word| {
        parse_number(&[Token::Word((*word).to_string(), TextSpan::synthetic())]).is_some()
    }) || words
        .first()
        .is_some_and(|word| word.contains('/') || word == &"x");
    starts_like_count && words.iter().any(|word| matches!(*word, "token" | "tokens"))
}

fn expand_segments_with_multi_create_clauses(segments: Vec<Vec<Token>>) -> Vec<Vec<Token>> {
    let mut expanded = Vec::new();

    for segment in segments {
        let Some((Verb::Create, _)) = find_verb(&segment) else {
            expanded.push(segment);
            continue;
        };
        let segment_words = words(&segment);
        let has_token_rules_tail = segment_words.windows(3).any(|window| {
            matches!(
                window,
                ["when", "this", "token"] | ["whenever", "this", "token"]
            )
        }) || segment_words.windows(2).any(|window| {
            matches!(
                window,
                ["this", "token"] | ["that", "token"] | ["those", "tokens"]
            )
        }) || segment_words
            .windows(2)
            .any(|window| matches!(window, ["it", "has"] | ["they", "have"]));
        if has_token_rules_tail {
            expanded.push(segment);
            continue;
        }
        let token_mentions = segment_words
            .into_iter()
            .filter(|word| matches!(*word, "token" | "tokens"))
            .count();
        if token_mentions < 2 {
            expanded.push(segment);
            continue;
        }

        let comma_parts = split_on_comma_or_semicolon(&segment);
        if comma_parts.len() < 2 {
            expanded.push(segment);
            continue;
        }

        let mut local_parts: Vec<Vec<Token>> = Vec::new();
        for part in comma_parts {
            if part.is_empty() {
                continue;
            }
            if let Some(previous) = local_parts.last()
                && is_token_creation_context(&words(previous))
                && starts_with_inline_token_rules_tail(&words(&part))
            {
                if let Some(last) = local_parts.last_mut() {
                    last.push(Token::Comma(TextSpan::synthetic()));
                    last.extend(part);
                }
                continue;
            }
            if segment_has_effect_head(&part) {
                local_parts.push(part);
                continue;
            }
            if let Some(previous) = local_parts.last()
                && let Some(expanded_part) = expand_missing_verb_segment(previous, &part)
            {
                local_parts.push(expanded_part);
                continue;
            }
            if let Some(last) = local_parts.last_mut() {
                last.push(Token::Comma(TextSpan::synthetic()));
                last.extend(part);
            } else {
                local_parts.push(part);
            }
        }

        if local_parts.len() > 1 {
            expanded.extend(local_parts);
        } else {
            expanded.push(segment);
        }
    }

    expanded
}

fn expand_missing_verb_segment(previous: &[Token], segment: &[Token]) -> Option<Vec<Token>> {
    let (verb, verb_idx) = find_verb(previous)?;
    match verb {
        Verb::Deal => {
            let segment_words = words(segment);
            if parse_value(segment).is_none() || !segment_words.contains(&"damage") {
                return None;
            }
            let mut expanded = Vec::new();
            expanded.extend(previous.iter().take(verb_idx + 1).cloned());
            expanded.extend(segment.iter().cloned());
            Some(expanded)
        }
        Verb::Sacrifice => {
            let segment_words = words(segment);
            let starts_like_object_phrase = matches!(
                segment_words.first().copied(),
                Some("a" | "an" | "another" | "target")
            ) || parse_number(segment).is_some();
            if !starts_like_object_phrase {
                return None;
            }
            let mut expanded = Vec::new();
            expanded.extend(previous.iter().take(verb_idx + 1).cloned());
            expanded.extend(segment.iter().cloned());
            Some(expanded)
        }
        Verb::Create => {
            if !starts_like_create_fragment(segment) {
                return None;
            }
            let mut expanded = Vec::new();
            expanded.extend(previous.iter().take(verb_idx + 1).cloned());
            expanded.extend(segment.iter().cloned());
            Some(expanded)
        }
        _ => None,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CarryContext {
    Player(PlayerAst),
    ForEachPlayer,
    ForEachTargetPlayers(ChoiceCount),
    ForEachOpponent,
}

fn player_ast_from_filter_for_carry(filter: &PlayerFilter) -> Option<PlayerAst> {
    match filter {
        PlayerFilter::You => Some(PlayerAst::You),
        PlayerFilter::Opponent => Some(PlayerAst::Opponent),
        PlayerFilter::Any => Some(PlayerAst::Any),
        PlayerFilter::IteratedPlayer => Some(PlayerAst::That),
        PlayerFilter::Target(inner) => {
            if matches!(inner.as_ref(), PlayerFilter::Opponent) {
                Some(PlayerAst::TargetOpponent)
            } else {
                Some(PlayerAst::Target)
            }
        }
        _ => None,
    }
}

fn player_owner_filter_from_target_for_carry(target: &TargetAst) -> Option<PlayerAst> {
    match target {
        TargetAst::Player(filter, _) => player_ast_from_filter_for_carry(filter),
        TargetAst::Object(filter, _, _) => {
            if !matches!(
                filter.zone,
                Some(Zone::Hand) | Some(Zone::Graveyard) | Some(Zone::Library) | Some(Zone::Exile)
            ) {
                return None;
            }
            filter
                .owner
                .as_ref()
                .and_then(player_ast_from_filter_for_carry)
        }
        TargetAst::WithCount(inner, _) => player_owner_filter_from_target_for_carry(inner),
        _ => None,
    }
}

fn explicit_player_for_carry(effect: &EffectAst) -> Option<CarryContext> {
    if matches!(effect, EffectAst::ForEachPlayer { .. }) {
        return Some(CarryContext::ForEachPlayer);
    }
    if let EffectAst::ForEachTargetPlayers { count, .. } = effect {
        return Some(CarryContext::ForEachTargetPlayers(*count));
    }
    if matches!(effect, EffectAst::ForEachOpponent { .. }) {
        return Some(CarryContext::ForEachOpponent);
    }
    if let EffectAst::Exile { target, .. } | EffectAst::ExileUntilSourceLeaves { target, .. } =
        effect
        && let Some(player) = player_owner_filter_from_target_for_carry(target)
    {
        return Some(CarryContext::Player(player));
    }
    if let EffectAst::ExileAll { filter, .. } = effect
        && let Some(owner) = filter.owner.as_ref()
        && let Some(player) = player_ast_from_filter_for_carry(owner)
    {
        return Some(CarryContext::Player(player));
    }

    let player = match effect {
        EffectAst::Draw { player, .. }
        | EffectAst::DiscardHand { player }
        | EffectAst::Discard { player, .. }
        | EffectAst::GainLife { player, .. }
        | EffectAst::LoseLife { player, .. }
        | EffectAst::Sacrifice { player, .. }
        | EffectAst::Scry { player, .. }
        | EffectAst::Surveil { player, .. }
        | EffectAst::Mill { player, .. }
        | EffectAst::PoisonCounters { player, .. }
        | EffectAst::EnergyCounters { player, .. }
        | EffectAst::RevealTop { player }
        | EffectAst::RevealHand { player }
        | EffectAst::PutIntoHand { player, .. } => *player,
        _ => return None,
    };

    if matches!(player, PlayerAst::Implicit) {
        None
    } else {
        Some(CarryContext::Player(player))
    }
}

fn effect_uses_implicit_player(effect: &EffectAst) -> bool {
    match effect {
        EffectAst::Draw { player, .. }
        | EffectAst::DiscardHand { player }
        | EffectAst::Discard { player, .. }
        | EffectAst::GainLife { player, .. }
        | EffectAst::LoseLife { player, .. }
        | EffectAst::Sacrifice { player, .. }
        | EffectAst::Scry { player, .. }
        | EffectAst::Surveil { player, .. }
        | EffectAst::Mill { player, .. }
        | EffectAst::PoisonCounters { player, .. }
        | EffectAst::EnergyCounters { player, .. }
        | EffectAst::RevealTop { player }
        | EffectAst::RevealHand { player }
        | EffectAst::PutIntoHand { player, .. } => matches!(*player, PlayerAst::Implicit),
        _ => false,
    }
}

fn maybe_apply_carried_player(effect: &mut EffectAst, carried_context: CarryContext) {
    match carried_context {
        CarryContext::Player(carried_player) => match effect {
            EffectAst::Draw { player, .. }
            | EffectAst::DiscardHand { player }
            | EffectAst::Discard { player, .. }
            | EffectAst::GainLife { player, .. }
            | EffectAst::LoseLife { player, .. }
            | EffectAst::Scry { player, .. }
            | EffectAst::Surveil { player, .. }
            | EffectAst::Mill { player, .. }
            | EffectAst::PoisonCounters { player, .. }
            | EffectAst::EnergyCounters { player, .. }
            | EffectAst::RevealTop { player }
            | EffectAst::RevealHand { player }
            | EffectAst::PutIntoHand { player, .. } => {
                if matches!(*player, PlayerAst::Implicit) {
                    *player = carried_player;
                }
            }
            _ => {}
        },
        CarryContext::ForEachPlayer => {
            if effect_uses_implicit_player(effect) {
                let wrapped = effect.clone();
                *effect = EffectAst::ForEachPlayer {
                    effects: vec![wrapped],
                };
            }
        }
        CarryContext::ForEachTargetPlayers(count) => {
            if effect_uses_implicit_player(effect) {
                let wrapped = effect.clone();
                *effect = EffectAst::ForEachTargetPlayers {
                    count,
                    effects: vec![wrapped],
                };
            }
        }
        CarryContext::ForEachOpponent => {
            if effect_uses_implicit_player(effect) {
                let wrapped = effect.clone();
                *effect = EffectAst::ForEachOpponent {
                    effects: vec![wrapped],
                };
            }
        }
    }
}

fn clause_words_for_carry(tokens: &[Token]) -> Vec<&str> {
    let mut clause_words = words(tokens);
    while clause_words
        .first()
        .is_some_and(|word| *word == "then" || *word == "and")
    {
        clause_words.remove(0);
    }
    clause_words
}

fn should_skip_draw_player_carry(
    effect: &EffectAst,
    carried_context: CarryContext,
    clause_tokens: &[Token],
) -> bool {
    let clause_words = clause_words_for_carry(clause_tokens);
    match carried_context {
        CarryContext::Player(_) => {
            let EffectAst::Draw { player, .. } = effect else {
                return false;
            };
            if !matches!(*player, PlayerAst::Implicit) {
                return false;
            }
            matches!(clause_words.first().copied(), Some("draw"))
        }
        CarryContext::ForEachPlayer
        | CarryContext::ForEachTargetPlayers(_)
        | CarryContext::ForEachOpponent => {
            let is_implicit_vision_effect = matches!(
                effect,
                EffectAst::Draw {
                    player: PlayerAst::Implicit,
                    ..
                } | EffectAst::Scry {
                    player: PlayerAst::Implicit,
                    ..
                } | EffectAst::Surveil {
                    player: PlayerAst::Implicit,
                    ..
                }
            );
            if !is_implicit_vision_effect {
                return false;
            }
            matches!(
                clause_words.first().copied(),
                Some("draw" | "scry" | "surveil")
            )
        }
    }
}

fn maybe_apply_carried_player_with_clause(
    effect: &mut EffectAst,
    carried_context: CarryContext,
    clause_tokens: &[Token],
) {
    if should_skip_draw_player_carry(effect, carried_context, clause_tokens) {
        return;
    }
    maybe_apply_carried_player(effect, carried_context);
}

fn bind_implicit_player_context(effect: &mut EffectAst, player: PlayerAst) {
    match effect {
        EffectAst::Draw {
            player: effect_player,
            ..
        }
        | EffectAst::DiscardHand {
            player: effect_player,
        }
        | EffectAst::Discard {
            player: effect_player,
            ..
        }
        | EffectAst::GainLife {
            player: effect_player,
            ..
        }
        | EffectAst::LoseLife {
            player: effect_player,
            ..
        }
        | EffectAst::Sacrifice {
            player: effect_player,
            ..
        }
        | EffectAst::Scry {
            player: effect_player,
            ..
        }
        | EffectAst::Surveil {
            player: effect_player,
            ..
        }
        | EffectAst::Mill {
            player: effect_player,
            ..
        }
        | EffectAst::PoisonCounters {
            player: effect_player,
            ..
        }
        | EffectAst::EnergyCounters {
            player: effect_player,
            ..
        }
        | EffectAst::RevealTop {
            player: effect_player,
        }
        | EffectAst::RevealHand {
            player: effect_player,
        }
        | EffectAst::PutIntoHand {
            player: effect_player,
            ..
        }
        | EffectAst::PayMana {
            player: effect_player,
            ..
        }
        | EffectAst::PayEnergy {
            player: effect_player,
            ..
        }
        | EffectAst::AddMana {
            player: effect_player,
            ..
        }
        | EffectAst::AddManaScaled {
            player: effect_player,
            ..
        }
        | EffectAst::AddManaAnyColor {
            player: effect_player,
            ..
        }
        | EffectAst::AddManaAnyOneColor {
            player: effect_player,
            ..
        }
        | EffectAst::AddManaChosenColor {
            player: effect_player,
            ..
        }
        | EffectAst::AddManaFromLandCouldProduce {
            player: effect_player,
            ..
        }
        | EffectAst::AddManaCommanderIdentity {
            player: effect_player,
            ..
        }
        | EffectAst::SearchLibrary {
            player: effect_player,
            ..
        }
        | EffectAst::ShuffleGraveyardIntoLibrary {
            player: effect_player,
        }
        | EffectAst::ShuffleLibrary {
            player: effect_player,
        }
        | EffectAst::CreateToken {
            player: effect_player,
            ..
        }
        | EffectAst::CreateTokenCopy {
            player: effect_player,
            ..
        }
        | EffectAst::CreateTokenCopyFromSource {
            player: effect_player,
            ..
        }
        | EffectAst::CreateTokenWithMods {
            player: effect_player,
            ..
        }
        | EffectAst::CopySpell {
            player: effect_player,
            ..
        }
        | EffectAst::RetargetStackObject {
            chooser: effect_player,
            ..
        } => {
            if matches!(*effect_player, PlayerAst::Implicit) {
                *effect_player = player;
            }
        }
        EffectAst::May { effects }
        | EffectAst::MayByPlayer { effects, .. }
        | EffectAst::MayByTaggedController { effects, .. }
        | EffectAst::IfResult { effects, .. }
        | EffectAst::ForEachOpponent { effects }
        | EffectAst::ForEachPlayer { effects }
        | EffectAst::ForEachTargetPlayers { effects, .. }
        | EffectAst::ForEachObject { effects, .. }
        | EffectAst::ForEachTagged { effects, .. }
        | EffectAst::ForEachOpponentDoesNot { effects }
        | EffectAst::ForEachPlayerDoesNot { effects }
        | EffectAst::ForEachOpponentDid { effects, .. }
        | EffectAst::ForEachPlayerDid { effects, .. }
        | EffectAst::ForEachTaggedPlayer { effects, .. }
        | EffectAst::DelayedUntilNextEndStep { effects, .. }
        | EffectAst::DelayedUntilEndStepOfExtraTurn { effects, .. }
        | EffectAst::DelayedUntilEndOfCombat { effects }
        | EffectAst::DelayedTriggerThisTurn { effects, .. }
        | EffectAst::DelayedWhenLastObjectDiesThisTurn { effects, .. }
        | EffectAst::UnlessPays { effects, .. }
        | EffectAst::VoteOption { effects, .. } => {
            for nested in effects {
                bind_implicit_player_context(nested, player);
            }
        }
        EffectAst::UnlessAction {
            effects,
            alternative,
            ..
        } => {
            for nested in effects {
                bind_implicit_player_context(nested, player);
            }
            for nested in alternative {
                bind_implicit_player_context(nested, player);
            }
        }
        EffectAst::Conditional {
            if_true, if_false, ..
        } => {
            for nested in if_true {
                bind_implicit_player_context(nested, player);
            }
            for nested in if_false {
                bind_implicit_player_context(nested, player);
            }
        }
        _ => {}
    }
}

fn parse_leading_player_may(tokens: &[Token]) -> Option<PlayerAst> {
    let mut words = words(tokens);
    while words
        .first()
        .is_some_and(|word| *word == "then" || *word == "and")
    {
        words.remove(0);
    }
    if words.len() < 2 {
        return None;
    }

    if words.starts_with(&["you", "may"]) {
        return Some(PlayerAst::You);
    }
    if words.starts_with(&["target", "opponent", "may"])
        || words.starts_with(&["target", "opponents", "may"])
    {
        return Some(PlayerAst::TargetOpponent);
    }
    if words.starts_with(&["target", "player", "may"])
        || words.starts_with(&["target", "players", "may"])
    {
        return Some(PlayerAst::Target);
    }
    if words.starts_with(&["that", "player", "may"])
        || words.starts_with(&["that", "players", "may"])
    {
        return Some(PlayerAst::That);
    }
    if words.len() >= 4
        && words[0] == "that"
        && matches!(words[1], "creatures" | "permanents" | "sources" | "spells")
        && words[2] == "controller"
        && words[3] == "may"
    {
        return Some(PlayerAst::ItsController);
    }
    if words.len() >= 4
        && words[0] == "that"
        && matches!(words[1], "creatures" | "permanents" | "sources" | "spells")
        && words[2] == "owner"
        && words[3] == "may"
    {
        return Some(PlayerAst::ItsOwner);
    }
    if words.starts_with(&["the", "player", "may"]) || words.starts_with(&["the", "players", "may"])
    {
        return Some(PlayerAst::That);
    }
    if words.starts_with(&["defending", "player", "may"]) {
        return Some(PlayerAst::Defending);
    }
    if words.starts_with(&["attacking", "player", "may"])
        || words.starts_with(&["the", "attacking", "player", "may"])
    {
        return Some(PlayerAst::Attacking);
    }
    if words.starts_with(&["its", "controller", "may"])
        || words.starts_with(&["their", "controller", "may"])
    {
        return Some(PlayerAst::ItsController);
    }
    if words.starts_with(&["its", "owner", "may"]) || words.starts_with(&["their", "owner", "may"])
    {
        return Some(PlayerAst::ItsOwner);
    }
    if words.starts_with(&["opponent", "may"])
        || words.starts_with(&["opponents", "may"])
        || words.starts_with(&["an", "opponent", "may"])
    {
        return Some(PlayerAst::Opponent);
    }

    None
}

fn remove_first_word(tokens: &[Token], word: &str) -> Vec<Token> {
    let mut removed = false;
    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        if !removed && token.is_word(word) {
            removed = true;
            continue;
        }
        out.push(token.clone());
    }
    out
}

fn remove_through_first_word(tokens: &[Token], word: &str) -> Vec<Token> {
    let mut seen = false;
    let mut out = Vec::new();
    for token in tokens {
        if !seen {
            if token.is_word(word) {
                seen = true;
            }
            continue;
        }
        out.push(token.clone());
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Add,
    Move,
    Deal,
    Draw,
    Counter,
    Destroy,
    Exile,
    Untap,
    Scry,
    Discard,
    Transform,
    Flip,
    Regenerate,
    Mill,
    Get,
    Reveal,
    Look,
    Lose,
    Gain,
    Put,
    Sacrifice,
    Create,
    Investigate,
    Proliferate,
    Tap,
    Attach,
    Remove,
    Return,
    Exchange,
    Become,
    Switch,
    Skip,
    Surveil,
    Shuffle,
    Reorder,
    Pay,
    Goad,
}

type ClausePrimitiveParser = fn(&[Token]) -> Result<Option<EffectAst>, CardTextError>;

struct ClausePrimitive {
    parser: ClausePrimitiveParser,
}

fn parse_retarget_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    if let Some(effect) = parse_choose_new_targets_clause(tokens)? {
        return Ok(Some(effect));
    }
    if let Some(effect) = parse_change_target_clause(tokens)? {
        return Ok(Some(effect));
    }
    Ok(None)
}

fn parse_choose_new_targets_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let is_choose = clause_words.starts_with(&["choose", "new", "targets", "for"])
        || clause_words.starts_with(&["chooses", "new", "targets", "for"]);
    if !is_choose {
        return Ok(None);
    }

    let mut tail_tokens = &tokens[4..];
    if tail_tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "missing choose-new-targets target".to_string(),
        ));
    }

    if let Some(if_idx) = tail_tokens.iter().position(|token| token.is_word("if")) {
        tail_tokens = &tail_tokens[..if_idx];
    }

    let tail_words = words(tail_tokens);
    if tail_words.starts_with(&["it"])
        || tail_words.starts_with(&["them"])
        || tail_words.starts_with(&["the", "copy"])
        || tail_words.starts_with(&["that", "copy"])
        || tail_words.starts_with(&["the", "spell"])
        || tail_words.starts_with(&["that", "spell"])
    {
        let target = TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(tail_tokens));
        return Ok(Some(EffectAst::RetargetStackObject {
            target,
            mode: RetargetModeAst::All,
            chooser: PlayerAst::Implicit,
            require_change: false,
            new_target_restriction: None,
        }));
    }

    let (count, base_tokens, explicit_target) = if tail_words.starts_with(&["any", "number", "of"])
    {
        (Some(ChoiceCount::any_number()), &tail_tokens[3..], false)
    } else if tail_words.starts_with(&["target"]) {
        (None, &tail_tokens[1..], true)
    } else {
        (None, tail_tokens, false)
    };

    let mut filter = parse_stack_retarget_filter(base_tokens)?;
    if base_tokens.iter().any(|token| token.is_word("other")) {
        filter.other = true;
    }

    let mut target = TargetAst::Object(
        filter,
        if explicit_target {
            span_from_tokens(tail_tokens)
        } else {
            None
        },
        None,
    );
    if let Some(count) = count {
        target = TargetAst::WithCount(Box::new(target), count);
    }

    Ok(Some(EffectAst::RetargetStackObject {
        target,
        mode: RetargetModeAst::All,
        chooser: PlayerAst::Implicit,
        require_change: false,
        new_target_restriction: None,
    }))
}

fn parse_change_target_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.is_empty() || clause_words[0] != "change" {
        return Ok(None);
    }

    if let Some(unless_idx) = tokens.iter().position(|token| token.is_word("unless")) {
        let main_tokens = trim_commas(&tokens[..unless_idx]);
        let unless_tokens = trim_commas(&tokens[unless_idx + 1..]);
        let Some(inner) = parse_change_target_clause_inner(&main_tokens)? else {
            return Ok(None);
        };
        let (player, mana) = parse_unless_pays_clause(&unless_tokens)?;
        return Ok(Some(EffectAst::UnlessPays {
            effects: vec![inner],
            player,
            mana,
        }));
    }

    parse_change_target_clause_inner(tokens)
}

fn parse_change_target_clause_inner(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let (mode, after_of_idx) = if clause_words.starts_with(&["change", "the", "target", "of"]) {
        (RetargetModeAst::All, 4)
    } else if clause_words.starts_with(&["change", "the", "targets", "of"]) {
        (RetargetModeAst::All, 4)
    } else if clause_words.starts_with(&["change", "a", "target", "of"]) {
        (RetargetModeAst::All, 4)
    } else {
        return Ok(None);
    };

    if tokens.len() <= after_of_idx {
        return Err(CardTextError::ParseError(
            "missing target after change-the-target clause".to_string(),
        ));
    }

    let mut tail_tokens = trim_commas(&tokens[after_of_idx..]).to_vec();
    let mut fixed_target: Option<TargetAst> = None;
    if let Some(to_idx) = tail_tokens.iter().position(|token| token.is_word("to")) {
        let to_tail = &tail_tokens[to_idx + 1..];
        let to_words = words(to_tail);
        if to_words.starts_with(&["this"]) {
            fixed_target = Some(TargetAst::Source(span_from_tokens(to_tail)));
            tail_tokens.truncate(to_idx);
        }
    }

    let mut filter = parse_stack_retarget_filter(&tail_tokens)?;
    let tail_words = words(&tail_tokens);

    if tail_words
        .windows(4)
        .any(|w| w == ["with", "a", "single", "target"])
    {
        filter = filter.target_count_exact(1);
    }
    if tail_words
        .windows(5)
        .any(|w| w == ["targets", "only", "a", "single", "creature"])
    {
        filter = filter
            .targeting_only_object(ObjectFilter::creature())
            .target_count_exact(1);
    }
    if tail_words
        .windows(4)
        .any(|w| w == ["targets", "only", "this", "creature"])
        || tail_words
            .windows(4)
            .any(|w| w == ["targets", "only", "this", "permanent"])
    {
        filter = filter
            .targeting_only_object(ObjectFilter::source())
            .target_count_exact(1);
    }
    if tail_words
        .windows(3)
        .any(|w| w == ["targets", "only", "you"])
    {
        filter = filter
            .targeting_only_player(PlayerFilter::You)
            .target_count_exact(1);
    }
    if tail_words
        .windows(4)
        .any(|w| w == ["targets", "only", "a", "player"])
    {
        filter = filter
            .targeting_only_player(PlayerFilter::Any)
            .target_count_exact(1);
    }
    if tail_words
        .windows(5)
        .any(|w| w == ["if", "that", "target", "is", "you"])
    {
        filter = filter
            .targeting_only_player(PlayerFilter::You)
            .target_count_exact(1);
    }

    let target = TargetAst::Object(filter, span_from_tokens(tokens), None);

    let mode = if let Some(fixed) = fixed_target {
        RetargetModeAst::OneToFixed { target: fixed }
    } else {
        mode
    };

    Ok(Some(EffectAst::RetargetStackObject {
        target,
        mode,
        chooser: PlayerAst::Implicit,
        require_change: true,
        new_target_restriction: None,
    }))
}

fn parse_unless_pays_clause(
    tokens: &[Token],
) -> Result<(PlayerAst, Vec<ManaSymbol>), CardTextError> {
    if tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "missing unless clause".to_string(),
        ));
    }
    let pays_idx = tokens
        .iter()
        .position(|token| token.is_word("pays"))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing pays keyword (clause: '{}')",
                words(tokens).join(" ")
            ))
        })?;

    let player_tokens = trim_commas(&tokens[..pays_idx]);
    let player = match parse_subject(&player_tokens) {
        SubjectAst::Player(player) => player,
        _ => PlayerAst::Implicit,
    };

    let mut mana = Vec::new();
    for token in tokens[pays_idx + 1..].iter() {
        let Some(word) = token.as_word() else {
            continue;
        };
        match parse_mana_symbol(word) {
            Ok(symbol) => mana.push(symbol),
            Err(_) => break,
        }
    }

    if mana.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing mana cost (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    Ok((player, mana))
}

fn parse_stack_retarget_filter(tokens: &[Token]) -> Result<ObjectFilter, CardTextError> {
    let words = words(tokens);
    let has_ability = words
        .iter()
        .any(|word| *word == "ability" || *word == "abilities");
    let has_spell = words
        .iter()
        .any(|word| *word == "spell" || *word == "spells");
    let has_activated = words.iter().any(|word| *word == "activated");
    let has_instant = words.iter().any(|word| *word == "instant");
    let has_sorcery = words.iter().any(|word| *word == "sorcery");

    let mut filter = if has_activated && has_ability {
        ObjectFilter::activated_ability()
    } else if has_ability && has_spell {
        ObjectFilter::spell_or_ability()
    } else if has_ability {
        ObjectFilter::ability()
    } else if (has_instant || has_sorcery) && has_spell {
        ObjectFilter::instant_or_sorcery()
    } else if has_spell {
        ObjectFilter::spell()
    } else {
        return Err(CardTextError::ParseError(format!(
            "unsupported retarget target clause (clause: '{}')",
            words.join(" ")
        )));
    };

    if words.iter().any(|word| *word == "other") {
        filter.other = true;
    }

    Ok(filter)
}

fn run_clause_primitives(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    const PRIMITIVES: &[ClausePrimitive] = &[
        ClausePrimitive {
            parser: parse_retarget_clause,
        },
        ClausePrimitive {
            parser: parse_copy_spell_clause,
        },
        ClausePrimitive {
            parser: parse_win_the_game_clause,
        },
        ClausePrimitive {
            parser: parse_deal_damage_equal_to_power_clause,
        },
        ClausePrimitive {
            parser: parse_fight_clause,
        },
        ClausePrimitive {
            parser: parse_clash_clause,
        },
        ClausePrimitive {
            parser: parse_for_each_target_players_clause,
        },
        ClausePrimitive {
            parser: parse_for_each_opponent_clause,
        },
        ClausePrimitive {
            parser: parse_for_each_player_clause,
        },
        ClausePrimitive {
            parser: parse_double_counters_clause,
        },
        ClausePrimitive {
            parser: parse_distribute_counters_clause,
        },
        ClausePrimitive {
            parser: parse_until_end_of_turn_may_play_tagged_clause,
        },
        ClausePrimitive {
            parser: parse_until_your_next_turn_may_play_tagged_clause,
        },
        ClausePrimitive {
            parser: parse_cast_or_play_tagged_clause,
        },
        ClausePrimitive {
            parser: parse_prevent_next_damage_clause,
        },
        ClausePrimitive {
            parser: parse_prevent_all_damage_clause,
        },
        ClausePrimitive {
            parser: parse_can_attack_as_though_no_defender_clause,
        },
        ClausePrimitive {
            parser: parse_must_block_if_able_clause,
        },
        ClausePrimitive {
            parser: parse_until_duration_triggered_clause,
        },
        ClausePrimitive {
            parser: parse_keyword_mechanic_clause,
        },
        ClausePrimitive {
            parser: parse_connive_clause,
        },
        ClausePrimitive {
            parser: parse_choose_target_and_verb_clause,
        },
        ClausePrimitive {
            parser: parse_verb_first_clause,
        },
    ];

    for primitive in PRIMITIVES {
        if let Some(effect) = (primitive.parser)(tokens)? {
            return Ok(Some(effect));
        }
    }
    Ok(None)
}

fn parse_must_block_if_able_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    use crate::effect::Until;

    let clause_words = words(tokens);

    // "All creatures able to block target creature this turn do so."
    if clause_words.starts_with(&["all", "creatures", "able", "to", "block"]) {
        let mut tail_tokens = trim_commas(&tokens[5..]);
        let tail_words = words(&tail_tokens);
        if !tail_words.ends_with(&["do", "so"]) {
            return Ok(None);
        }
        tail_tokens = trim_commas(&tail_tokens[..tail_tokens.len().saturating_sub(2)]);

        let (duration, attacker_tokens) =
            if let Some((duration, remainder)) = parse_restriction_duration(&tail_tokens)? {
                (duration, remainder)
            } else {
                (Until::EndOfTurn, tail_tokens.to_vec())
            };
        let attacker_tokens = trim_commas(&attacker_tokens);
        if attacker_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing attacker in must-block clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }

        let attacker_target = parse_target_phrase(&attacker_tokens)?;
        let attacker_filter = target_ast_to_object_filter(attacker_target).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported attacker target in must-block clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;

        return Ok(Some(EffectAst::Cant {
            restriction: crate::effect::Restriction::must_block_specific_attacker(
                ObjectFilter::creature(),
                attacker_filter,
            ),
            duration,
        }));
    }

    // "<subject> blocks <attacker> this turn if able."
    let Some(block_idx) = tokens
        .iter()
        .position(|token| token.is_word("block") || token.is_word("blocks"))
    else {
        return Ok(None);
    };
    if block_idx == 0 || block_idx + 1 >= tokens.len() {
        return Ok(None);
    }

    let subject_tokens = trim_commas(&tokens[..block_idx]);
    if subject_tokens.is_empty() {
        return Ok(None);
    }
    let blockers_filter = parse_subject_object_filter(&subject_tokens)?.ok_or_else(|| {
        CardTextError::ParseError(format!(
            "unsupported blocker subject in must-block clause (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;

    let mut tail_tokens = trim_commas(&tokens[block_idx + 1..]);
    let tail_words = words(&tail_tokens);
    if !tail_words.ends_with(&["if", "able"]) {
        return Ok(None);
    }
    tail_tokens = trim_commas(&tail_tokens[..tail_tokens.len().saturating_sub(2)]);

    let (duration, attacker_tokens) =
        if let Some((duration, remainder)) = parse_restriction_duration(&tail_tokens)? {
            (duration, remainder)
        } else {
            (Until::EndOfTurn, tail_tokens.to_vec())
        };
    let attacker_tokens = trim_commas(&attacker_tokens);
    if attacker_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing attacker in must-block clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let attacker_target = parse_target_phrase(&attacker_tokens)?;
    let attacker_filter = target_ast_to_object_filter(attacker_target).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "unsupported attacker target in must-block clause (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;

    Ok(Some(EffectAst::Cant {
        restriction: crate::effect::Restriction::must_block_specific_attacker(
            blockers_filter,
            attacker_filter,
        ),
        duration,
    }))
}

fn parse_until_duration_triggered_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let has_leading_duration = clause_words.starts_with(&["until", "end", "of", "turn"])
        || clause_words.starts_with(&["until", "your", "next", "turn"])
        || clause_words.starts_with(&["until", "your", "next", "upkeep"])
        || clause_words.starts_with(&["until", "your", "next", "untap", "step"])
        || clause_words.starts_with(&["during", "your", "next", "untap", "step"]);
    if !has_leading_duration {
        return Ok(None);
    }

    let Some((duration, trigger_tokens)) = parse_restriction_duration(tokens)? else {
        return Ok(None);
    };
    if trigger_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing trigger after duration clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let trigger_words = words(&trigger_tokens);
    let looks_like_trigger = trigger_words
        .first()
        .is_some_and(|word| *word == "when" || *word == "whenever")
        || trigger_words.starts_with(&["at", "the"]);
    if !looks_like_trigger {
        return Ok(None);
    }

    let (trigger, effects, max_triggers_per_turn) = match parse_triggered_line(&trigger_tokens)? {
        LineAst::Triggered {
            trigger,
            effects,
            max_triggers_per_turn,
        } => (trigger, effects, max_triggers_per_turn),
        _ => {
            return Err(CardTextError::ParseError(format!(
                "unsupported duration-triggered clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    };

    let (compiled_effects, choices) = compile_trigger_effects(Some(&trigger), &effects)?;
    let trigger_text = trigger_words.join(" ");
    let ability = Ability {
        kind: AbilityKind::Triggered(TriggeredAbility {
            trigger: compile_trigger_spec(trigger),
            effects: compiled_effects,
            choices,
            intervening_if: max_triggers_per_turn.map(crate::ConditionExpr::MaxTimesEachTurn),
        }),
        functional_zones: vec![Zone::Battlefield],
        text: Some(trigger_text.clone()),
    };
    let granted = StaticAbility::grant_object_ability_for_filter(
        ObjectFilter::source(),
        ability,
        trigger_text,
    );

    Ok(Some(EffectAst::GrantAbilitiesToTarget {
        target: TargetAst::Source(span_from_tokens(tokens)),
        abilities: vec![granted],
        duration,
    }))
}

fn parse_power_reference_word_count(words: &[&str]) -> Option<usize> {
    if words.starts_with(&["its", "power"]) || words.starts_with(&["that", "power"]) {
        return Some(2);
    }
    if words.starts_with(&["this", "source", "power"])
        || words.starts_with(&["this", "creature", "power"])
        || words.starts_with(&["that", "creature", "power"])
        || words.starts_with(&["that", "objects", "power"])
    {
        return Some(3);
    }
    None
}

fn is_damage_source_target(target: &TargetAst) -> bool {
    matches!(
        target,
        TargetAst::Source(_) | TargetAst::Object(_, _, _) | TargetAst::Tagged(_, _)
    )
}

fn parse_deal_damage_equal_to_power_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let Some(deal_idx) = tokens
        .iter()
        .position(|token| token.is_word("deal") || token.is_word("deals"))
    else {
        return Ok(None);
    };
    if deal_idx == 0 {
        return Ok(None);
    }

    let source_tokens = trim_commas(&tokens[..deal_idx]);

    let rest = trim_commas(&tokens[deal_idx + 1..]);
    if rest.is_empty() || !rest[0].is_word("damage") {
        return Ok(None);
    }

    let Some(equal_idx) = rest
        .windows(2)
        .position(|window| window[0].is_word("equal") && window[1].is_word("to"))
    else {
        return Ok(None);
    };

    let source = parse_target_phrase(&source_tokens)?;
    if !is_damage_source_target(&source) {
        return Err(CardTextError::ParseError(format!(
            "unsupported damage source target phrase (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let power_ref_words = words(&rest[equal_idx + 2..]);
    let Some(power_ref_len) = parse_power_reference_word_count(&power_ref_words) else {
        return Ok(None);
    };

    let tail_after_power = trim_commas(&rest[equal_idx + 2 + power_ref_len..]);
    let pre_equal_words = words(&rest[..equal_idx]);

    let target = if pre_equal_words == ["damage"] {
        let mut target_tokens = tail_after_power.as_slice();
        if target_tokens
            .first()
            .is_some_and(|token| token.is_word("to"))
        {
            target_tokens = &target_tokens[1..];
        }
        if target_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing damage target after power reference (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let mut normalized_target_tokens = target_tokens;
        let target_words = words(target_tokens);
        if target_words.starts_with(&["each", "of"]) {
            let each_of_tokens = &target_tokens[2..];
            let each_of_words = words(each_of_tokens);
            if each_of_words.iter().any(|word| *word == "target") {
                normalized_target_tokens = each_of_tokens;
            }
        }
        parse_target_phrase(normalized_target_tokens)?
    } else if pre_equal_words.starts_with(&["damage", "to"]) {
        let target_tokens = trim_commas(&rest[2..equal_idx]);
        let target_words = words(&target_tokens);
        if target_words == ["itself"] || target_words == ["it"] {
            if !tail_after_power.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "unsupported trailing target after self-damage power clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            source.clone()
        } else {
            if !tail_after_power.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "unsupported trailing target after explicit power-damage target (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            parse_target_phrase(&target_tokens)?
        }
    } else {
        return Ok(None);
    };

    Ok(Some(EffectAst::DealDamageEqualToPower { source, target }))
}

fn parse_fight_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let Some(fight_idx) = tokens
        .iter()
        .position(|token| token.is_word("fight") || token.is_word("fights"))
    else {
        return Ok(None);
    };

    if fight_idx + 1 >= tokens.len() {
        return Err(CardTextError::ParseError(format!(
            "fight clause requires two creatures (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let right_tokens = trim_commas(&tokens[fight_idx + 1..]);
    if right_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "fight clause requires two creatures (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let creature1 = if fight_idx == 0 {
        TargetAst::Source(None)
    } else {
        let left_tokens = trim_commas(&tokens[..fight_idx]);
        if left_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "fight clause requires two creatures (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        if let Some(filter) = parse_for_each_object_subject(&left_tokens)? {
            let creature2 = parse_target_phrase(&right_tokens)?;
            if matches!(
                creature2,
                TargetAst::Player(_, _) | TargetAst::PlayerOrPlaneswalker(_, _)
            ) {
                return Err(CardTextError::ParseError(format!(
                    "fight target must be a creature (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            return Ok(Some(EffectAst::ForEachObject {
                filter,
                effects: vec![EffectAst::FightIterated { creature2 }],
            }));
        }
        parse_target_phrase(&left_tokens)?
    };
    let creature2 = parse_target_phrase(&right_tokens)?;

    for target in [&creature1, &creature2] {
        if matches!(
            target,
            TargetAst::Player(_, _) | TargetAst::PlayerOrPlaneswalker(_, _)
        ) {
            return Err(CardTextError::ParseError(format!(
                "fight target must be a creature (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    }

    Ok(Some(EffectAst::Fight {
        creature1,
        creature2,
    }))
}

fn parse_clash_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let Some(first) = clause_words.first().copied() else {
        return Ok(None);
    };
    if first != "clash" && first != "clashes" {
        return Ok(None);
    }

    let mut tail = trim_commas(&tokens[1..]);
    if tail.first().is_some_and(|token| token.is_word("with")) {
        tail = trim_commas(&tail[1..]);
    }
    let tail_end = tail
        .iter()
        .position(|token| token.is_word("then") || matches!(token, Token::Comma(_)))
        .unwrap_or(tail.len());
    let tail = trim_commas(&tail[..tail_end]);
    if tail.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing opponent in clash clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let tail_words: Vec<&str> = words(&tail)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    let opponent = match tail_words.as_slice() {
        ["opponent"] => ClashOpponentAst::Opponent,
        ["target", "opponent"] => ClashOpponentAst::TargetOpponent,
        ["defending", "player"] => ClashOpponentAst::DefendingPlayer,
        _ => {
            return Err(CardTextError::ParseError(format!(
                "unsupported clash target (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    };

    Ok(Some(EffectAst::Clash { opponent }))
}

