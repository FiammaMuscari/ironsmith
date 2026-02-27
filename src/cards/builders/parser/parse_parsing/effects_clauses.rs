fn parse_effect_clause(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    if tokens.is_empty() {
        return Err(CardTextError::ParseError("empty effect clause".to_string()));
    }

    if let Some(player) = parse_leading_player_may(tokens) {
        let mut stripped = remove_through_first_word(tokens, "may");
        if stripped
            .first()
            .is_some_and(|token| token.is_word("have") || token.is_word("has"))
        {
            stripped.remove(0);
        }
        let mut effects = parse_effect_chain_with_sentence_primitives(&stripped)?;
        for effect in &mut effects {
            bind_implicit_player_context(effect, player);
        }
        return Ok(EffectAst::MayByPlayer { player, effects });
    }

    if tokens.first().is_some_and(|token| token.is_word("may")) {
        let stripped = remove_first_word(tokens, "may");
        let effects = parse_effect_chain_with_sentence_primitives(&stripped)?;
        return Ok(EffectAst::May { effects });
    }

    let clause_words = words(tokens);
    if clause_words
        .iter()
        .any(|word| *word == "choose" || *word == "chooses")
        && clause_words.contains(&"creature")
        && clause_words.contains(&"type")
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported creature-type choice clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    if is_mana_replacement_clause_words(&clause_words) {
        return Err(CardTextError::ParseError(format!(
            "unsupported mana replacement clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    if is_mana_trigger_additional_clause_words(&clause_words) {
        return Err(CardTextError::ParseError(format!(
            "unsupported mana-triggered additional-mana clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    if let Some(effect) = run_clause_primitives(tokens)? {
        return Ok(effect);
    }

    if let Some(effect) = parse_has_base_power_clause(tokens)? {
        return Ok(effect);
    }

    if let Some(effect) = parse_has_base_power_toughness_clause(tokens)? {
        return Ok(effect);
    }

    if let Some((chooser, choose_filter, choose_count)) =
        parse_target_player_choose_objects_clause(tokens)?
    {
        return Ok(EffectAst::ChooseObjects {
            filter: choose_filter,
            count: choose_count,
            player: chooser,
            tag: TagKey::from(IT_TAG),
        });
    }

    // "This creature assigns no combat damage this turn."
    // Used in Laccolith-style effects: "If you do, this creature assigns no combat damage this turn."
    if clause_words
        .windows(4)
        .any(|window| window == ["assigns", "no", "combat", "damage"])
    {
        let assigns_idx = tokens
            .iter()
            .position(|token| token.is_word("assigns") || token.is_word("assign"))
            .unwrap_or(0);
        let subject_tokens = trim_commas(&tokens[..assigns_idx]);
        let tail_tokens = trim_commas(&tokens[assigns_idx + 1..]);
        let tail_words = words(&tail_tokens);
        if !tail_words.starts_with(&["no", "combat", "damage"]) {
            return Err(CardTextError::ParseError(format!(
                "unsupported assigns-no-combat-damage clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let mut idx = 3usize;
        if tail_words.get(idx) == Some(&"this") && tail_words.get(idx + 1) == Some(&"turn") {
            idx += 2;
        } else if tail_words.get(idx) == Some(&"this") && tail_words.get(idx + 1) == Some(&"combat")
        {
            idx += 2;
        }
        if idx != tail_words.len() {
            return Err(CardTextError::ParseError(format!(
                "unsupported assigns-no-combat-damage clause tail (clause: '{}')",
                clause_words.join(" ")
            )));
        }

        let subject_words = words(&subject_tokens);
        let source = if subject_words.is_empty()
            || matches!(
                subject_words.as_slice(),
                ["it"] | ["this"] | ["this", "creature"]
            ) {
            TargetAst::Source(None)
        } else {
            parse_target_phrase(&subject_tokens)?
        };

        return Ok(EffectAst::PreventAllCombatDamageFromSource {
            duration: Until::EndOfTurn,
            source,
        });
    }

    if tokens.first().is_some_and(|token| token.is_word("target")) && find_verb(tokens).is_none() {
        let clause_words = words(tokens);
        let looks_like_restriction_clause = find_negation_span(tokens).is_some()
            || clause_words.contains(&"blocked")
            || clause_words.contains(&"except")
            || clause_words.contains(&"unless")
            || clause_words.contains(&"attack")
            || clause_words.contains(&"attacks")
            || clause_words.contains(&"block")
            || clause_words.contains(&"blocks");
        if looks_like_restriction_clause {
            return Err(CardTextError::ParseError(format!(
                "unsupported target-only restriction clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let target = parse_target_phrase(tokens)?;
        return Ok(EffectAst::TargetOnly { target });
    }

    let (verb, verb_idx) = find_verb(tokens).ok_or_else(|| {
        let clause = words(tokens).join(" ");
        let known_verbs = [
            "add",
            "move",
            "deal",
            "draw",
            "counter",
            "destroy",
            "exile",
            "untap",
            "scry",
            "discard",
            "transform",
            "regenerate",
            "mill",
            "get",
            "reveal",
            "look",
            "lose",
            "gain",
            "put",
            "sacrifice",
            "create",
            "investigate",
            "attach",
            "remove",
            "return",
            "exchange",
            "become",
            "switch",
            "skip",
            "surveil",
            "shuffle",
            "reorder",
            "pay",
            "goad",
        ];
        CardTextError::ParseError(format!(
            "could not find verb in effect clause (clause: '{clause}'; known verbs: {})",
            known_verbs.join(", ")
        ))
    })?;
    parser_trace_stack("parse_effect_clause:verb-found", tokens);

    if matches!(verb, Verb::Counter)
        && verb_idx > 0
        && tokens.iter().any(|token| token.is_word("on"))
    {
        if let Ok(effect) = parse_put_counters(tokens) {
            parser_trace("parse_effect_clause:counter-noun-treated-as-put", tokens);
            return Ok(effect);
        }
    }

    if matches!(verb, Verb::Get) {
        let subject_tokens = &tokens[..verb_idx];
        if !subject_tokens.is_empty() {
            let subject_words = words(subject_tokens);
            if let Some(mod_token) = tokens.get(verb_idx + 1).and_then(Token::as_word)
                && let Ok((power, toughness)) = parse_pt_modifier_values(mod_token)
            {
                let modifier_tail = &tokens[verb_idx + 1..];
                if let Some(count) = parse_get_for_each_count_value(modifier_tail)? {
                    let modifier_words = words(modifier_tail);
                    let duration = if modifier_words.starts_with(&["until", "end", "of", "turn"])
                        || modifier_words
                            .windows(4)
                            .any(|window| window == ["until", "end", "of", "turn"])
                    {
                        Until::EndOfTurn
                    } else {
                        Until::EndOfTurn
                    };
                    let target = parse_target_phrase(subject_tokens)?;
                    let power_per = match power {
                        Value::Fixed(value) => value,
                        _ => {
                            return Err(CardTextError::ParseError(format!(
                                "unsupported dynamic gets-for-each power modifier (clause: '{}')",
                                words(tokens).join(" ")
                            )));
                        }
                    };
                    let toughness_per = match toughness {
                        Value::Fixed(value) => value,
                        _ => {
                            return Err(CardTextError::ParseError(format!(
                                "unsupported dynamic gets-for-each toughness modifier (clause: '{}')",
                                words(tokens).join(" ")
                            )));
                        }
                    };
                    return Ok(EffectAst::PumpForEach {
                        power_per,
                        toughness_per,
                        target,
                        count,
                        duration,
                    });
                }

                let (power, toughness, duration, condition) =
                    parse_get_modifier_values_with_tail(modifier_tail, power, toughness)?;

                let mut normalized_subject_words: Vec<&str> = subject_words
                    .iter()
                    .copied()
                    .filter(|word| *word != "each")
                    .collect();
                if normalized_subject_words.first().copied() == Some("of") {
                    normalized_subject_words.remove(0);
                }
                if normalized_subject_words.as_slice() == ["it"]
                    || normalized_subject_words.as_slice() == ["they"]
                    || normalized_subject_words.as_slice() == ["them"]
                {
                    return Ok(EffectAst::Pump {
                        power: power.clone(),
                        toughness: toughness.clone(),
                        target: TargetAst::Tagged(
                            TagKey::from(IT_TAG),
                            span_from_tokens(subject_tokens),
                        ),
                        duration,
                        condition,
                    });
                }

                let is_demonstrative_subject = normalized_subject_words
                    .first()
                    .is_some_and(|word| *word == "that" || *word == "those");
                if is_demonstrative_subject {
                    let target = parse_target_phrase(subject_tokens)?;
                    return Ok(EffectAst::Pump {
                        power: power.clone(),
                        toughness: toughness.clone(),
                        target,
                        duration,
                        condition,
                    });
                }

                if subject_words.contains(&"target") {
                    let target_tokens = if subject_tokens
                        .first()
                        .is_some_and(|token| token.is_word("have") || token.is_word("has"))
                    {
                        &subject_tokens[1..]
                    } else {
                        subject_tokens
                    };
                    let target = parse_target_phrase(target_tokens)?;
                    return Ok(EffectAst::Pump {
                        power: power.clone(),
                        toughness: toughness.clone(),
                        target,
                        duration,
                        condition,
                    });
                }

                let has_counter_state_pronoun = subject_words.windows(3).any(|window| {
                    matches!(window[0], "counter" | "counters")
                        && window[1] == "on"
                        && matches!(window[2], "it" | "them")
                });
                let has_disallowed_pronoun_reference = (subject_words.contains(&"it")
                    || subject_words.contains(&"them"))
                    && !has_counter_state_pronoun;
                if !subject_words.contains(&"this")
                    && !has_disallowed_pronoun_reference
                    && !has_demonstrative_object_reference(&subject_words)
                    && let Ok(filter) = parse_object_filter(subject_tokens, false)
                    && filter != ObjectFilter::default()
                {
                    return Ok(EffectAst::PumpAll {
                        filter,
                        power: power.clone(),
                        toughness: toughness.clone(),
                        duration,
                    });
                }
            }
        }
    }

    let subject_tokens = &tokens[..verb_idx];
    let subject_words = words(subject_tokens);
    if is_target_player_dealt_damage_by_this_turn_subject(&subject_words) {
        return Err(CardTextError::ParseError(format!(
            "unsupported combat-history player subject (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    if matches!(verb, Verb::Gain) && !subject_tokens.is_empty() {
        let rest_words = words(&tokens[verb_idx + 1..]);
        let has_protection = rest_words.contains(&"protection");
        let has_choice = rest_words.contains(&"choice");
        let has_color = rest_words.contains(&"color");
        let has_colorless = rest_words.contains(&"colorless");
        if has_protection && has_choice && (has_color || has_colorless) {
            let target = parse_target_phrase(subject_tokens)?;
            return Ok(EffectAst::GrantProtectionChoice {
                target,
                allow_colorless: has_colorless,
            });
        }
    }
    if matches!(verb, Verb::Gain)
        && let Some(effect) = parse_simple_gain_ability_clause(tokens)?
    {
        return Ok(effect);
    }
    if matches!(verb, Verb::Lose)
        && let Some(effect) = parse_simple_lose_ability_clause(tokens)?
    {
        return Ok(effect);
    }
    let for_each_subject_filter = parse_for_each_object_subject(subject_tokens)?;
    let rest = &tokens[verb_idx + 1..];
    let mut effect = if matches!(verb, Verb::Become) {
        parse_become_clause(subject_tokens, rest)?
    } else {
        let subject = parse_subject(subject_tokens);
        parse_effect_with_verb(verb, Some(subject), rest)?
    };
    if let Some(filter) = for_each_subject_filter {
        effect = EffectAst::ForEachObject {
            filter,
            effects: vec![effect],
        };
    }
    Ok(effect)
}

fn parse_become_clause(
    subject_tokens: &[Token],
    rest_tokens: &[Token],
) -> Result<EffectAst, CardTextError> {
    use crate::effect::Until;

    let subject_words = words(subject_tokens);
    let subject = parse_subject(subject_tokens);

    // Split off trailing duration, if present.
    let (duration, become_tokens) =
        if let Some((duration, remainder)) = parse_restriction_duration(rest_tokens)? {
            (duration, remainder)
        } else {
            (Until::Forever, trim_commas(rest_tokens).to_vec())
        };
    let become_words_vec = words(&become_tokens);
    let become_words = if become_words_vec
        .first()
        .is_some_and(|w| *w == "the" || *w == "a")
    {
        &become_words_vec[1..]
    } else {
        &become_words_vec[..]
    };

    // Player "life total becomes N"
    if let Some(SubjectAst::Player(player)) = Some(subject) {
        if subject_words.contains(&"life") && subject_words.contains(&"total") {
            let amount = parse_value(&become_tokens)
                .map(|(value, _)| value)
                .ok_or_else(|| {
                    CardTextError::ParseError(format!(
                        "missing life total amount (clause: '{}')",
                        words(rest_tokens).join(" ")
                    ))
                })?;
            return Ok(EffectAst::SetLifeTotal { amount, player });
        }
    }

    // Resolve object target.
    let target = if subject_words.is_empty()
        || subject_words == ["it"]
        || subject_words == ["they"]
        || subject_words == ["them"]
    {
        TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(subject_tokens))
    } else if subject_words == ["this"]
        || subject_words == ["this", "permanent"]
        || subject_words == ["this", "creature"]
        || subject_words == ["this", "land"]
    {
        TargetAst::Source(span_from_tokens(subject_tokens))
    } else {
        parse_target_phrase(subject_tokens)?
    };

    // "the/a basic land type of your choice"
    if become_words == ["basic", "land", "type", "of", "your", "choice"] {
        return Ok(EffectAst::BecomeBasicLandTypeChoice { target, duration });
    }

    // "the color of your choice" / "color of your choice"
    if become_words == ["color", "of", "your", "choice"]
        || become_words == ["color", "or", "colors", "of", "your", "choice"]
        || become_words == ["colors", "of", "your", "choice"]
    {
        return Ok(EffectAst::BecomeColorChoice { target, duration });
    }

    // "the creature type of your choice" / "creature type of your choice"
    if become_words == ["creature", "type", "of", "your", "choice"] {
        return Ok(EffectAst::BecomeCreatureTypeChoice {
            target,
            duration,
            excluded_subtypes: Vec::new(),
        });
    }

    // "colorless"
    if become_words == ["colorless"] {
        return Ok(EffectAst::MakeColorless { target, duration });
    }

    // "<color>" / "<color> and <color>" / "<color> and or <color>"
    let color_tokens = become_words
        .iter()
        .copied()
        .filter(|word| *word != "and" && *word != "or")
        .collect::<Vec<_>>();
    if !color_tokens.is_empty() {
        let mut colors = crate::color::ColorSet::new();
        let mut all_colors = true;
        for word in color_tokens {
            if let Some(color) = parse_color(word) {
                colors = colors.union(color);
            } else {
                all_colors = false;
                break;
            }
        }
        if all_colors && !colors.is_empty() {
            return Ok(EffectAst::SetColors {
                target,
                colors,
                duration,
            });
        }
    }

    Err(CardTextError::ParseError(format!(
        "unsupported become clause (clause: '{}')",
        words(rest_tokens).join(" ")
    )))
}

fn parse_for_each_object_subject(
    subject_tokens: &[Token],
) -> Result<Option<ObjectFilter>, CardTextError> {
    if subject_tokens.is_empty() {
        return Ok(None);
    }
    let subject_words = words(subject_tokens);
    if subject_words.is_empty() {
        return Ok(None);
    }

    let mut filter_tokens = if subject_words.starts_with(&["for", "each"]) {
        &subject_tokens[2..]
    } else if subject_words.first() == Some(&"each") {
        &subject_tokens[1..]
    } else {
        return Ok(None);
    };
    if filter_tokens
        .first()
        .is_some_and(|token| token.is_word("of"))
    {
        filter_tokens = &filter_tokens[1..];
    }
    if filter_tokens.is_empty() {
        return Ok(None);
    }
    let mut normalized_filter_tokens: Vec<Token> = filter_tokens.to_vec();
    if let Some(attached_idx) = filter_tokens
        .iter()
        .position(|token| token.is_word("attached"))
        && filter_tokens
            .get(attached_idx + 1)
            .is_some_and(|token| token.is_word("to"))
        && attached_idx > 0
    {
        let attached_tail_words = words(&filter_tokens[attached_idx + 2..]);
        let attached_to_creature = attached_tail_words.starts_with(&["creature"])
            || attached_tail_words.starts_with(&["a", "creature"]);
        if attached_to_creature {
            normalized_filter_tokens = trim_commas(&filter_tokens[..attached_idx]);
        }
    }

    let filter_words = words(&normalized_filter_tokens);
    if filter_words.is_empty() {
        return Ok(None);
    }

    // Player-iteration forms are handled by dedicated ForEachPlayer/Opponent parsers.
    if filter_words.starts_with(&["player"])
        || filter_words.starts_with(&["players"])
        || filter_words.starts_with(&["opponent"])
        || filter_words.starts_with(&["opponents"])
        || filter_words.starts_with(&["target", "player"])
        || filter_words.starts_with(&["target", "players"])
        || filter_words.starts_with(&["target", "opponent"])
        || filter_words.starts_with(&["target", "opponents"])
    {
        return Ok(None);
    }

    Ok(Some(parse_object_filter(&normalized_filter_tokens, false)?))
}

fn parse_for_each_targeted_object_subject(
    subject_tokens: &[Token],
) -> Result<Option<(ObjectFilter, ChoiceCount)>, CardTextError> {
    if subject_tokens.is_empty() {
        return Ok(None);
    }
    let subject_words = words(subject_tokens);
    if subject_words.is_empty() {
        return Ok(None);
    }

    let mut target_tokens = if subject_words.starts_with(&["for", "each"]) {
        &subject_tokens[2..]
    } else if subject_words.first() == Some(&"each") {
        &subject_tokens[1..]
    } else {
        return Ok(None);
    };
    if target_tokens
        .first()
        .is_some_and(|token| token.is_word("of"))
    {
        target_tokens = &target_tokens[1..];
    }
    if target_tokens.is_empty() {
        return Ok(None);
    }

    let target = match parse_target_phrase(target_tokens) {
        Ok(target) => target,
        Err(_) => return Ok(None),
    };
    let TargetAst::WithCount(inner, count) = target else {
        return Ok(None);
    };
    let TargetAst::Object(filter, _, _) = *inner else {
        return Ok(None);
    };
    Ok(Some((filter, count)))
}

fn has_demonstrative_object_reference(words: &[&str]) -> bool {
    words.windows(2).any(|window| {
        matches!(
            window,
            ["that", "creature"]
                | ["that", "creatures"]
                | ["that", "permanent"]
                | ["that", "permanents"]
                | ["that", "artifact"]
                | ["that", "artifacts"]
                | ["that", "enchantment"]
                | ["that", "enchantments"]
                | ["that", "land"]
                | ["that", "lands"]
                | ["that", "card"]
                | ["that", "cards"]
                | ["that", "token"]
                | ["that", "tokens"]
                | ["that", "spell"]
                | ["that", "spells"]
                | ["those", "creatures"]
                | ["those", "permanents"]
                | ["those", "artifacts"]
                | ["those", "enchantments"]
                | ["those", "lands"]
                | ["those", "cards"]
                | ["those", "tokens"]
                | ["those", "spells"]
        )
    })
}

fn is_target_player_dealt_damage_by_this_turn_subject(words: &[&str]) -> bool {
    if words.len() < 8 {
        return false;
    }
    if !(words.starts_with(&["target", "player"]) || words.starts_with(&["target", "players"])) {
        return false;
    }
    words
        .windows(6)
        .any(|window| window == ["dealt", "damage", "by", "this", "creature", "this"])
        && words.windows(2).any(|window| window == ["this", "turn"])
}

fn is_mana_replacement_clause_words(words: &[&str]) -> bool {
    let has_if = words.contains(&"if");
    let has_tap = words.contains(&"tap") || words.contains(&"taps");
    let has_for_mana = words.windows(2).any(|window| window == ["for", "mana"]);
    let has_produce = words.contains(&"produce") || words.contains(&"produces");
    let has_instead = words.contains(&"instead");
    has_if && has_tap && has_for_mana && has_produce && has_instead
}

fn is_mana_trigger_additional_clause_words(words: &[&str]) -> bool {
    let has_whenever = words.contains(&"whenever");
    let has_tap = words.contains(&"tap") || words.contains(&"taps");
    let has_for_mana = words.windows(2).any(|window| window == ["for", "mana"]);
    let has_add = words.contains(&"add") || words.contains(&"adds");
    let has_additional = words.contains(&"additional");
    has_whenever && has_tap && has_for_mana && has_add && has_additional
}

fn parse_has_base_power_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let words_all = words(tokens);
    let Some(has_idx) = words_all
        .iter()
        .position(|word| *word == "has" || *word == "have")
    else {
        return Ok(None);
    };
    let subject_tokens = &tokens[..has_idx];
    if subject_tokens.is_empty() {
        return Ok(None);
    }
    let subject_words = words(subject_tokens);

    let rest_words = &words_all[has_idx + 1..];
    if rest_words.len() < 3 || !rest_words.starts_with(&["base", "power"]) {
        return Ok(None);
    }
    if rest_words.get(2).is_some_and(|word| *word == "and") {
        return Ok(None);
    }

    let has_token_idx = tokens
        .iter()
        .position(|token| token.is_word("has") || token.is_word("have"))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing has/have token in base-power clause (clause: '{}')",
                words_all.join(" ")
            ))
        })?;
    let rest_tokens = &tokens[has_token_idx + 1..];

    let mut seen_words = 0usize;
    let mut value_token_idx = None;
    for (idx, token) in rest_tokens.iter().enumerate() {
        if token.as_word().is_some() {
            seen_words += 1;
            if seen_words == 3 {
                value_token_idx = Some(idx);
                break;
            }
        }
    }
    let Some(value_token_idx) = value_token_idx else {
        return Ok(None);
    };
    let (power, value_used) = parse_value(&rest_tokens[value_token_idx..]).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "invalid base power value (clause: '{}')",
            words_all.join(" ")
        ))
    })?;

    let tail_words: Vec<&str> = rest_tokens[value_token_idx + value_used..]
        .iter()
        .filter_map(Token::as_word)
        .collect();
    if tail_words.is_empty() {
        let has_target_subject = subject_words.contains(&"target");
        let has_leading_until_eot = subject_words.starts_with(&["until", "end", "of", "turn"]);
        let has_temporal_words = words_all
            .windows(4)
            .any(|window| window == ["until", "end", "of", "turn"])
            || words_all
                .windows(2)
                .any(|window| window == ["this", "turn"] || window == ["next", "turn"]);
        if !has_target_subject && !has_leading_until_eot && !has_temporal_words {
            return Ok(None);
        }
    } else if tail_words.as_slice() != ["until", "end", "of", "turn"] {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing base power clause (clause: '{}')",
            words_all.join(" ")
        )));
    }

    let target_tokens: Vec<Token> = if subject_words.starts_with(&["until", "end", "of", "turn"]) {
        let mut skip_idx = 4usize;
        if subject_tokens
            .get(skip_idx)
            .is_some_and(|token| matches!(token, Token::Comma(_)))
        {
            skip_idx += 1;
        }
        trim_commas(&subject_tokens[skip_idx..]).to_vec()
    } else {
        subject_tokens.to_vec()
    };
    let target = parse_target_phrase(&target_tokens)?;
    Ok(Some(EffectAst::SetBasePower {
        power,
        target,
        duration: Until::EndOfTurn,
    }))
}

fn parse_has_base_power_toughness_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let words_all = words(tokens);
    let Some(has_idx) = words_all
        .iter()
        .position(|word| *word == "has" || *word == "have")
    else {
        return Ok(None);
    };
    let subject_tokens = &tokens[..has_idx];
    if subject_tokens.is_empty() {
        return Ok(None);
    }
    let subject_words = words(subject_tokens);

    let rest_words = &words_all[has_idx + 1..];
    if rest_words.len() < 5 || !rest_words.starts_with(&["base", "power", "and", "toughness"]) {
        return Ok(None);
    }

    let (power, toughness) = parse_pt_modifier(rest_words[4]).map_err(|_| {
        CardTextError::ParseError(format!(
            "invalid base power/toughness value (clause: '{}')",
            words_all.join(" ")
        ))
    })?;

    let tail = &rest_words[5..];
    if tail.is_empty() {
        let has_target_subject = subject_words.contains(&"target");
        let has_leading_until_eot = subject_words.starts_with(&["until", "end", "of", "turn"]);
        let has_temporal_words = words_all
            .windows(4)
            .any(|window| window == ["until", "end", "of", "turn"])
            || words_all
                .windows(2)
                .any(|window| window == ["this", "turn"] || window == ["next", "turn"]);
        if !has_target_subject && !has_leading_until_eot && !has_temporal_words {
            return Ok(None);
        }
    }
    if !tail.is_empty() && tail != ["until", "end", "of", "turn"] {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing base power/toughness clause (clause: '{}')",
            words_all.join(" ")
        )));
    }

    let target_tokens: Vec<Token> = if subject_words.starts_with(&["until", "end", "of", "turn"]) {
        let mut skip_idx = 4usize;
        if subject_tokens
            .get(skip_idx)
            .is_some_and(|token| matches!(token, Token::Comma(_)))
        {
            skip_idx += 1;
        }
        trim_commas(&subject_tokens[skip_idx..]).to_vec()
    } else {
        subject_tokens.to_vec()
    };
    let target = parse_target_phrase(&target_tokens)?;
    Ok(Some(EffectAst::SetBasePowerToughness {
        power: Value::Fixed(power),
        toughness: Value::Fixed(toughness),
        target,
        duration: Until::EndOfTurn,
    }))
}

fn parse_get_for_each_count_value(tokens: &[Token]) -> Result<Option<Value>, CardTextError> {
    let mut for_each_idx = None;
    for idx in 0..tokens.len().saturating_sub(1) {
        if tokens[idx].is_word("for") && tokens[idx + 1].is_word("each") {
            for_each_idx = Some(idx);
            break;
        }
    }

    let Some(idx) = for_each_idx else {
        return Ok(None);
    };

    let mut filter_tokens = &tokens[idx + 2..];
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "missing filter after 'for each' in gets clause".to_string(),
        ));
    }

    let mut other = false;
    if filter_tokens
        .first()
        .is_some_and(|token| token.is_word("other") || token.is_word("another"))
    {
        other = true;
        filter_tokens = &filter_tokens[1..];
    }

    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "missing filter after 'for each' in gets clause".to_string(),
        ));
    }

    let filter_words = words(filter_tokens);
    if filter_words.starts_with(&["basic", "land", "type", "among"])
        || filter_words.starts_with(&["basic", "land", "types", "among"])
    {
        let mut scope_tokens = &filter_tokens[4..];
        if scope_tokens
            .first()
            .is_some_and(|token| token.is_word("the"))
        {
            scope_tokens = &scope_tokens[1..];
        }
        if scope_tokens.is_empty() {
            return Err(CardTextError::ParseError(
                "missing scope after 'basic land type among' in gets clause".to_string(),
            ));
        }
        let filter = parse_object_filter(scope_tokens, false)?;
        return Ok(Some(Value::BasicLandTypesAmong(filter)));
    }
    if filter_words.starts_with(&["color", "among"])
        || filter_words.starts_with(&["colors", "among"])
    {
        let mut scope_tokens = &filter_tokens[2..];
        if scope_tokens
            .first()
            .is_some_and(|token| token.is_word("the"))
        {
            scope_tokens = &scope_tokens[1..];
        }
        if scope_tokens.is_empty() {
            return Err(CardTextError::ParseError(
                "missing scope after 'color among' in gets clause".to_string(),
            ));
        }
        let filter = parse_object_filter(scope_tokens, false)?;
        return Ok(Some(Value::ColorsAmong(filter)));
    }

    Ok(Some(Value::Count(parse_object_filter(
        filter_tokens,
        other,
    )?)))
}

fn value_contains_unbound_x(value: &Value) -> bool {
    match value {
        Value::X | Value::XTimes(_) => true,
        Value::Add(left, right) => {
            value_contains_unbound_x(left) || value_contains_unbound_x(right)
        }
        _ => false,
    }
}

fn replace_unbound_x_with_value(
    value: Value,
    replacement: &Value,
    clause: &str,
) -> Result<Value, CardTextError> {
    match value {
        Value::X => Ok(replacement.clone()),
        Value::XTimes(multiplier) => {
            if multiplier == 1 {
                return Ok(replacement.clone());
            }
            if let Value::Fixed(fixed) = replacement {
                return Ok(Value::Fixed(fixed * multiplier));
            }
            Err(CardTextError::ParseError(format!(
                "unsupported signed dynamic X replacement in gets clause (clause: '{}')",
                clause
            )))
        }
        Value::Add(left, right) => Ok(Value::Add(
            Box::new(replace_unbound_x_with_value(*left, replacement, clause)?),
            Box::new(replace_unbound_x_with_value(*right, replacement, clause)?),
        )),
        other => Ok(other),
    }
}

fn parse_get_modifier_values_with_tail(
    modifier_tokens: &[Token],
    power: Value,
    toughness: Value,
) -> Result<(Value, Value, Until, Option<crate::ConditionExpr>), CardTextError> {
    let clause = words(modifier_tokens).join(" ");
    let mut out_power = power;
    let mut out_toughness = toughness;
    let duration = Until::EndOfTurn;
    let mut condition = None;

    if modifier_tokens.is_empty() {
        return Ok((out_power, out_toughness, duration, condition));
    }

    let after_modifier = &modifier_tokens[1..];
    let after_modifier_words = words(after_modifier);
    let until_word_count = if after_modifier_words.starts_with(&["until", "end", "of", "turn"]) {
        4usize
    } else {
        0usize
    };
    let tail_start = token_index_for_word_index(after_modifier, until_word_count)
        .unwrap_or(after_modifier.len());
    let tail_tokens = trim_commas(&after_modifier[tail_start..]);

    if tail_tokens.is_empty() {
        return Ok((out_power, out_toughness, duration, condition));
    }

    let tail_words = words(&tail_tokens);
    if tail_words.starts_with(&["for", "as", "long", "as"])
        && tail_words.contains(&"this")
        && tail_words.contains(&"remains")
        && tail_words.contains(&"tapped")
    {
        condition = Some(crate::ConditionExpr::SourceIsTapped);
        return Ok((
            out_power,
            out_toughness,
            Until::ThisLeavesTheBattlefield,
            condition,
        ));
    }
    if !tail_words.starts_with(&["where", "x", "is"]) {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing gets clause (clause: '{}')",
            clause
        )));
    }

    if !value_contains_unbound_x(&out_power) && !value_contains_unbound_x(&out_toughness) {
        return Err(CardTextError::ParseError(format!(
            "where-X gets clause missing X modifier (clause: '{}')",
            clause
        )));
    }

    let x_value = parse_where_x_value_clause(&tail_tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "unsupported where-X gets clause (clause: '{}')",
            clause
        ))
    })?;
    out_power = replace_unbound_x_with_value(out_power, &x_value, &clause)?;
    out_toughness = replace_unbound_x_with_value(out_toughness, &x_value, &clause)?;

    Ok((out_power, out_toughness, duration, condition))
}

fn force_implicit_token_controller_you(effects: &mut [EffectAst]) {
    for effect in effects {
        match effect {
            EffectAst::CreateToken { player, .. }
            | EffectAst::CreateTokenWithMods { player, .. }
            | EffectAst::CreateTokenCopy { player, .. }
            | EffectAst::CreateTokenCopyFromSource { player, .. } => {
                if matches!(player, PlayerAst::Implicit) {
                    *player = PlayerAst::You;
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
            | EffectAst::VoteOption { effects, .. } => force_implicit_token_controller_you(effects),
            EffectAst::UnlessAction {
                effects,
                alternative,
                ..
            } => {
                force_implicit_token_controller_you(effects);
                force_implicit_token_controller_you(alternative);
            }
            EffectAst::Conditional {
                if_true, if_false, ..
            } => {
                force_implicit_token_controller_you(if_true);
                force_implicit_token_controller_you(if_false);
            }
            _ => {}
        }
    }
}

fn parse_for_each_opponent_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let mut clause_tokens = tokens;
    let mut clause_words = words(clause_tokens);
    if clause_words.first().copied() == Some("then") {
        clause_tokens = &clause_tokens[1..];
        clause_words = words(clause_tokens);
    }
    if clause_words.len() < 2 {
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
    if inner_words.starts_with(&["who", "has", "less", "life", "than", "you"]) {
        let effect_start =
            token_index_for_word_index(&inner_tokens, 6).unwrap_or(inner_tokens.len());
        let effect_tokens = trim_commas(&inner_tokens[effect_start..]);
        if effect_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing effect after 'each opponent who has less life than you' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let mut branch_effects = if effect_tokens.iter().any(|token| token.is_word("may")) {
            let stripped = remove_first_word(&effect_tokens, "may");
            let inner_effects = parse_effect_chain_inner(&stripped)?;
            vec![EffectAst::May {
                effects: inner_effects,
            }]
        } else {
            parse_effect_chain(&effect_tokens)?
        };
        force_implicit_token_controller_you(&mut branch_effects);
        return Ok(Some(EffectAst::ForEachOpponent {
            effects: vec![EffectAst::Conditional {
                predicate: PredicateAst::PlayerHasLessLifeThanYou {
                    player: PlayerAst::That,
                },
                if_true: branch_effects,
                if_false: Vec::new(),
            }],
        }));
    }
    if inner_words.first().copied() == Some("who")
        && let Some((negation_idx, negation_len)) = negated_action_word_index(&inner_words)
    {
        let effect_token_start = if let Some(comma_idx) = inner_tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)))
        {
            comma_idx + 1
        } else if let Some(this_way_idx) = inner_words
            .windows(2)
            .position(|pair| pair == ["this", "way"])
        {
            token_index_for_word_index(&inner_tokens, this_way_idx + 2)
                .unwrap_or(inner_tokens.len())
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
        let effects = parse_effect_chain_inner(&effect_tokens)?;
        return Ok(Some(EffectAst::ForEachOpponentDoesNot { effects }));
    }

    if inner_words.first().copied() == Some("who")
        && let Some(this_way_idx) = inner_words
            .windows(2)
            .position(|window| window == ["this", "way"])
    {
        let effect_start = this_way_idx + 2;
        let effect_token_start =
            token_index_for_word_index(&inner_tokens, effect_start).unwrap_or(inner_tokens.len());
        let effect_tokens = trim_commas(&inner_tokens[effect_token_start..]);
        if effect_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing effect after 'each opponent who ... this way' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let effects = parse_effect_chain_inner(&effect_tokens)?;
        let predicate = parse_who_did_this_way_predicate(&inner_tokens)?;
        return Ok(Some(EffectAst::ForEachOpponentDid { effects, predicate }));
    }
    if inner_words.starts_with(&["who", "does"])
        || inner_words.starts_with(&["who", "do"])
        || inner_words.starts_with(&["who", "did"])
    {
        let effect_token_start = if let Some(comma_idx) = inner_tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)))
        {
            comma_idx + 1
        } else {
            token_index_for_word_index(&inner_tokens, 2).unwrap_or(inner_tokens.len())
        };
        let effect_tokens = trim_commas(&inner_tokens[effect_token_start..]);
        if effect_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing effect after 'each opponent who does' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let mut effects = parse_effect_chain_inner(&effect_tokens)?;
        for effect in &mut effects {
            bind_implicit_player_context(effect, PlayerAst::You);
        }
        return Ok(Some(EffectAst::ForEachOpponentDid {
            effects,
            predicate: None,
        }));
    }

    let effects = if inner_tokens.iter().any(|token| token.is_word("may")) {
        let stripped = remove_first_word(&inner_tokens, "may");
        let inner_effects = parse_effect_chain_inner(&stripped)?;
        vec![EffectAst::May {
            effects: inner_effects,
        }]
    } else {
        parse_effect_chain(&inner_tokens)?
    };
    Ok(Some(EffectAst::ForEachOpponent { effects }))
}

fn parse_for_each_target_players_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let mut clause_tokens = tokens;
    let mut clause_words = words(clause_tokens);
    if clause_words.first().copied() == Some("then") {
        clause_tokens = &clause_tokens[1..];
        clause_words = words(clause_tokens);
    }
    if clause_words.len() < 4 {
        return Ok(None);
    }

    let mut start = 0usize;
    let mut count = ChoiceCount::exactly(1);
    if clause_words.starts_with(&["any", "number", "of"]) {
        count = ChoiceCount::any_number();
        start = 3;
    } else if clause_words.starts_with(&["up", "to"])
        && let Some((value, used)) = parse_number(&clause_tokens[2..])
    {
        count = ChoiceCount::up_to(value as usize);
        start = 2 + used;
    } else if let Some((range_count, used)) = parse_target_count_range_prefix(clause_tokens) {
        count = range_count;
        start = used;
    } else if let Some((value, used)) = parse_number(clause_tokens)
        && clause_tokens
            .get(used)
            .is_some_and(|token| token.is_word("target"))
    {
        count = ChoiceCount::exactly(value as usize);
        start = used;
    }

    let Some(target_token) = clause_tokens.get(start) else {
        return Ok(None);
    };
    if !target_token.is_word("target") {
        return Ok(None);
    }
    if !clause_tokens
        .get(start + 1)
        .is_some_and(|token| token.is_word("player") || token.is_word("players"))
    {
        return Ok(None);
    }
    if !clause_tokens
        .get(start + 2)
        .is_some_and(|token| token.is_word("each"))
    {
        return Ok(None);
    }

    let inner_tokens = trim_commas(&clause_tokens[start + 3..]);
    if inner_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing effect after target-player each clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let effects = if inner_tokens.iter().any(|token| token.is_word("may")) {
        let stripped = remove_first_word(&inner_tokens, "may");
        let inner_effects = parse_effect_chain_inner(&stripped)?;
        vec![EffectAst::May {
            effects: inner_effects,
        }]
    } else {
        parse_effect_chain_inner(&inner_tokens)?
    };
    Ok(Some(EffectAst::ForEachTargetPlayers { count, effects }))
}

fn parse_who_did_this_way_predicate(
    inner_tokens: &[Token],
) -> Result<Option<PredicateAst>, CardTextError> {
    let inner_words = words(inner_tokens);
    if inner_words.first().copied() != Some("who") {
        return Ok(None);
    }
    let Some(this_way_idx) = inner_words
        .windows(2)
        .position(|window| window == ["this", "way"])
    else {
        return Ok(None);
    };
    let verb = inner_words.get(1).copied().unwrap_or("");
    let supports_tag = matches!(verb, "sacrificed" | "destroyed" | "exiled");
    if !supports_tag {
        return Ok(None);
    }
    if this_way_idx <= 2 {
        return Ok(None);
    }
    let filter_start = token_index_for_word_index(inner_tokens, 2).unwrap_or(inner_tokens.len());
    let filter_end =
        token_index_for_word_index(inner_tokens, this_way_idx).unwrap_or(inner_tokens.len());
    if filter_start >= filter_end {
        return Ok(None);
    }
    let filter_tokens = trim_commas(&inner_tokens[filter_start..filter_end]);
    if filter_tokens.is_empty() {
        return Ok(None);
    }
    let filter = match parse_object_filter(&filter_tokens, false) {
        Ok(filter) => filter,
        Err(_) => return Ok(None),
    };
    Ok(Some(PredicateAst::PlayerTaggedObjectMatches {
        player: PlayerAst::That,
        tag: TagKey::from(IT_TAG),
        filter,
    }))
}

fn parse_for_each_player_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let mut clause_tokens = tokens;
    let mut clause_words = words(clause_tokens);
    if clause_words.first().copied() == Some("then") {
        clause_tokens = &clause_tokens[1..];
        clause_words = words(clause_tokens);
    }
    if clause_words.len() < 2 {
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
    if inner_tokens.len() > 3
        && inner_tokens[0].is_word("who")
        && inner_tokens[1].is_word("controls")
    {
        let mut effect_start = None;
        for idx in 2..inner_tokens.len() {
            if let Some(word) = inner_tokens[idx].as_word()
                && (word == "may"
                    || find_verb(&inner_tokens[idx..]).is_some_and(|(_, verb_idx)| verb_idx == 0))
            {
                effect_start = Some(idx);
                break;
            }
        }
        let effect_start = effect_start.ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing effect clause after 'each player who controls' (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;

        let filter_tokens = trim_commas(&inner_tokens[2..effect_start]);
        if filter_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing filter after 'each player who controls' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let filter_words = words(&filter_tokens);
        let (controls_most, normalized_filter_tokens) =
            if filter_words.starts_with(&["the", "most"]) {
                let start_idx =
                    token_index_for_word_index(&filter_tokens, 2).unwrap_or(filter_tokens.len());
                (true, trim_commas(&filter_tokens[start_idx..]))
            } else if filter_words.starts_with(&["most"]) {
                let start_idx =
                    token_index_for_word_index(&filter_tokens, 1).unwrap_or(filter_tokens.len());
                (true, trim_commas(&filter_tokens[start_idx..]))
            } else {
                (false, filter_tokens)
            };
        if normalized_filter_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing object filter after 'most' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let filter = parse_object_filter(&normalized_filter_tokens, false)?;

        let effect_tokens = trim_commas(&inner_tokens[effect_start..]);
        let branch_effects = if effect_tokens.iter().any(|token| token.is_word("may")) {
            let stripped = remove_first_word(&effect_tokens, "may");
            let inner_effects = parse_effect_chain_inner(&stripped)?;
            vec![EffectAst::May {
                effects: inner_effects,
            }]
        } else {
            parse_effect_chain_inner(&effect_tokens)?
        };

        let predicate = if controls_most {
            PredicateAst::PlayerControlsMost {
                player: PlayerAst::That,
                filter,
            }
        } else {
            PredicateAst::PlayerControls {
                player: PlayerAst::That,
                filter,
            }
        };
        let effects = vec![EffectAst::Conditional {
            predicate,
            if_true: branch_effects,
            if_false: Vec::new(),
        }];
        return Ok(Some(EffectAst::ForEachPlayer { effects }));
    }

    let inner_words = words(&inner_tokens);
    if inner_words.first().copied() == Some("who") {
        let tapped_land_turn_idx = inner_words
            .windows(7)
            .position(|window| window == ["tapped", "a", "land", "for", "mana", "this", "turn"])
            .map(|idx| idx + 6)
            .or_else(|| {
                inner_words
                    .windows(6)
                    .position(|window| window == ["tapped", "land", "for", "mana", "this", "turn"])
                    .map(|idx| idx + 5)
            });
        if let Some(turn_idx) = tapped_land_turn_idx {
            let effect_token_start = token_index_for_word_index(&inner_tokens, turn_idx + 1)
                .unwrap_or(inner_tokens.len());
            let effect_tokens = trim_commas(&inner_tokens[effect_token_start..]);
            if effect_tokens.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "missing effect after 'each player who tapped a land for mana this turn' (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            let branch_effects = if effect_tokens.iter().any(|token| token.is_word("may")) {
                let stripped = remove_first_word(&effect_tokens, "may");
                let inner_effects = parse_effect_chain_inner(&stripped)?;
                vec![EffectAst::May {
                    effects: inner_effects,
                }]
            } else {
                parse_effect_chain_inner(&effect_tokens)?
            };
            return Ok(Some(EffectAst::ForEachPlayer {
                effects: vec![EffectAst::Conditional {
                    predicate: PredicateAst::PlayerTappedLandForManaThisTurn {
                        player: PlayerAst::That,
                    },
                    if_true: branch_effects,
                    if_false: Vec::new(),
                }],
            }));
        }
    }
    if inner_words.first().copied() == Some("who")
        && let Some((negation_idx, negation_len)) = negated_action_word_index(&inner_words)
    {
        let effect_token_start = if let Some(comma_idx) = inner_tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)))
        {
            comma_idx + 1
        } else if let Some(this_way_idx) = inner_words
            .windows(2)
            .position(|pair| pair == ["this", "way"])
        {
            token_index_for_word_index(&inner_tokens, this_way_idx + 2)
                .unwrap_or(inner_tokens.len())
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
        let effects = parse_effect_chain_inner(&effect_tokens)?;
        return Ok(Some(EffectAst::ForEachPlayerDoesNot { effects }));
    }
    if inner_words.first().copied() == Some("who")
        && let Some(this_way_idx) = inner_words
            .windows(2)
            .position(|window| window == ["this", "way"])
    {
        let effect_start = this_way_idx + 2;
        let effect_token_start =
            token_index_for_word_index(&inner_tokens, effect_start).unwrap_or(inner_tokens.len());
        let effect_tokens = trim_commas(&inner_tokens[effect_token_start..]);
        if effect_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing effect after 'each player who ... this way' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let effects = parse_effect_chain_inner(&effect_tokens)?;
        let predicate = parse_who_did_this_way_predicate(&inner_tokens)?;
        return Ok(Some(EffectAst::ForEachPlayerDid { effects, predicate }));
    }
    if inner_words.starts_with(&["who", "does"])
        || inner_words.starts_with(&["who", "do"])
        || inner_words.starts_with(&["who", "did"])
    {
        let effect_token_start = if let Some(comma_idx) = inner_tokens
            .iter()
            .position(|token| matches!(token, Token::Comma(_)))
        {
            comma_idx + 1
        } else {
            token_index_for_word_index(&inner_tokens, 2).unwrap_or(inner_tokens.len())
        };
        let effect_tokens = trim_commas(&inner_tokens[effect_token_start..]);
        if effect_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing effect after 'each player who does' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let mut effects = parse_effect_chain_inner(&effect_tokens)?;
        for effect in &mut effects {
            bind_implicit_player_context(effect, PlayerAst::You);
        }
        return Ok(Some(EffectAst::ForEachPlayerDid {
            effects,
            predicate: None,
        }));
    }

    let effects = if inner_tokens.iter().any(|token| token.is_word("may")) {
        let stripped = remove_first_word(&inner_tokens, "may");
        let inner_effects = parse_effect_chain_inner(&stripped)?;
        vec![EffectAst::May {
            effects: inner_effects,
        }]
    } else {
        parse_effect_chain_inner(&inner_tokens)?
    };

    Ok(Some(EffectAst::ForEachPlayer { effects }))
}

fn parse_double_counters_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["double", "the", "number", "of"]) {
        return Ok(None);
    }

    let counters_idx = tokens
        .iter()
        .position(|token| token.is_word("counter") || token.is_word("counters"))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing counters keyword (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    if counters_idx <= 4 {
        return Err(CardTextError::ParseError(format!(
            "missing counter type (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let counter_type =
        parse_counter_type_from_tokens(&tokens[4..counters_idx]).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported counter type in double-counters clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;

    let on_idx = tokens[counters_idx + 1..]
        .iter()
        .position(|token| token.is_word("on"))
        .map(|offset| counters_idx + 1 + offset)
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing 'on' in double-counters clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;

    let mut filter_tokens = &tokens[on_idx + 1..];
    if filter_tokens
        .first()
        .is_some_and(|token| token.is_word("each") || token.is_word("all"))
    {
        filter_tokens = &filter_tokens[1..];
    }
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing filter in double-counters clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let filter = parse_object_filter(filter_tokens, false)?;
    Ok(Some(EffectAst::DoubleCountersOnEach {
        counter_type,
        filter,
    }))
}

fn parse_distribute_counters_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    parse_distribute_counters_sentence(tokens)
}

fn parse_tagged_cast_or_play_target(words: &[&str]) -> Option<(bool, usize)> {
    if words.starts_with(&["it"]) || words.starts_with(&["them"]) {
        return Some((false, 1));
    }
    if words.starts_with(&["that", "card"])
        || words.starts_with(&["those", "cards"])
        || words.starts_with(&["that", "spell"])
        || words.starts_with(&["those", "spells"])
        || words.starts_with(&["the", "card"])
        || words.starts_with(&["the", "cards"])
    {
        return Some((false, 2));
    }
    if words.starts_with(&["the", "copy"])
        || words.starts_with(&["that", "copy"])
        || words.starts_with(&["a", "copy"])
    {
        return Some((true, 2));
    }
    None
}

fn parse_until_end_of_turn_may_play_tagged_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let prefix_len = if clause_words.starts_with(&["until", "the", "end", "of", "turn"]) {
        5
    } else if clause_words.starts_with(&["until", "end", "of", "turn"]) {
        4
    } else {
        return Ok(None);
    };

    let tail = &clause_words[prefix_len..];
    if !tail.starts_with(&["you", "may", "play"]) {
        return Err(CardTextError::ParseError(format!(
            "unsupported until-end-of-turn permission clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let target_words = &tail[3..];
    let Some((as_copy, consumed)) = parse_tagged_cast_or_play_target(target_words) else {
        return Err(CardTextError::ParseError(format!(
            "unsupported until-end-of-turn play target (clause: '{}')",
            clause_words.join(" ")
        )));
    };
    if as_copy || consumed != target_words.len() {
        return Err(CardTextError::ParseError(format!(
            "unsupported until-end-of-turn play target (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(Some(EffectAst::GrantPlayTaggedUntilEndOfTurn {
        tag: TagKey::from(IT_TAG),
        player: PlayerAst::You,
    }))
}

fn parse_until_your_next_turn_may_play_tagged_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let prefix_len =
        if clause_words.starts_with(&["until", "the", "end", "of", "your", "next", "turn"]) {
            7
        } else if clause_words.starts_with(&["until", "end", "of", "your", "next", "turn"]) {
            6
        } else {
            return Ok(None);
        };

    let tail = &clause_words[prefix_len..];
    if !tail.starts_with(&["you", "may", "play"]) {
        return Err(CardTextError::ParseError(format!(
            "unsupported until-next-turn permission clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let target_words = &tail[3..];
    let is_supported_target = matches!(
        target_words,
        ["those", "cards"] | ["those", "card"] | ["them"] | ["it"] | ["that", "card"]
    );
    if !is_supported_target {
        return Err(CardTextError::ParseError(format!(
            "unsupported until-next-turn play target (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(Some(EffectAst::GrantPlayTaggedUntilYourNextTurn {
        tag: TagKey::from(IT_TAG),
        player: PlayerAst::You,
    }))
}

fn parse_cast_or_play_tagged_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let mut clause_words = words(tokens);
    while clause_words
        .first()
        .is_some_and(|word| *word == "then" || *word == "and")
    {
        clause_words.remove(0);
    }
    let Some(verb_word) = clause_words.first().copied() else {
        return Ok(None);
    };
    let is_cast = verb_word == "cast";
    let is_play = verb_word == "play";
    if !is_cast && !is_play {
        return Ok(None);
    }

    let rest = &clause_words[1..];
    let Some((as_copy, consumed)) = parse_tagged_cast_or_play_target(rest) else {
        return Ok(None);
    };
    let tail = &rest[consumed..];

    let has_this_turn_duration = tail == ["this", "turn"];
    let has_until_end_of_turn_duration = tail == ["until", "end", "of", "turn"]
        || tail == ["until", "the", "end", "of", "turn"];
    if has_this_turn_duration || has_until_end_of_turn_duration {
        return Ok(Some(EffectAst::GrantPlayTaggedUntilEndOfTurn {
            tag: TagKey::from(IT_TAG),
            player: PlayerAst::Implicit,
        }));
    }

    let without_paying_its_cost = tail == ["without", "paying", "its", "mana", "cost"]
        || tail == ["without", "paying", "their", "mana", "cost"];
    if tail.is_empty() || without_paying_its_cost {
        return Ok(Some(EffectAst::CastTagged {
            tag: TagKey::from(IT_TAG),
            allow_land: is_play,
            as_copy,
            without_paying_mana_cost: without_paying_its_cost,
        }));
    }

    Ok(None)
}

fn parse_prevent_next_time_damage_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["the", "next", "time"]) {
        return Ok(None);
    }

    let Some(would_idx) = clause_words.iter().position(|w| *w == "would") else {
        return Ok(None);
    };
    if clause_words.get(would_idx + 1..would_idx + 4) != Some(["deal", "damage", "to"].as_slice()) {
        return Ok(None);
    }

    // Must be "this turn ... prevent that damage".
    let this_turn_rel = clause_words[would_idx + 4..]
        .windows(2)
        .position(|window| window == ["this", "turn"])
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported prevent-next-time damage duration (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    let this_turn_idx = (would_idx + 4) + this_turn_rel;

    let tail = &clause_words[this_turn_idx + 2..];
    if tail != ["prevent", "that", "damage"] {
        return Ok(None);
    }

    // Parse source phrase (between "time" and "would").
    let source_words = &clause_words[3..would_idx];
    if source_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing prevent-next-time damage source (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let source = if source_words
        .windows(3)
        .any(|w| w == ["of", "your", "choice"])
    {
        PreventNextTimeDamageSourceAst::Choice
    } else {
        // Patterns like "a red source", "an artifact source", "black or red source".
        let mut words = source_words.to_vec();
        if words.first().is_some_and(|w| matches!(*w, "a" | "an")) {
            words.remove(0);
        }
        if words.last().copied() == Some("source") {
            words.pop();
        }
        if words.is_empty() {
            return Ok(Some(vec![EffectAst::PreventNextTimeDamage {
                source: PreventNextTimeDamageSourceAst::Filter(ObjectFilter::default()),
                target: PreventNextTimeDamageTargetAst::AnyTarget,
            }]));
        }

        let mut filter = ObjectFilter::default();
        let mut colors: Option<crate::color::ColorSet> = None;
        for w in words {
            if matches!(w, "or" | "and") {
                continue;
            }
            if let Some(color) = parse_color(w) {
                colors = Some(
                    colors
                        .unwrap_or_else(crate::color::ColorSet::new)
                        .union(color),
                );
                continue;
            }
            if let Some(card_type) = parse_card_type(w) {
                if !filter.card_types.contains(&card_type) {
                    filter.card_types.push(card_type);
                }
                continue;
            }
            if w == "shadow" {
                filter = filter.with_static_ability(StaticAbilityId::Shadow);
                continue;
            }
        }
        if let Some(colors) = colors {
            // If only colors were set, COLORLESS ORing is harmless due to contains-any semantics.
            filter.colors = Some(colors);
        }

        PreventNextTimeDamageSourceAst::Filter(filter)
    };

    // Parse target phrase (between "to" and "this turn").
    let target_words = &clause_words[would_idx + 4..this_turn_idx];
    let target = if target_words == ["you"] {
        PreventNextTimeDamageTargetAst::You
    } else if target_words == ["any", "target"] {
        PreventNextTimeDamageTargetAst::AnyTarget
    } else {
        return Err(CardTextError::ParseError(format!(
            "unsupported prevent-next-time damage target scope (clause: '{}')",
            clause_words.join(" ")
        )));
    };

    Ok(Some(vec![EffectAst::PreventNextTimeDamage {
        source,
        target,
    }]))
}

fn parse_redirect_next_damage_sentence(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.starts_with(&["the", "next", "time"]) {
        let Some(would_idx) = clause_words.iter().position(|word| *word == "would") else {
            return Ok(None);
        };
        if clause_words.get(would_idx + 1..would_idx + 4)
            != Some(["deal", "damage", "to"].as_slice())
        {
            return Ok(None);
        }

        let this_turn_rel = clause_words[would_idx + 4..]
            .windows(2)
            .position(|window| window == ["this", "turn"])
            .ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "unsupported redirected-next-time damage duration (clause: '{}')",
                    clause_words.join(" ")
                ))
            })?;
        let this_turn_idx = (would_idx + 4) + this_turn_rel;

        let source_words = &clause_words[3..would_idx];
        if source_words.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing redirected-next-time damage source (clause: '{}')",
                clause_words.join(" ")
            )));
        }

        let source = if source_words
            .windows(3)
            .any(|window| window == ["of", "your", "choice"])
        {
            PreventNextTimeDamageSourceAst::Choice
        } else {
            let mut words = source_words.to_vec();
            if words.first().is_some_and(|word| matches!(*word, "a" | "an")) {
                words.remove(0);
            }
            if words.last().copied() == Some("source") {
                words.pop();
            }
            let mut filter = ObjectFilter::default();
            let mut colors: Option<crate::color::ColorSet> = None;
            for word in words {
                if matches!(word, "or" | "and") {
                    continue;
                }
                if let Some(color) = parse_color(word) {
                    colors = Some(
                        colors
                            .unwrap_or_else(crate::color::ColorSet::new)
                            .union(color),
                    );
                    continue;
                }
                if let Some(card_type) = parse_card_type(word) {
                    if !filter.card_types.contains(&card_type) {
                        filter.card_types.push(card_type);
                    }
                    continue;
                }
                if word == "shadow" {
                    filter = filter.with_static_ability(StaticAbilityId::Shadow);
                    continue;
                }
            }
            if let Some(colors) = colors {
                filter.colors = Some(colors);
            }
            PreventNextTimeDamageSourceAst::Filter(filter)
        };

        let target_words = &clause_words[would_idx + 4..this_turn_idx];
        if target_words.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing redirected-next-time damage target (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let target_tokens = target_words
            .iter()
            .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
            .collect::<Vec<_>>();
        let target = parse_target_phrase(&target_tokens)?;

        let tail = &clause_words[this_turn_idx + 2..];
        if tail.len() < 7
            || !tail.starts_with(&["that", "damage", "is", "dealt", "to"])
            || tail.last().copied() != Some("instead")
        {
            return Ok(None);
        }
        let redirect_words = &tail[5..tail.len() - 1];
        let redirects_to_source = matches!(
            redirect_words,
            ["this"] | ["it"] | ["this", "creature"] | ["this", "permanent"]
        );
        if !redirects_to_source {
            return Err(CardTextError::ParseError(format!(
                "unsupported redirected-next-time damage destination (clause: '{}')",
                clause_words.join(" ")
            )));
        }

        return Ok(Some(vec![EffectAst::RedirectNextTimeDamageToSource {
            source,
            target,
        }]));
    }

    if !clause_words.starts_with(&["the", "next"]) {
        return Ok(None);
    }

    let amount_token = Token::Word(
        clause_words.get(2).copied().unwrap_or_default().to_string(),
        TextSpan::synthetic(),
    );
    let Some((amount, amount_used)) = parse_value(&[amount_token]) else {
        return Ok(None);
    };
    if amount_used != 1 {
        return Err(CardTextError::ParseError(format!(
            "unsupported redirected-next-damage amount (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let mut idx = 3usize;
    if clause_words.get(idx..idx + 6)
        != Some(["damage", "that", "would", "be", "dealt", "to"].as_slice())
    {
        return Ok(None);
    }
    idx += 6;

    let this_turn_rel = clause_words[idx..]
        .windows(2)
        .position(|window| window == ["this", "turn"])
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported redirected-next-damage duration (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    let this_turn_idx = idx + this_turn_rel;
    let protected_words = &clause_words[idx..this_turn_idx];
    let protects_source = matches!(
        protected_words,
        ["this"] | ["it"] | ["this", "creature"] | ["this", "permanent"]
    );
    if !protects_source {
        return Err(CardTextError::ParseError(format!(
            "unsupported redirected-next-damage protected target (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let tail = &clause_words[this_turn_idx + 2..];
    if tail.len() < 5
        || !tail.starts_with(&["is", "dealt", "to"])
        || tail.last().copied() != Some("instead")
    {
        return Ok(None);
    }

    let target_words = &tail[3..tail.len() - 1];
    if target_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing redirected-next-damage target (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let target_tokens = target_words
        .iter()
        .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
        .collect::<Vec<_>>();
    let target = parse_target_phrase(&target_tokens)?;

    Ok(Some(vec![EffectAst::RedirectNextDamageFromSourceToTarget {
        amount,
        target,
    }]))
}

fn parse_prevent_next_damage_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.first().copied() != Some("prevent") {
        return Ok(None);
    }

    let mut idx = 1usize;
    if clause_words.get(idx) == Some(&"the") {
        idx += 1;
    }
    if clause_words.get(idx) != Some(&"next") {
        return Ok(None);
    }
    idx += 1;

    let amount_token = Token::Word(
        clause_words
            .get(idx)
            .copied()
            .unwrap_or_default()
            .to_string(),
        TextSpan::synthetic(),
    );
    let Some((amount, amount_used)) = parse_value(&[amount_token]) else {
        return Err(CardTextError::ParseError(format!(
            "missing prevent damage amount (clause: '{}')",
            clause_words.join(" ")
        )));
    };
    idx += amount_used;

    if clause_words.get(idx) != Some(&"damage") {
        return Ok(None);
    }
    idx += 1;

    if clause_words.get(idx..idx + 4) != Some(["that", "would", "be", "dealt"].as_slice()) {
        return Err(CardTextError::ParseError(format!(
            "unsupported prevent-next damage clause tail (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    idx += 4;

    if clause_words.get(idx) != Some(&"to") {
        return Err(CardTextError::ParseError(format!(
            "unsupported prevent-next damage target scope (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    idx += 1;

    let this_turn_rel = clause_words[idx..]
        .windows(2)
        .position(|window| window == ["this", "turn"])
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported prevent-next damage duration (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    let this_turn_idx = idx + this_turn_rel;
    if this_turn_idx + 2 != clause_words.len() {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing prevent-next damage clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let target_words = &clause_words[idx..this_turn_idx];
    if target_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing prevent-next damage target (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let target_tokens = target_words
        .iter()
        .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
        .collect::<Vec<_>>();
    let target = parse_target_phrase(&target_tokens)?;

    Ok(Some(EffectAst::PreventDamage {
        amount,
        target,
        duration: Until::EndOfTurn,
    }))
}

fn parse_prevent_all_damage_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let prefix = [
        "prevent", "all", "damage", "that", "would", "be", "dealt", "to",
    ];
    if !clause_words.starts_with(&prefix) {
        return Ok(None);
    }
    if clause_words.len() <= prefix.len() + 1 {
        return Err(CardTextError::ParseError(format!(
            "missing prevent-all damage target (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    if clause_words[clause_words.len().saturating_sub(2)..] != ["this", "turn"] {
        return Err(CardTextError::ParseError(format!(
            "unsupported prevent-all damage duration (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let target_words = &clause_words[prefix.len()..clause_words.len() - 2];
    if target_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing prevent-all damage target (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let target_tokens = target_words
        .iter()
        .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
        .collect::<Vec<_>>();
    let target = parse_target_phrase(&target_tokens)?;

    Ok(Some(EffectAst::PreventAllDamageToTarget {
        target,
        duration: Until::EndOfTurn,
    }))
}

fn parse_can_attack_as_though_no_defender_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let Some(can_idx) = clause_words.iter().position(|word| *word == "can") else {
        return Ok(None);
    };
    let subject_words = &clause_words[..can_idx];
    let tail = &clause_words[can_idx..];
    let has_core = tail.starts_with(&["can", "attack"])
        && tail.windows(2).any(|window| window == ["as", "though"])
        && tail.contains(&"turn")
        && tail.contains(&"have")
        && tail.last().copied() == Some("defender");
    if !has_core {
        return Ok(None);
    }

    let target = if subject_words.is_empty() {
        TargetAst::Tagged(TagKey::from(IT_TAG), Some(TextSpan::synthetic()))
    } else {
        let subject_tokens = subject_words
            .iter()
            .map(|word| Token::Word((*word).to_string(), TextSpan::synthetic()))
            .collect::<Vec<_>>();
        parse_target_phrase(&subject_tokens)?
    };

    Ok(Some(EffectAst::GrantAbilitiesToTarget {
        target,
        abilities: vec![StaticAbility::can_attack_as_though_no_defender()],
        duration: Until::EndOfTurn,
    }))
}

fn parse_win_the_game_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.len() < 4 {
        return Ok(None);
    }

    if !clause_words.starts_with(&["you", "win", "the", "game"]) {
        return Ok(None);
    }

    if clause_words.len() == 4 {
        return Ok(Some(EffectAst::WinGame {
            player: PlayerAst::You,
        }));
    }

    if clause_words.get(4).copied() != Some("if") {
        return Ok(None);
    }

    let if_tail = clause_words.get(5..).unwrap_or_default();
    if if_tail.len() < 6
        || if_tail[0] != "you"
        || if_tail[1] != "own"
        || !matches!(if_tail[2], "a" | "an" | "the")
        || if_tail[3] != "card"
        || if_tail[4] != "named"
    {
        return Ok(None);
    }

    let after_named = &if_tail[5..];
    let Some(in_idx) = after_named.iter().position(|word| *word == "in") else {
        return Ok(None);
    };
    if in_idx == 0 {
        return Ok(None);
    }

    let name_words = &after_named[..in_idx];
    let remainder = &after_named[in_idx..];

    let has_exile = remainder.contains(&"exile");
    let has_hand = remainder.contains(&"hand");
    let has_graveyard = remainder.contains(&"graveyard");
    let has_battlefield = remainder.contains(&"battlefield");
    if !(has_exile && has_hand && has_graveyard && has_battlefield) {
        return Ok(None);
    }

    let name = name_words
        .iter()
        .map(|word| title_case_token_word(word))
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return Ok(None);
    }

    Ok(Some(EffectAst::Conditional {
        predicate: PredicateAst::PlayerOwnsCardNamedInZones {
            player: PlayerAst::You,
            name,
            zones: vec![Zone::Exile, Zone::Hand, Zone::Graveyard, Zone::Battlefield],
        },
        if_true: vec![EffectAst::WinGame {
            player: PlayerAst::You,
        }],
        if_false: Vec::new(),
    }))
}

fn parse_copy_spell_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    let Some(copy_idx) = tokens
        .iter()
        .position(|token| token.is_word("copy") || token.is_word("copies"))
    else {
        return Ok(None);
    };
    let simple_copy_reference = copy_idx == 0
        && matches!(
            clause_words.get(1).copied(),
            Some("it") | Some("this") | Some("that")
        );
    if simple_copy_reference {
        if let Some(then_idx) = tokens.iter().position(|token| token.is_word("then")) {
            let tail_tokens = trim_commas(&tokens[then_idx + 1..]);
            if let Some(spec) = parse_may_cast_it_sentence(&tail_tokens)
                && spec.as_copy
            {
                return Ok(Some(build_may_cast_tagged_effect(&spec)));
            }
        }
        let base = EffectAst::CopySpell {
            target: TargetAst::Source(None),
            count: Value::Fixed(1),
            player: PlayerAst::Implicit,
            may_choose_new_targets: false,
        };
        if let Some(if_idx) = tokens.iter().position(|token| token.is_word("if")) {
            let predicate_tokens = trim_commas(&tokens[if_idx + 1..]);
            if predicate_tokens.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "missing predicate after copy clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            let predicate = parse_predicate(&predicate_tokens)?;
            return Ok(Some(EffectAst::Conditional {
                predicate,
                if_true: vec![base],
                if_false: Vec::new(),
            }));
        }
        return Ok(Some(base));
    }
    if !clause_words.contains(&"spell") && !clause_words.contains(&"spells") {
        return Ok(None);
    }

    let subject = parse_subject(&tokens[..copy_idx]);
    let player = match subject {
        SubjectAst::Player(player) => player,
        SubjectAst::This => PlayerAst::Implicit,
    };

    let tail = &tokens[copy_idx + 1..];
    if tail.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing spell target in copy clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let mut split_idx = None;
    for idx in 0..tail.len() {
        if !tail[idx].is_word("and") {
            continue;
        }
        let mut after = words(&tail[idx + 1..]);
        if after.first().copied() == Some("may") {
            after.remove(0);
        }
        if after.first().copied() == Some("choose")
            && after
                .iter()
                .any(|word| *word == "target" || *word == "targets")
            && after.iter().any(|word| *word == "copy")
        {
            split_idx = Some(idx);
            break;
        }
    }

    let mut count = Value::Fixed(1);
    let mut copy_target_tail = if let Some(idx) = split_idx {
        &tail[..idx]
    } else {
        tail
    };
    if let Some(for_each_idx) = copy_target_tail
        .windows(2)
        .position(|window| window[0].is_word("for") && window[1].is_word("each"))
    {
        let count_filter_tokens = trim_commas(&copy_target_tail[for_each_idx + 2..]);
        if count_filter_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing count filter after 'for each' in copy clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let count_filter = parse_object_filter(&count_filter_tokens, false)?;
        count = Value::Count(count_filter);
        copy_target_tail = &copy_target_tail[..for_each_idx];
    }

    let copy_target_tokens = trim_commas(copy_target_tail);
    if copy_target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing spell target in copy clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let target_words = words(&copy_target_tokens);
    let target = if target_words.as_slice() == ["this", "spell"]
        || target_words.as_slice() == ["that", "spell"]
    {
        TargetAst::Source(None)
    } else {
        parse_target_phrase(&copy_target_tokens)?
    };

    let mut may_choose_new_targets = false;
    if let Some(idx) = split_idx {
        let mut choose_words = words(&tail[idx + 1..]);
        if choose_words.first().copied() == Some("may") {
            may_choose_new_targets = true;
            choose_words.remove(0);
        }
        let has_choose = choose_words.first().copied() == Some("choose");
        let has_new = choose_words.contains(&"new");
        let has_target = choose_words
            .iter()
            .any(|word| *word == "target" || *word == "targets");
        let has_copy = choose_words.contains(&"copy");
        if !has_choose || !has_target || !has_copy {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing copy clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        if !has_new {
            return Err(CardTextError::ParseError(format!(
                "missing 'new' in copy retarget clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    }

    Ok(Some(EffectAst::CopySpell {
        target,
        count,
        player,
        may_choose_new_targets,
    }))
}

fn parse_verb_first_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let Some(Token::Word(word, _)) = tokens.first() else {
        return Ok(None);
    };

    let verb = match word.as_str() {
        "add" => Verb::Add,
        "move" => Verb::Move,
        "counter" => Verb::Counter,
        "destroy" => Verb::Destroy,
        "exile" => Verb::Exile,
        "draw" => Verb::Draw,
        "deal" => Verb::Deal,
        "sacrifice" => Verb::Sacrifice,
        "create" => Verb::Create,
        "investigate" => Verb::Investigate,
        "proliferate" => Verb::Proliferate,
        "tap" => Verb::Tap,
        "attach" => Verb::Attach,
        "untap" => Verb::Untap,
        "scry" => Verb::Scry,
        "discard" => Verb::Discard,
        "transform" => Verb::Transform,
        "regenerate" => Verb::Regenerate,
        "mill" => Verb::Mill,
        "get" => Verb::Get,
        "remove" => Verb::Remove,
        "return" => Verb::Return,
        "exchange" => Verb::Exchange,
        "become" => Verb::Become,
        "skip" => Verb::Skip,
        "surveil" => Verb::Surveil,
        "shuffle" => Verb::Shuffle,
        "pay" => Verb::Pay,
        "goad" => Verb::Goad,
        "look" => Verb::Look,
        _ => return Ok(None),
    };

    let effect = parse_effect_with_verb(verb, None, &tokens[1..])?;
    Ok(Some(effect))
}

fn is_simple_chosen_object_reference(tokens: &[Token]) -> bool {
    let words: Vec<&str> = words(tokens)
        .into_iter()
        .filter(|word| !is_article(word) && *word != "then")
        .collect();
    if words.is_empty() {
        return false;
    }
    if words == ["it"] || words == ["them"] {
        return true;
    }
    if has_demonstrative_object_reference(&words) {
        return true;
    }
    false
}

fn parse_choose_target_and_verb_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["choose", "target"]) {
        return Ok(None);
    }

    let Some(and_idx) = tokens.iter().position(|token| token.is_word("and")) else {
        return Ok(None);
    };
    if and_idx <= 1 {
        return Ok(None);
    }

    let target_tokens = trim_commas(&tokens[1..and_idx]);
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing target after choose clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    if find_verb(&target_tokens).is_some() {
        return Ok(None);
    }

    let mut tail_tokens = trim_commas(&tokens[and_idx + 1..]);
    if tail_tokens
        .first()
        .is_some_and(|token| token.is_word("then"))
    {
        tail_tokens = tail_tokens[1..].to_vec();
    }
    if tail_tokens.is_empty() {
        return Ok(None);
    }

    let Some((verb, verb_idx)) = find_verb(&tail_tokens) else {
        return Ok(None);
    };
    if verb_idx != 0 {
        return Ok(None);
    }

    let rest_tokens = trim_commas(&tail_tokens[1..]);
    if !is_simple_chosen_object_reference(&rest_tokens) {
        return Ok(None);
    }

    let effect = parse_effect_with_verb(verb, None, &target_tokens)?;
    Ok(Some(effect))
}

fn parse_choose_target_prelude_sentence(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.first().copied() != Some("choose") {
        return Ok(None);
    }

    let target_tokens = trim_commas(&tokens[1..]);
    if target_tokens.is_empty() || !starts_with_target_indicator(&target_tokens) {
        return Ok(None);
    }
    if find_verb(&target_tokens).is_some() {
        return Ok(None);
    }

    let target = parse_target_phrase(&target_tokens)?;
    Ok(Some(EffectAst::TargetOnly { target }))
}

fn parse_keyword_mechanic_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    if tokens.is_empty() {
        return Ok(None);
    }

    let mut start = 0usize;
    if tokens.get(start).is_some_and(|token| token.is_word("then")) {
        start += 1;
    }
    if tokens.get(start).is_some_and(|token| token.is_word("you")) {
        start += 1;
    }
    if start >= tokens.len() {
        return Ok(None);
    }

    let clause_tokens = &tokens[start..];
    let clause_words = words(clause_tokens);
    if clause_words.is_empty() {
        return Ok(None);
    }

    if clause_words.first() == Some(&"amass") {
        return Err(CardTextError::ParseError(format!(
            "unsupported amass mechanic (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    if clause_words.starts_with(&["open", "an", "attraction"])
        || clause_words.starts_with(&["opens", "an", "attraction"])
    {
        return Ok(Some(EffectAst::OpenAttraction));
    }

    if clause_words == ["manifest", "dread"] {
        return Ok(Some(EffectAst::ManifestDread));
    }

    if matches!(
        clause_words.first().copied(),
        Some("bolster" | "support" | "adapt")
    ) {
        let keyword = clause_words[0];
        let (amount, used) = parse_number(&clause_tokens[1..]).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing numeric amount for {keyword} clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
        if 1 + used != clause_tokens.len() {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing {keyword} clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let effect = match keyword {
            "bolster" => EffectAst::Bolster { amount },
            "support" => EffectAst::Support { amount },
            "adapt" => EffectAst::Adapt { amount },
            _ => unreachable!(),
        };
        return Ok(Some(effect));
    }

    if clause_words.first() == Some(&"fateseal") {
        let (count, used) = parse_value(&clause_tokens[1..]).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing amount for fateseal clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
        if 1 + used != clause_tokens.len() {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing fateseal clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        return Ok(Some(EffectAst::Scry {
            count,
            player: PlayerAst::Opponent,
        }));
    }

    if matches!(
        clause_words.first().copied(),
        Some("discover" | "discovers")
    ) {
        let (count, used) = parse_value(&clause_tokens[1..]).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing amount for discover clause (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
        if 1 + used != clause_tokens.len() {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing discover clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        return Ok(Some(EffectAst::Discover {
            count,
            player: PlayerAst::You,
        }));
    }

    if matches!(clause_words.last().copied(), Some("explore" | "explores")) {
        let subject_tokens = &clause_tokens[..clause_tokens.len().saturating_sub(1)];
        let subject_words = words(subject_tokens);
        let target = if subject_words.is_empty()
            || subject_words == ["it"]
            || subject_words == ["this"]
            || subject_words == ["this", "creature"]
            || subject_words == ["this", "permanent"]
        {
            TargetAst::Source(span_from_tokens(subject_tokens))
        } else {
            parse_target_phrase(subject_tokens)?
        };
        return Ok(Some(EffectAst::Explore { target }));
    }

    Ok(None)
}

fn parse_connive_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let Some(connive_idx) = tokens
        .iter()
        .rposition(|token| token.is_word("connive") || token.is_word("connives"))
    else {
        return Ok(None);
    };

    // We currently only support trailing "connive/connives" clauses.
    if tokens[connive_idx + 1..]
        .iter()
        .any(|token| token.as_word().is_some())
    {
        return Ok(None);
    }

    let subject_tokens = &tokens[..connive_idx];
    if subject_tokens.is_empty() {
        return Ok(None);
    }

    let subject_words = words(subject_tokens);
    if subject_words == ["each", "creature", "that", "convoked", "this", "spell"] {
        return Ok(Some(EffectAst::ForEachTagged {
            tag: TagKey::from("convoked_this_spell"),
            effects: vec![EffectAst::ConniveIterated],
        }));
    }

    let target = parse_target_phrase(subject_tokens)?;
    Ok(Some(EffectAst::Connive { target }))
}

fn find_verb(tokens: &[Token]) -> Option<(Verb, usize)> {
    for (idx, token) in tokens.iter().enumerate() {
        let Some(word) = token.as_word() else {
            continue;
        };
        if matches!(word, "counter" | "counters")
            && tokens
                .get(idx + 1)
                .and_then(Token::as_word)
                .is_some_and(|next| matches!(next, "on" | "from" | "among"))
        {
            continue;
        }
        let verb = match word {
            "adds" | "add" => Verb::Add,
            "moves" | "move" => Verb::Move,
            "deals" | "deal" => Verb::Deal,
            "draws" | "draw" => Verb::Draw,
            "counters" | "counter" => Verb::Counter,
            "destroys" | "destroy" => Verb::Destroy,
            "exiles" | "exile" => Verb::Exile,
            "reveals" | "reveal" => Verb::Reveal,
            "looks" | "look" => Verb::Look,
            "loses" | "lose" => Verb::Lose,
            "gains" | "gain" => Verb::Gain,
            "puts" | "put" => Verb::Put,
            "sacrifices" | "sacrifice" => Verb::Sacrifice,
            "creates" | "create" => Verb::Create,
            "investigates" | "investigate" => Verb::Investigate,
            "proliferates" | "proliferate" => Verb::Proliferate,
            "taps" | "tap" => Verb::Tap,
            "attaches" | "attach" => Verb::Attach,
            "untaps" | "untap" => Verb::Untap,
            "scries" | "scry" => Verb::Scry,
            "discards" | "discard" => Verb::Discard,
            "transforms" | "transform" => Verb::Transform,
            "flips" | "flip" => Verb::Flip,
            "regenerates" | "regenerate" => Verb::Regenerate,
            "mills" | "mill" => Verb::Mill,
            "gets" | "get" => Verb::Get,
            "removes" | "remove" => Verb::Remove,
            "returns" | "return" => Verb::Return,
            "exchanges" | "exchange" => Verb::Exchange,
            "becomes" | "become" => Verb::Become,
            "switches" | "switch" => Verb::Switch,
            "skips" | "skip" => Verb::Skip,
            "surveils" | "surveil" => Verb::Surveil,
            "shuffles" | "shuffle" => Verb::Shuffle,
            "reorders" | "reorder" => Verb::Reorder,
            "pays" | "pay" => Verb::Pay,
            "goads" | "goad" => Verb::Goad,
            _ => continue,
        };
        return Some((verb, idx));
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubjectAst {
    This,
    Player(PlayerAst),
}

fn parse_subject(tokens: &[Token]) -> SubjectAst {
    let words = words(tokens);
    if words.is_empty() {
        return SubjectAst::This;
    }

    let mut start = 0usize;
    if words.starts_with(&["any", "number", "of"]) {
        start = 3;
    }

    let mut slice = &words[start..];
    while slice
        .first()
        .is_some_and(|word| *word == "then" || *word == "and")
    {
        slice = &slice[1..];
    }
    if let Some(have_idx) = slice
        .iter()
        .position(|word| *word == "have" || *word == "has")
    {
        if have_idx + 1 < slice.len() {
            slice = &slice[have_idx + 1..];
        }
    }

    if slice.starts_with(&["you"]) || slice.starts_with(&["your"]) {
        return SubjectAst::Player(PlayerAst::You);
    }

    if slice.starts_with(&["target", "opponent"]) || slice.starts_with(&["target", "opponents"]) {
        return SubjectAst::Player(PlayerAst::TargetOpponent);
    }

    if slice.starts_with(&["target", "player"]) || slice.starts_with(&["target", "players"]) {
        return SubjectAst::Player(PlayerAst::Target);
    }

    if slice.starts_with(&["opponent"])
        || slice.starts_with(&["opponents"])
        || slice.starts_with(&["an", "opponent"])
    {
        return SubjectAst::Player(PlayerAst::Opponent);
    }

    if slice.starts_with(&["defending", "player"]) {
        return SubjectAst::Player(PlayerAst::Defending);
    }
    if slice.ends_with(&["defending", "player"]) {
        return SubjectAst::Player(PlayerAst::Defending);
    }
    if slice.starts_with(&["attacking", "player"])
        || slice.starts_with(&["the", "attacking", "player"])
    {
        return SubjectAst::Player(PlayerAst::Attacking);
    }
    if slice.ends_with(&["attacking", "player"]) {
        return SubjectAst::Player(PlayerAst::Attacking);
    }

    if slice.starts_with(&["that", "player"]) {
        return SubjectAst::Player(PlayerAst::That);
    }

    if slice.starts_with(&["that", "players"]) || slice.starts_with(&["their"]) {
        return SubjectAst::Player(PlayerAst::That);
    }

    // Handle possessive references like "that creature's controller" /
    // "that permanent's controller" after tokenizer apostrophe normalization.
    if slice.len() >= 3
        && slice[0] == "that"
        && (slice[2] == "controller" || slice[2] == "owner")
        && (slice[1] == "creatures"
            || slice[1] == "permanents"
            || slice[1] == "sources"
            || slice[1] == "spells")
    {
        let player = if slice[2] == "owner" {
            PlayerAst::ItsOwner
        } else {
            PlayerAst::ItsController
        };
        return SubjectAst::Player(player);
    }

    if slice.starts_with(&["its", "controller"]) {
        return SubjectAst::Player(PlayerAst::ItsController);
    }
    if slice.starts_with(&["its", "owner"]) {
        return SubjectAst::Player(PlayerAst::ItsOwner);
    }
    if slice.starts_with(&["their", "owner"]) {
        return SubjectAst::Player(PlayerAst::ItsOwner);
    }
    if slice.ends_with(&["its", "controller"]) || slice.ends_with(&["their", "controller"]) {
        return SubjectAst::Player(PlayerAst::ItsController);
    }
    if slice.ends_with(&["its", "owner"]) || slice.ends_with(&["their", "owner"]) {
        return SubjectAst::Player(PlayerAst::ItsOwner);
    }

    if slice.starts_with(&["this"]) || slice.starts_with(&["thiss"]) {
        return SubjectAst::This;
    }

    SubjectAst::This
}

fn parse_effect_with_verb(
    verb: Verb,
    subject: Option<SubjectAst>,
    tokens: &[Token],
) -> Result<EffectAst, CardTextError> {
    match verb {
        Verb::Add => parse_add_mana(tokens, subject),
        Verb::Move => parse_move(tokens),
        Verb::Deal => parse_deal_damage(tokens),
        Verb::Draw => parse_draw(tokens, subject),
        Verb::Counter => parse_counter(tokens),
        Verb::Destroy => parse_destroy(tokens),
        Verb::Exile => parse_exile(tokens, subject),
        Verb::Reveal => parse_reveal(tokens, subject),
        Verb::Look => parse_look(tokens, subject),
        Verb::Lose => parse_lose_life(tokens, subject),
        Verb::Gain => {
            if tokens.first().is_some_and(|token| token.is_word("control")) {
                parse_gain_control(tokens, subject)
            } else {
                parse_gain_life(tokens, subject)
            }
        }
        Verb::Put => {
            let has_onto = tokens.iter().any(|token| token.is_word("onto"));
            let has_counter_words = tokens
                .iter()
                .any(|token| token.is_word("counter") || token.is_word("counters"));

            // Prefer zone moves like "... onto the battlefield" over counter placement because
            // "counter(s)" may appear in subordinate clauses (e.g. "mana value equal to the number
            // of charge counters on this artifact").
            if has_onto {
                if let Ok(effect) = parse_put_into_hand(tokens, subject) {
                    Ok(effect)
                } else if has_counter_words {
                    parse_put_counters(tokens)
                } else {
                    parse_put_into_hand(tokens, subject)
                }
            } else if has_counter_words {
                parse_put_counters(tokens)
            } else {
                parse_put_into_hand(tokens, subject)
            }
        }
        Verb::Sacrifice => parse_sacrifice(tokens, subject),
        Verb::Create => parse_create(tokens, subject),
        Verb::Investigate => parse_investigate(tokens),
        Verb::Proliferate => Ok(EffectAst::Proliferate),
        Verb::Tap => parse_tap(tokens),
        Verb::Attach => parse_attach(tokens),
        Verb::Untap => parse_untap(tokens),
        Verb::Scry => parse_scry(tokens, subject),
        Verb::Discard => parse_discard(tokens, subject),
        Verb::Transform => parse_transform(tokens),
        Verb::Flip => parse_flip(tokens),
        Verb::Regenerate => parse_regenerate(tokens),
        Verb::Mill => parse_mill(tokens, subject),
        Verb::Get => parse_get(tokens, subject),
        Verb::Remove => parse_remove(tokens),
        Verb::Return => parse_return(tokens),
        Verb::Exchange => parse_exchange(tokens),
        Verb::Become => parse_become(tokens, subject),
        Verb::Switch => parse_switch(tokens),
        Verb::Skip => parse_skip(tokens, subject),
        Verb::Surveil => parse_surveil(tokens, subject),
        Verb::Shuffle => parse_shuffle(tokens, subject),
        Verb::Reorder => parse_reorder(tokens, subject),
        Verb::Pay => parse_pay(tokens, subject),
        Verb::Goad => parse_goad(tokens),
    }
}

fn parse_look(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    fn parse_library_owner(words: &[&str]) -> Option<(PlayerAst, usize)> {
        if words.starts_with(&["your", "library"]) {
            return Some((PlayerAst::You, 2));
        }
        if words.starts_with(&["their", "library"]) {
            return Some((PlayerAst::That, 2));
        }
        if words.starts_with(&["that", "player", "library"])
            || words.starts_with(&["that", "players", "library"])
        {
            return Some((PlayerAst::That, 3));
        }
        if words.starts_with(&["target", "player", "library"])
            || words.starts_with(&["target", "players", "library"])
        {
            return Some((PlayerAst::Target, 3));
        }
        if words.starts_with(&["target", "opponent", "library"])
            || words.starts_with(&["target", "opponents", "library"])
        {
            return Some((PlayerAst::TargetOpponent, 3));
        }
        if words.starts_with(&["its", "owner", "library"])
            || words.starts_with(&["its", "owners", "library"])
        {
            return Some((PlayerAst::ItsOwner, 3));
        }
        if words.starts_with(&["his", "or", "her", "library"]) {
            return Some((PlayerAst::That, 4));
        }
        None
    }

    // "Look at the top N cards of your library."
    let mut clause_tokens = trim_commas(tokens);
    if clause_tokens
        .first()
        .is_some_and(|token| token.is_word("at"))
    {
        clause_tokens = trim_commas(&clause_tokens[1..]);
    }
    let clause_words = words(&clause_tokens);

    let Some(top_idx) = clause_tokens.iter().position(|t| t.is_word("top")) else {
        return Err(CardTextError::ParseError(format!(
            "unsupported look clause (clause: '{}')",
            clause_words.join(" ")
        )));
    };
    if top_idx + 1 >= clause_tokens.len() {
        return Err(CardTextError::ParseError(format!(
            "missing look top noun (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let mut idx = top_idx + 1;
    let count = if clause_tokens
        .get(idx)
        .and_then(Token::as_word)
        .is_some_and(|w| w == "card" || w == "cards")
    {
        Value::Fixed(1)
    } else {
        let (value, used) = parse_value(&clause_tokens[idx..]).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing look count (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
        idx += used;
        value
    };

    // Consume "card(s)"
    if clause_tokens
        .get(idx)
        .and_then(Token::as_word)
        .is_some_and(|w| w == "card" || w == "cards")
    {
        idx += 1;
    } else {
        return Err(CardTextError::ParseError(format!(
            "missing look card noun (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    // Consume "of <player> library"
    if !clause_tokens.get(idx).is_some_and(|t| t.is_word("of")) {
        return Err(CardTextError::ParseError(format!(
            "missing 'of' in look clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    idx += 1;
    let mut owner_tokens = &clause_tokens[idx..];
    while owner_tokens
        .first()
        .is_some_and(|t| t.is_word("the") || t.is_word("a") || t.is_word("an"))
    {
        owner_tokens = &owner_tokens[1..];
    }
    let owner_words = words(owner_tokens);
    let (player, used_words) = parse_library_owner(&owner_words)
        .or_else(|| {
            // If the clause uses a subject ("target player looks ..."), treat that as the default.
            subject.and_then(|s| match s {
                SubjectAst::Player(p) => Some((p, 0)),
                _ => None,
            })
        })
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported look library owner (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    // No trailing words supported for now (based on word tokens).
    if used_words < owner_words.len() {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing look clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(EffectAst::LookAtTopCards {
        player,
        count,
        tag: TagKey::from(IT_TAG),
    })
}

fn parse_reorder(
    tokens: &[Token],
    _subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let clause = words(tokens).join(" ");
    let clause_words = words(tokens);
    if clause_words.is_empty() {
        return Err(CardTextError::ParseError(
            "missing reorder target".to_string(),
        ));
    }

    let (player, rest) = if clause_words.starts_with(&["your", "graveyard"]) {
        (PlayerAst::You, &clause_words[2..])
    } else if clause_words.starts_with(&["their", "graveyard"]) {
        (PlayerAst::That, &clause_words[2..])
    } else if clause_words.starts_with(&["that", "player", "graveyard"])
        || clause_words.starts_with(&["that", "players", "graveyard"])
    {
        (PlayerAst::That, &clause_words[3..])
    } else if clause_words.starts_with(&["its", "controller", "graveyard"])
        || clause_words.starts_with(&["its", "controllers", "graveyard"])
    {
        (PlayerAst::ItsController, &clause_words[3..])
    } else if clause_words.starts_with(&["its", "owner", "graveyard"])
        || clause_words.starts_with(&["its", "owners", "graveyard"])
    {
        (PlayerAst::ItsOwner, &clause_words[3..])
    } else {
        return Err(CardTextError::ParseError(format!(
            "unsupported reorder clause (clause: '{clause}')"
        )));
    };

    if !rest.is_empty() && rest != ["as", "you", "choose"] {
        return Err(CardTextError::ParseError(format!(
            "unsupported reorder clause tail (clause: '{clause}')"
        )));
    }

    Ok(EffectAst::ReorderGraveyard { player })
}

fn parse_shuffle(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    fn is_simple_library_phrase(words: &[&str]) -> bool {
        matches!(
            words,
            ["library"]
                | ["your", "library"]
                | ["their", "library"]
                | ["that", "player", "library"]
                | ["that", "players", "library"]
                | ["its", "owner", "library"]
                | ["its", "owners", "library"]
                | ["his", "or", "her", "library"]
        )
    }

    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    if tokens.is_empty() {
        // Support standalone "Shuffle." clauses. If the sentence includes an explicit player
        // subject, use it; otherwise return an implicit player that can be filled in by the
        // carry-context logic (and compiles to "you" by default).
        return Ok(EffectAst::ShuffleLibrary { player });
    }

    let clause_words = words(tokens);
    if clause_words.contains(&"graveyard")
        || clause_words.contains(&"cards")
        || clause_words.contains(&"card")
        || clause_words.contains(&"into")
        || clause_words.contains(&"from")
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported shuffle clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    if is_simple_library_phrase(&clause_words) {
        return Ok(EffectAst::ShuffleLibrary { player });
    }

    Err(CardTextError::ParseError(format!(
        "unsupported shuffle clause (clause: '{}')",
        clause_words.join(" ")
    )))
}

fn parse_goad(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let target_tokens = trim_commas(tokens);
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError("missing goad target".to_string()));
    }

    let target_words = words(&target_tokens);
    if target_words.as_slice() == ["it"] || target_words.as_slice() == ["them"] {
        return Ok(EffectAst::Goad {
            target: TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(&target_tokens)),
        });
    }

    let target = parse_target_phrase(&target_tokens)?;
    if matches!(
        target,
        TargetAst::Player(_, _) | TargetAst::PlayerOrPlaneswalker(_, _)
    ) {
        return Err(CardTextError::ParseError(format!(
            "goad target must be a creature (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    Ok(EffectAst::Goad { target })
}

fn parse_attach_object_phrase(tokens: &[Token]) -> Result<TargetAst, CardTextError> {
    let object_words = words(tokens);
    let object_span = span_from_tokens(tokens);
    if object_words.is_empty() {
        return Err(CardTextError::ParseError(
            "missing object to attach".to_string(),
        ));
    }

    let is_source_attachment = is_source_reference_words(&object_words)
        || object_words.starts_with(&["this", "equipment"])
        || object_words.starts_with(&["this", "aura"])
        || object_words.starts_with(&["this", "enchantment"])
        || object_words.starts_with(&["this", "artifact"]);
    if is_source_attachment {
        return Ok(TargetAst::Source(object_span));
    }

    if matches!(object_words.as_slice(), ["it"] | ["them"]) {
        return Ok(TargetAst::Tagged(TagKey::from(IT_TAG), object_span));
    }

    let mut tagged_filter = ObjectFilter::default();
    if matches!(
        object_words.as_slice(),
        ["that", "equipment"] | ["those", "equipment"]
    ) {
        tagged_filter.zone = Some(Zone::Battlefield);
        tagged_filter.card_types.push(CardType::Artifact);
        tagged_filter.subtypes.push(Subtype::Equipment);
    } else if matches!(
        object_words.as_slice(),
        ["that", "aura"] | ["those", "auras"]
    ) {
        tagged_filter.zone = Some(Zone::Battlefield);
        tagged_filter.card_types.push(CardType::Enchantment);
        tagged_filter.subtypes.push(Subtype::Aura);
    } else if matches!(
        object_words.as_slice(),
        ["that", "artifact"] | ["those", "artifacts"]
    ) {
        tagged_filter.zone = Some(Zone::Battlefield);
        tagged_filter.card_types.push(CardType::Artifact);
    } else if object_words.as_slice() == ["that", "enchantment"] {
        tagged_filter.zone = Some(Zone::Battlefield);
        tagged_filter.card_types.push(CardType::Enchantment);
    }

    if tagged_filter.zone.is_some() {
        tagged_filter
            .tagged_constraints
            .push(TaggedObjectConstraint {
                tag: TagKey::from(IT_TAG),
                relation: TaggedOpbjectRelation::IsTaggedObject,
            });
        return Ok(TargetAst::Object(tagged_filter, object_span, None));
    }

    if tokens.first().is_some_and(|token| token.is_word("target"))
        && let Some(attached_idx) = tokens.iter().position(|token| token.is_word("attached"))
        && tokens
            .get(attached_idx + 1)
            .is_some_and(|token| token.is_word("to"))
    {
        let head_tokens = trim_commas(&tokens[..attached_idx]);
        if !head_tokens.is_empty() {
            return parse_target_phrase(&head_tokens);
        }
    }

    parse_target_phrase(tokens)
}

fn parse_attach(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    if tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "attach clause missing object and destination".to_string(),
        ));
    }

    let Some(to_idx) = tokens.iter().rposition(|token| token.is_word("to")) else {
        return Err(CardTextError::ParseError(format!(
            "attach clause missing destination (clause: '{}')",
            clause_words.join(" ")
        )));
    };
    if to_idx == 0 || to_idx + 1 >= tokens.len() {
        return Err(CardTextError::ParseError(format!(
            "attach clause missing object or destination (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let object_tokens = trim_commas(&tokens[..to_idx]);
    let target_tokens = trim_commas(&tokens[to_idx + 1..]);
    if object_tokens.is_empty() || target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "attach clause missing object or destination (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let object = parse_attach_object_phrase(&object_tokens)?;
    let target_words = words(&target_tokens);
    let target = if matches!(target_words.as_slice(), ["it"] | ["them"]) {
        TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(&target_tokens))
    } else {
        parse_target_phrase(&target_tokens)?
    };

    Ok(EffectAst::Attach { object, target })
}

fn parse_deal_damage(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    let is_divided_as_you_choose_clause = clause_words.contains(&"divided")
        && clause_words.contains(&"choose")
        && clause_words.contains(&"among");
    if is_divided_as_you_choose_clause {
        return Err(CardTextError::ParseError(format!(
            "unsupported divided-damage distribution clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    if let Some(effect) = parse_deal_damage_equal_to_clause(tokens)? {
        return Ok(effect);
    }
    if let Some(effect) = parse_deal_damage_to_target_equal_to_clause(tokens)? {
        return Ok(effect);
    }
    if clause_words.starts_with(&["that", "much"]) {
        return parse_deal_damage_with_amount(tokens, Value::EventValue(EventValueSpec::Amount), 2);
    }

    if let Some((value, used)) = parse_value(tokens) {
        return parse_deal_damage_with_amount(tokens, value, used);
    }

    if clause_words.starts_with(&["damage", "to", "each", "opponent"])
        && clause_words.contains(&"number")
        && clause_words.contains(&"cards")
        && clause_words.contains(&"hand")
    {
        let value = Value::CardsInHand(PlayerFilter::IteratedPlayer);
        return Ok(EffectAst::ForEachOpponent {
            effects: vec![EffectAst::DealDamage {
                amount: value,
                target: TargetAst::Player(PlayerFilter::IteratedPlayer, None),
            }],
        });
    }

    Err(CardTextError::ParseError(format!(
        "missing damage amount (clause: '{}')",
        clause_words.join(" ")
    )))
}

fn parse_deal_damage_to_target_equal_to_clause(
    tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["damage", "to"]) {
        return Ok(None);
    }

    let Some(equal_word_idx) = clause_words
        .windows(2)
        .position(|window| window == ["equal", "to"])
    else {
        return Ok(None);
    };
    let Some(equal_token_idx) = token_index_for_word_index(tokens, equal_word_idx) else {
        return Ok(None);
    };

    let mut target_tokens = trim_commas(&tokens[1..equal_token_idx]);
    if target_tokens
        .first()
        .is_some_and(|token| token.is_word("to"))
    {
        target_tokens.remove(0);
    }
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing damage target in equal-to clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let amount = parse_add_mana_equal_amount_value(tokens)
        .or(parse_equal_to_aggregate_filter_value(tokens))
        .or(parse_equal_to_number_of_filter_value(tokens))
        .or(parse_dynamic_cost_modifier_value(tokens)?)
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing damage amount (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
    let target = parse_target_phrase(&target_tokens)?;
    Ok(Some(EffectAst::DealDamage { amount, target }))
}

fn parse_deal_damage_equal_to_clause(tokens: &[Token]) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["damage", "equal", "to"]) {
        return Ok(None);
    }

    let mut target_to_idx = None;
    for idx in 3..tokens.len() {
        if !tokens[idx].is_word("to") {
            continue;
        }
        let tail_words = words(&tokens[idx + 1..]);
        if tail_words.is_empty() {
            continue;
        }
        let looks_like_target = tail_words.contains(&"target")
            || matches!(
                tail_words.first().copied(),
                Some(
                    "any"
                        | "each"
                        | "all"
                        | "it"
                        | "itself"
                        | "them"
                        | "him"
                        | "her"
                        | "that"
                        | "this"
                        | "you"
                        | "player"
                        | "opponent"
                        | "creature"
                        | "planeswalker"
                )
            );
        if looks_like_target {
            target_to_idx = Some(idx);
            break;
        }
    }

    let Some(target_to_idx) = target_to_idx else {
        return Err(CardTextError::ParseError(format!(
            "missing damage target in equal-to clause (clause: '{}')",
            clause_words.join(" ")
        )));
    };

    let amount_tokens = &tokens[..target_to_idx];
    let amount = parse_add_mana_equal_amount_value(amount_tokens)
        .or(parse_dynamic_cost_modifier_value(amount_tokens)?)
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing damage amount (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;

    let target_tokens = &tokens[target_to_idx + 1..];
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing damage target in equal-to clause (clause: '{}')",
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
    let target = parse_target_phrase(normalized_target_tokens)?;
    Ok(Some(EffectAst::DealDamage { amount, target }))
}

fn parse_deal_damage_with_amount(
    tokens: &[Token],
    amount: Value,
    used: usize,
) -> Result<EffectAst, CardTextError> {
    let rest = &tokens[used..];
    let Some(Token::Word(word, _)) = rest.first() else {
        return Err(CardTextError::ParseError(
            "missing damage keyword".to_string(),
        ));
    };
    if word != "damage" {
        return Err(CardTextError::ParseError(
            "missing damage keyword".to_string(),
        ));
    }

    let mut target_tokens = &rest[1..];
    if target_tokens
        .first()
        .is_some_and(|token| token.is_word("to"))
    {
        target_tokens = &target_tokens[1..];
    }
    if let Some(among_idx) = target_tokens
        .iter()
        .position(|token| token.is_word("among"))
    {
        let among_tail = &target_tokens[among_idx + 1..];
        if among_tail.iter().any(|token| token.is_word("target"))
            && among_tail.iter().any(|token| {
                token.is_word("player")
                    || token.is_word("players")
                    || token.is_word("creature")
                    || token.is_word("creatures")
            })
        {
            target_tokens = among_tail;
        }
    }

    if target_tokens.iter().any(|token| token.is_word("where")) {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing where damage clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    if let Some(instead_idx) = target_tokens
        .iter()
        .position(|token| token.is_word("instead"))
        && target_tokens
            .get(instead_idx + 1)
            .is_some_and(|token| token.is_word("if"))
    {
        let pre_target_tokens = trim_commas(&target_tokens[..instead_idx]);
        let condition_tokens = trim_commas(&target_tokens[instead_idx + 2..]);
        let predicate =
            if let Some(predicate) = parse_instead_if_control_predicate(&condition_tokens)? {
                predicate
            } else {
                parse_predicate(&condition_tokens)?
            };
        let target = if pre_target_tokens.is_empty() {
            TargetAst::PlayerOrPlaneswalker(PlayerFilter::Any, None)
        } else {
            parse_target_phrase(&pre_target_tokens)?
        };
        return Ok(EffectAst::Conditional {
            predicate,
            if_true: vec![EffectAst::DealDamage {
                amount: amount.clone(),
                target,
            }],
            if_false: Vec::new(),
        });
    }

    if let Some(if_idx) = target_tokens.iter().position(|token| token.is_word("if")) {
        let pre_target_tokens = trim_commas(&target_tokens[..if_idx]);
        let condition_tokens = trim_commas(&target_tokens[if_idx + 1..]);
        if pre_target_tokens.is_empty() || condition_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing if clause in damage effect (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        let predicate = parse_predicate(&condition_tokens)?;
        let target = parse_target_phrase(&pre_target_tokens)?;
        return Ok(EffectAst::Conditional {
            predicate,
            if_true: vec![EffectAst::DealDamage { amount, target }],
            if_false: Vec::new(),
        });
    }

    let target_words = words(target_tokens);
    if target_words.starts_with(&["each", "of"]) {
        let each_of_tokens = &target_tokens[2..];
        let each_of_words = words(each_of_tokens);
        if each_of_words.iter().any(|word| *word == "target") {
            let target = parse_target_phrase(each_of_tokens)?;
            return Ok(EffectAst::DealDamage { amount, target });
        }
    }
    if target_words.as_slice() == ["each", "player"]
        || target_words.as_slice() == ["each", "players"]
    {
        return Ok(EffectAst::ForEachPlayer {
            effects: vec![EffectAst::DealDamage {
                amount: amount.clone(),
                target: TargetAst::Player(PlayerFilter::IteratedPlayer, None),
            }],
        });
    }
    if target_words.as_slice() == ["each", "opponent"]
        || target_words.as_slice() == ["each", "opponents"]
    {
        return Ok(EffectAst::ForEachOpponent {
            effects: vec![EffectAst::DealDamage {
                amount: amount.clone(),
                target: TargetAst::Player(PlayerFilter::IteratedPlayer, None),
            }],
        });
    }
    if target_words.starts_with(&["each", "opponent", "who"])
        && target_words
            .windows(2)
            .any(|window| window == ["this", "way"])
    {
        let predicate = parse_who_did_this_way_predicate(&target_tokens[2..])?;
        return Ok(EffectAst::ForEachOpponentDid {
            effects: vec![EffectAst::DealDamage {
                amount: amount.clone(),
                target: TargetAst::Player(PlayerFilter::IteratedPlayer, None),
            }],
            predicate,
        });
    }
    if target_words.starts_with(&["each", "player", "who"])
        && target_words
            .windows(2)
            .any(|window| window == ["this", "way"])
    {
        let predicate = parse_who_did_this_way_predicate(&target_tokens[2..])?;
        return Ok(EffectAst::ForEachPlayerDid {
            effects: vec![EffectAst::DealDamage {
                amount: amount.clone(),
                target: TargetAst::Player(PlayerFilter::IteratedPlayer, None),
            }],
            predicate,
        });
    }

    if matches!(target_words.first(), Some(&"each") | Some(&"all"))
        && let Some(and_each_idx) = target_words.windows(3).position(|window| {
            window == ["and", "each", "player"] || window == ["and", "each", "players"]
        })
        && and_each_idx >= 1
        && and_each_idx + 3 == target_words.len()
    {
        let filter_tokens = &target_tokens[1..and_each_idx];
        let mut filter = parse_object_filter(filter_tokens, false)?;
        if filter.controller.is_none() {
            filter.controller = Some(PlayerFilter::IteratedPlayer);
        }
        return Ok(EffectAst::ForEachPlayer {
            effects: vec![
                EffectAst::DealDamage {
                    amount: amount.clone(),
                    target: TargetAst::Player(PlayerFilter::IteratedPlayer, None),
                },
                EffectAst::DealDamageEach {
                    amount: amount.clone(),
                    filter,
                },
            ],
        });
    }

    if target_words.starts_with(&["each", "opponent", "and", "each"])
        && target_words.contains(&"creature")
        && target_words.contains(&"planeswalker")
        && (target_words
            .windows(2)
            .any(|pair| pair == ["they", "control"])
            || target_words
                .windows(3)
                .any(|triplet| triplet == ["that", "player", "controls"]))
    {
        let mut filter = ObjectFilter::default();
        filter.card_types = vec![CardType::Creature, CardType::Planeswalker];
        filter.controller = Some(PlayerFilter::IteratedPlayer);
        return Ok(EffectAst::ForEachOpponent {
            effects: vec![
                EffectAst::DealDamage {
                    amount: amount.clone(),
                    target: TargetAst::Player(PlayerFilter::IteratedPlayer, None),
                },
                EffectAst::DealDamageEach {
                    amount: amount.clone(),
                    filter,
                },
            ],
        });
    }

    if matches!(target_words.first(), Some(&"each") | Some(&"all")) {
        if target_tokens.len() < 2 {
            return Err(CardTextError::ParseError(
                "missing damage target filter after 'each'".to_string(),
            ));
        }
        let filter_tokens = &target_tokens[1..];
        let filter = parse_object_filter(filter_tokens, false)?;
        return Ok(EffectAst::DealDamageEach {
            amount: amount.clone(),
            filter,
        });
    }

    if let Some(at_idx) = target_tokens.iter().position(|token| token.is_word("at")) {
        let timing_words = words(&target_tokens[at_idx..]);
        let matches_end_of_combat = timing_words.as_slice() == ["at", "end", "of", "combat"]
            || timing_words.as_slice() == ["at", "the", "end", "of", "combat"];
        if matches_end_of_combat && at_idx >= 1 {
            let pre_target_tokens = trim_commas(&target_tokens[..at_idx]);
            if !pre_target_tokens.is_empty() {
                let target = parse_target_phrase(&pre_target_tokens)?;
                return Ok(EffectAst::DelayedUntilEndOfCombat {
                    effects: vec![EffectAst::DealDamage { amount, target }],
                });
            }
        }
    }

    let target = parse_target_phrase(&target_tokens)?;
    Ok(EffectAst::DealDamage { amount, target })
}

fn parse_instead_if_control_predicate(
    tokens: &[Token],
) -> Result<Option<PredicateAst>, CardTextError> {
    let clause_words = words(tokens);
    let starts_with_you_control = clause_words.starts_with(&["you", "control"])
        || clause_words.starts_with(&["you", "controlled"]);
    if !starts_with_you_control {
        return Ok(None);
    }

    let mut filter_tokens = &tokens[2..];
    let mut min_count: Option<u32> = None;
    if let Some((count, used)) = parse_number(filter_tokens)
        && count > 1
    {
        let tail = &filter_tokens[used..];
        if tail.first().is_some_and(|token| token.is_word("or"))
            && tail.get(1).is_some_and(|token| token.is_word("more"))
        {
            min_count = Some(count);
            filter_tokens = &tail[2..];
        } else if tail.first().is_some_and(|token| token.is_word("or"))
            && tail.get(1).is_some_and(|token| token.is_word("fewer"))
        {
            // Keep unsupported "or fewer" variants as plain control checks for now.
            filter_tokens = &tail[2..];
        }
    }
    if filter_tokens
        .first()
        .is_some_and(|token| token.is_word("at"))
        && filter_tokens
            .get(1)
            .is_some_and(|token| token.is_word("least"))
        && let Some((count, used)) = parse_number(&filter_tokens[2..])
        && count > 1
    {
        min_count = Some(count);
        filter_tokens = &filter_tokens[2 + used..];
    }
    let cut_markers: &[&[&str]] = &[&["as", "you", "cast", "this", "spell"], &["this", "turn"]];
    for marker in cut_markers {
        if let Some(idx) = words(filter_tokens)
            .windows(marker.len())
            .position(|window| window == *marker)
        {
            let cut_idx =
                token_index_for_word_index(filter_tokens, idx).unwrap_or(filter_tokens.len());
            filter_tokens = &filter_tokens[..cut_idx];
            break;
        }
    }
    let mut filter_tokens = trim_commas(filter_tokens);
    let filter_words = words(&filter_tokens);
    let mut requires_different_powers = false;
    if filter_words.ends_with(&["with", "different", "powers"])
        || filter_words.ends_with(&["with", "different", "power"])
    {
        requires_different_powers = true;
        let cut_word_idx = filter_words.len().saturating_sub(3);
        let cut_token_idx =
            token_index_for_word_index(&filter_tokens, cut_word_idx).unwrap_or(filter_tokens.len());
        filter_tokens = trim_commas(&filter_tokens[..cut_token_idx]);
    }
    if filter_tokens.is_empty() {
        return Ok(None);
    }

    let other = filter_tokens
        .first()
        .is_some_and(|token| token.is_word("another") || token.is_word("other"));
    let filter = parse_object_filter(&filter_tokens, other)?;
    if let Some(count) = min_count {
        if requires_different_powers {
            return Ok(Some(
                PredicateAst::PlayerControlsAtLeastWithDifferentPowers {
                    player: PlayerAst::You,
                    filter,
                    count,
                },
            ));
        }
        Ok(Some(PredicateAst::PlayerControlsAtLeast {
            player: PlayerAst::You,
            filter,
            count,
        }))
    } else {
        Ok(Some(PredicateAst::PlayerControls {
            player: PlayerAst::You,
            filter,
        }))
    }
}

fn parse_move(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["all", "counters", "from"]) {
        return Err(CardTextError::ParseError(format!(
            "unsupported move clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let from_idx = tokens
        .iter()
        .position(|token| token.is_word("from"))
        .unwrap_or(2);
    let onto_idx = tokens
        .iter()
        .position(|token| token.is_word("onto"))
        .or_else(|| tokens.iter().position(|token| token.is_word("to")))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing move destination (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;

    let from_tokens = &tokens[from_idx + 1..onto_idx];
    let to_tokens = &tokens[onto_idx + 1..];
    let from = parse_target_phrase(from_tokens)?;
    let to = parse_target_phrase(to_tokens)?;

    Ok(EffectAst::MoveAllCounters { from, to })
}

fn parse_draw(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    let mut parsed_that_many_minus_one = false;
    let mut parsed_that_many_plus_one = false;
    let mut consumed_embedded_card_keyword = false;
    let (mut count, used) = if clause_words.starts_with(&["that", "many"]) {
        let mut value = Value::EventValue(EventValueSpec::Amount);
        let consumed = 2usize;
        let rest = &tokens[consumed..];
        if rest
            .first()
            .is_some_and(|token| token.is_word("card") || token.is_word("cards"))
        {
            let trailing = trim_commas(&rest[1..]);
            let trailing_words = words(&trailing);
            if trailing_words.as_slice() == ["minus", "one"] {
                value = Value::EventValueOffset(EventValueSpec::Amount, -1);
                parsed_that_many_minus_one = true;
            } else if trailing_words.as_slice() == ["plus", "one"] {
                value = Value::EventValueOffset(EventValueSpec::Amount, 1);
                parsed_that_many_plus_one = true;
            } else if !trailing_words.is_empty()
                && !(trailing_words
                    .windows(2)
                    .any(|window| window[0] == "for" && window[1] == "each"))
            {
                return Err(CardTextError::ParseError(format!(
                    "unsupported trailing draw clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
        }
        (value, consumed)
    } else if let Some(value) = parse_draw_as_many_cards_value(tokens) {
        consumed_embedded_card_keyword = true;
        (value, tokens.len())
    } else if tokens
        .first()
        .is_some_and(|token| token.is_word("another"))
        && tokens
            .get(1)
            .is_some_and(|token| token.is_word("card") || token.is_word("cards"))
    {
        (Value::Fixed(1), 1)
    } else if tokens
        .first()
        .is_some_and(|token| token.is_word("card") || token.is_word("cards"))
    {
        let tail = trim_commas(&tokens[1..]);
        let value = parse_draw_card_prefixed_count_value(&tail)?.ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing draw count (clause: '{}')",
                clause_words.join(" ")
            ))
        })?;
        consumed_embedded_card_keyword = true;
        (value, tokens.len())
    } else {
        parse_value(tokens).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing draw count (clause: '{}')",
                clause_words.join(" ")
            ))
        })?
    };

    let rest = &tokens[used..];
    let tail = if consumed_embedded_card_keyword {
        trim_commas(rest)
    } else {
        let mut card_word_idx = 0usize;
        if rest
            .first()
            .is_some_and(|token| token.is_word("additional"))
        {
            card_word_idx = 1;
        }
        let Some(card_word) = rest.get(card_word_idx).and_then(Token::as_word) else {
            return Err(CardTextError::ParseError(
                "missing card keyword".to_string(),
            ));
        };
        if card_word != "card" && card_word != "cards" {
            return Err(CardTextError::ParseError(
                "missing card keyword".to_string(),
            ));
        }
        trim_commas(&rest[card_word_idx + 1..])
    };
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };
    let mut effect = EffectAst::Draw {
        count: count.clone(),
        player,
    };

    if !tail.is_empty() {
        let tail_words = words(&tail);
        if !((parsed_that_many_minus_one && tail_words.as_slice() == ["minus", "one"])
            || (parsed_that_many_plus_one && tail_words.as_slice() == ["plus", "one"]))
        {
            let has_for_each = tail
                .windows(2)
                .any(|window| window[0].is_word("for") && window[1].is_word("each"));
            if has_for_each {
                let dynamic = parse_dynamic_cost_modifier_value(&tail)?.ok_or_else(|| {
                    CardTextError::ParseError(format!(
                        "unsupported draw for-each clause (clause: '{}')",
                        words(tokens).join(" ")
                    ))
                })?;
                match count {
                    Value::Fixed(1) => count = dynamic,
                    _ => {
                        return Err(CardTextError::ParseError(format!(
                            "unsupported multiplied draw count (clause: '{}')",
                            words(tokens).join(" ")
                        )));
                    }
                }
                effect = EffectAst::Draw {
                    count: count.clone(),
                    player,
                };
            } else if let Some(parsed) = parse_draw_trailing_clause(&tail, effect.clone())? {
                effect = parsed;
            } else {
                return Err(CardTextError::ParseError(format!(
                    "unsupported trailing draw clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
        }
    }
    Ok(effect)
}

fn parse_draw_trailing_clause(
    tokens: &[Token],
    draw_effect: EffectAst,
) -> Result<Option<EffectAst>, CardTextError> {
    let tail_words = words(tokens);
    if tail_words.as_slice() == ["instead"] {
        return Ok(Some(draw_effect));
    }

    if let Some(timing) = parse_draw_delayed_timing_words(&tail_words) {
        return Ok(Some(wrap_return_with_delayed_timing(
            draw_effect,
            Some(timing),
        )));
    }

    if tail_words.first().copied() == Some("if") {
        let predicate_tokens = trim_commas(&tokens[1..]);
        if predicate_tokens.is_empty() {
            return Err(CardTextError::ParseError(
                "missing condition after trailing if clause".to_string(),
            ));
        }
        let predicate = parse_predicate(&predicate_tokens)?;
        return Ok(Some(EffectAst::Conditional {
            predicate,
            if_true: vec![draw_effect],
            if_false: Vec::new(),
        }));
    }

    if tail_words.first().copied() == Some("unless") {
        return try_build_unless(vec![draw_effect], tokens, 0);
    }

    Ok(None)
}

fn parse_draw_delayed_timing_words(words: &[&str]) -> Option<DelayedReturnTimingAst> {
    if let Some(timing) = parse_delayed_return_timing_words(words) {
        return Some(timing);
    }

    if matches!(
        words,
        ["at", "beginning", "of", "next", "turns", "upkeep"]
            | ["at", "beginning", "of", "the", "next", "turns", "upkeep"]
            | ["at", "the", "beginning", "of", "next", "turns", "upkeep"]
            | ["at", "the", "beginning", "of", "the", "next", "turns", "upkeep"]
    ) {
        return Some(DelayedReturnTimingAst::NextUpkeep(PlayerAst::Any));
    }

    None
}

fn parse_draw_as_many_cards_value(tokens: &[Token]) -> Option<Value> {
    let clause_words = words(tokens);
    let starts_as_many = clause_words.len() >= 4
        && clause_words[0] == "as"
        && clause_words[1] == "many"
        && matches!(clause_words[2], "card" | "cards")
        && clause_words[3] == "as";
    if !starts_as_many {
        return None;
    }

    let tail = &clause_words[4..];
    let references_previous_event = tail.windows(2).any(|window| window == ["this", "way"]);
    if references_previous_event {
        return Some(Value::EventValue(EventValueSpec::Amount));
    }

    None
}

fn parse_draw_card_prefixed_count_value(tokens: &[Token]) -> Result<Option<Value>, CardTextError> {
    if tokens.is_empty() {
        return Ok(None);
    }

    if let Some(value) = parse_draw_equal_to_value(tokens)? {
        return Ok(Some(value));
    }
    if let Some(value) = parse_dynamic_cost_modifier_value(tokens)? {
        return Ok(Some(value));
    }

    Ok(None)
}

fn parse_draw_equal_to_value(tokens: &[Token]) -> Result<Option<Value>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["equal", "to"]) {
        return Ok(None);
    }

    if let Some(value) = parse_devotion_value_from_add_clause(tokens)? {
        return Ok(Some(value));
    }
    if let Some(value) = parse_add_mana_equal_amount_value(tokens)
        .or_else(|| parse_equal_to_number_of_opponents_you_have_value(tokens))
        .or_else(|| parse_equal_to_number_of_counters_on_reference_value(tokens))
        .or_else(|| parse_equal_to_aggregate_filter_value(tokens))
        .or_else(|| parse_equal_to_number_of_filter_plus_or_minus_fixed_value(tokens))
        .or_else(|| parse_equal_to_number_of_filter_value(tokens))
    {
        return Ok(Some(value));
    }
    if clause_words
        .windows(2)
        .any(|window| window == ["this", "way"])
    {
        return Ok(Some(Value::EventValue(EventValueSpec::Amount)));
    }
    if let Some(value) = parse_dynamic_cost_modifier_value(tokens)? {
        return Ok(Some(value));
    }

    Ok(None)
}

fn parse_counter(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.contains(&"ability")
        && (clause_words.contains(&"activated") || clause_words.contains(&"triggered"))
    {
        if clause_words == ["target", "activated", "or", "triggered", "ability"] {
            return Ok(EffectAst::CounterActivatedOrTriggeredAbility);
        }
        if matches!(
            clause_words.as_slice(),
            [
                "target",
                "spell",
                "activated",
                "ability",
                "or",
                "triggered",
                "ability"
            ] | [
                "target",
                "spell",
                "or",
                "activated",
                "ability",
                "or",
                "triggered",
                "ability"
            ]
        ) {
            return Ok(EffectAst::Counter {
                target: TargetAst::Object(
                    ObjectFilter::spell_or_ability(),
                    Some(TextSpan::synthetic()),
                    None,
                ),
            });
        }
        return Err(CardTextError::ParseError(format!(
            "unsupported counter-ability target clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    if let Some(if_idx) = tokens.iter().position(|token| token.is_word("if")) {
        if if_idx == 0 || if_idx + 1 >= tokens.len() {
            return Err(CardTextError::ParseError(format!(
                "missing conditional counter target or predicate (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        let target_tokens = trim_commas(&tokens[..if_idx]);
        let predicate_tokens = trim_commas(&tokens[if_idx + 1..]);
        let target = parse_target_phrase(&target_tokens)?;
        let predicate = parse_predicate(&predicate_tokens)?;
        return Ok(EffectAst::Conditional {
            predicate,
            if_true: vec![EffectAst::Counter { target }],
            if_false: Vec::new(),
        });
    }

    if let Some(unless_idx) = tokens.iter().position(|token| token.is_word("unless")) {
        let target_tokens = &tokens[..unless_idx];
        let target = parse_target_phrase(target_tokens)?;

        let unless_tokens = &tokens[unless_idx + 1..];
        let pays_idx = unless_tokens
            .iter()
            .position(|token| token.is_word("pays"))
            .ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "missing pays keyword (clause: '{}')",
                    words(tokens).join(" ")
                ))
            })?;

        // Parse the contiguous mana payment immediately following "pays".
        // Stop at the first non-mana word so trailing dynamic qualifiers
        // ("for each ...", "where X is ...", "plus an additional ...") do not
        // accidentally duplicate symbols.
        let mut mana = Vec::new();
        let mut trailing_start: Option<usize> = None;
        for (offset, token) in unless_tokens[pays_idx + 1..].iter().enumerate() {
            let Some(word) = token.as_word() else {
                continue;
            };
            match parse_mana_symbol(word) {
                Ok(symbol) => mana.push(symbol),
                Err(_) => {
                    trailing_start = Some(pays_idx + 1 + offset);
                    break;
                }
            }
        }

        if mana.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing mana cost (clause: '{}')",
                words(tokens).join(" ")
            )));
        }

        let mut life = None;
        let mut additional_generic = None;
        if let Some(trailing_idx) = trailing_start {
            let trailing_tokens = trim_commas(&unless_tokens[trailing_idx..]);
            let trailing_words = words(&trailing_tokens);
            if trailing_tokens
                .first()
                .is_some_and(|token| token.is_word("and"))
            {
                let life_tokens = trim_commas(&trailing_tokens[1..]);
                if let Some((amount, used)) = parse_value(&life_tokens)
                    && life_tokens
                        .get(used)
                        .is_some_and(|token| token.is_word("life"))
                    && trim_commas(&life_tokens[used + 1..]).is_empty()
                {
                    life = Some(amount);
                } else {
                    parser_trace("parse_counter:ignored-trailing-unless-payment", tokens);
                }
            } else if let Some(value) =
                parse_counter_unless_additional_generic_value(&trailing_tokens)?
            {
                additional_generic = Some(value);
            } else if trailing_words.starts_with(&["for", "each"]) {
                if let Some(dynamic) = parse_dynamic_cost_modifier_value(&trailing_tokens)? {
                    if let [ManaSymbol::Generic(multiplier)] = mana.as_slice() {
                        additional_generic =
                            Some(scale_value_multiplier(dynamic, *multiplier as i32));
                        mana.clear();
                    } else {
                        parser_trace("parse_counter:ignored-trailing-unless-payment", tokens);
                    }
                } else {
                    parser_trace("parse_counter:ignored-trailing-unless-payment", tokens);
                }
            } else if !trailing_words.is_empty() {
                parser_trace("parse_counter:ignored-trailing-unless-payment", tokens);
            }
        }

        if mana.is_empty() && life.is_none() && additional_generic.is_none() {
            return Err(CardTextError::ParseError(format!(
                "missing mana cost (clause: '{}')",
                words(tokens).join(" ")
            )));
        }

        return Ok(EffectAst::CounterUnlessPays {
            target,
            mana,
            life,
            additional_generic,
        });
    }

    let target = parse_target_phrase(tokens)?;
    Ok(EffectAst::Counter { target })
}

fn scale_value_multiplier(value: Value, multiplier: i32) -> Value {
    if multiplier <= 0 {
        return Value::Fixed(0);
    }
    if multiplier == 1 {
        return value;
    }
    match value {
        Value::Fixed(amount) => Value::Fixed(amount * multiplier),
        Value::Count(filter) => Value::CountScaled(filter, multiplier),
        Value::CountScaled(filter, factor) => Value::CountScaled(filter, factor * multiplier),
        other => {
            let mut result = Value::Fixed(0);
            for _ in 0..multiplier {
                result = match result {
                    Value::Fixed(0) => other.clone(),
                    _ => Value::Add(Box::new(result), Box::new(other.clone())),
                };
            }
            result
        }
    }
}

fn parse_counter_unless_additional_generic_value(
    tokens: &[Token],
) -> Result<Option<Value>, CardTextError> {
    if tokens.is_empty() || !tokens[0].is_word("plus") {
        return Ok(None);
    }

    let mut idx = 1usize;
    if tokens.get(idx).is_some_and(|token| token.is_word("an")) {
        idx += 1;
    }
    if !tokens
        .get(idx)
        .is_some_and(|token| token.is_word("additional"))
    {
        return Ok(None);
    }
    idx += 1;

    let symbol_word = tokens
        .get(idx)
        .and_then(Token::as_word)
        .ok_or_else(|| CardTextError::ParseError("missing additional mana symbol".to_string()))?;
    let symbol = parse_mana_symbol(symbol_word).map_err(|_| {
        CardTextError::ParseError(format!(
            "unsupported additional payment symbol '{}' in counter clause",
            symbol_word
        ))
    })?;
    let multiplier = match symbol {
        ManaSymbol::Generic(amount) => amount as i32,
        _ => {
            return Err(CardTextError::ParseError(
                "unsupported nongeneric additional counter payment".to_string(),
            ));
        }
    };

    let filter_tokens = trim_commas(&tokens[idx + 1..]);
    let filter_words = words(&filter_tokens);
    if !filter_words.starts_with(&["for", "each"]) {
        return Err(CardTextError::ParseError(format!(
            "unsupported additional counter payment tail (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let dynamic = parse_dynamic_cost_modifier_value(&filter_tokens)?.ok_or_else(|| {
        CardTextError::ParseError(format!(
            "unsupported additional counter payment filter (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;
    Ok(Some(scale_value_multiplier(dynamic, multiplier)))
}

fn parse_reveal(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    let words = words(tokens);
    // Many effects split "reveal it/that card/those cards" into a standalone clause.
    // The engine does not model hidden information, so this compiles to a semantic no-op
    // that still allows parsing and auditing to proceed.
    if matches!(
        words.as_slice(),
        ["it"]
            | ["them"]
            | ["that"]
            | ["that", "card"]
            | ["those", "cards"]
            | ["those"]
            | ["this", "card"]
            | ["this"]
    ) {
        return Ok(EffectAst::RevealTagged {
            tag: TagKey::from(IT_TAG),
        });
    }
    let reveals_from_among = words.contains(&"from")
        && words.contains(&"among")
        && (words.contains(&"them") || words.contains(&"those"));
    if reveals_from_among {
        return Ok(EffectAst::RevealTagged {
            tag: TagKey::from(IT_TAG),
        });
    }
    let reveals_outside_game = words.contains(&"outside") && words.contains(&"game");
    if reveals_outside_game {
        return Ok(EffectAst::RevealTagged {
            tag: TagKey::from(IT_TAG),
        });
    }
    let reveals_first_draw = words.starts_with(&["the", "first", "card", "you", "draw"]);
    if reveals_first_draw {
        return Ok(EffectAst::RevealTagged {
            tag: TagKey::from(IT_TAG),
        });
    }
    let reveals_card_this_way = (words.contains(&"card") || words.contains(&"cards"))
        && words.ends_with(&["this", "way"]);
    if reveals_card_this_way {
        return Ok(EffectAst::RevealTagged {
            tag: TagKey::from(IT_TAG),
        });
    }
    let reveals_conditional_it = words.first() == Some(&"it") && words.contains(&"if");
    if reveals_conditional_it {
        return Ok(EffectAst::RevealTagged {
            tag: TagKey::from(IT_TAG),
        });
    }
    if words.contains(&"hand") {
        let is_full_hand_reveal = matches!(words.as_slice(), ["your", "hand"] | ["their", "hand"])
            || words.as_slice() == ["his", "or", "her", "hand"];
        if !is_full_hand_reveal {
            if words.contains(&"from") {
                return Ok(EffectAst::RevealTagged {
                    tag: TagKey::from(IT_TAG),
                });
            }
            return Err(CardTextError::ParseError(format!(
                "unsupported reveal-hand clause (clause: '{}')",
                words.join(" ")
            )));
        }
        return Ok(EffectAst::RevealHand { player });
    }

    let has_card = words.contains(&"card") || words.contains(&"cards");
    let has_library = words.contains(&"library") || words.contains(&"libraries");
    let explicit_top_card = words.as_slice() == ["top", "card"]
        || words.as_slice() == ["the", "top", "card"];

    if !has_card || (!has_library && !explicit_top_card) {
        return Err(CardTextError::ParseError(format!(
            "unsupported reveal clause (clause: '{}')",
            words.join(" ")
        )));
    }

    Ok(EffectAst::RevealTop { player })
}

fn parse_life_amount(tokens: &[Token], amount_kind: &str) -> Result<(Value, usize), CardTextError> {
    let clause_words = words(tokens);
    if clause_words == ["that", "much", "life"] {
        // "that much life" binds to the triggering event amount.
        return Ok((Value::EventValue(EventValueSpec::Amount), 2));
    }

    parse_value(tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing {amount_kind} amount (clause: '{}')",
            clause_words.join(" ")
        ))
    })
}

fn parse_life_equal_to_value(tokens: &[Token]) -> Result<Option<Value>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["life", "equal", "to"]) {
        return Ok(None);
    }

    let amount_tokens = &tokens[1..];
    let amount_words = words(amount_tokens);

    if let Some(value) = parse_add_mana_equal_amount_value(amount_tokens) {
        return Ok(Some(value));
    }
    if let Some(value) = parse_devotion_value_from_add_clause(amount_tokens)? {
        return Ok(Some(value));
    }
    if let Some(value) = parse_equal_to_number_of_filter_value(amount_tokens) {
        return Ok(Some(value));
    }
    if matches!(
        amount_words.as_slice(),
        ["equal", "to", "the", "life", "lost", "this", "way"]
            | ["equal", "to", "life", "lost", "this", "way"]
            | ["equal", "to", "the", "amount", "of", "life", "lost", "this", "way"]
            | ["equal", "to", "amount", "of", "life", "lost", "this", "way"]
    ) {
        return Ok(Some(Value::EventValue(EventValueSpec::LifeAmount)));
    }
    if let Some(value) = parse_dynamic_cost_modifier_value(amount_tokens)? {
        return Ok(Some(value));
    }

    Err(CardTextError::ParseError(format!(
        "missing life amount in equal-to clause (clause: '{}')",
        clause_words.join(" ")
    )))
}

fn parse_life_amount_from_trailing(
    base_amount: &Value,
    trailing: &[Token],
) -> Result<Option<Value>, CardTextError> {
    if trailing.is_empty() {
        return Ok(None);
    }

    if let Some(dynamic) = parse_dynamic_cost_modifier_value(trailing)? {
        if let Some(multiplier) = match base_amount {
            Value::Fixed(value) => Some(*value),
            Value::X => Some(1),
            _ => None,
        } {
            return Ok(Some(scale_value_multiplier(dynamic, multiplier)));
        }
    }

    if let Some(where_value) = parse_where_x_value_clause(trailing) {
        if value_contains_unbound_x(base_amount) {
            let clause = words(trailing).join(" ");
            return Ok(Some(replace_unbound_x_with_value(
                base_amount.clone(),
                &where_value,
                &clause,
            )?));
        }
        if matches!(base_amount, Value::Fixed(1)) {
            return Ok(Some(where_value));
        }
    }

    Ok(None)
}

fn validate_life_keyword(rest: &[Token]) -> Result<(), CardTextError> {
    if rest
        .first()
        .and_then(Token::as_word)
        .is_some_and(|word| word != "life")
    {
        return Err(CardTextError::ParseError(
            "missing life keyword".to_string(),
        ));
    }
    Ok(())
}

fn remap_source_stat_value_to_it(value: Value) -> Value {
    match value {
        Value::PowerOf(spec) if matches!(spec.as_ref(), ChooseSpec::Source) => {
            Value::PowerOf(Box::new(ChooseSpec::Tagged(TagKey::from(IT_TAG))))
        }
        Value::ToughnessOf(spec) if matches!(spec.as_ref(), ChooseSpec::Source) => {
            Value::ToughnessOf(Box::new(ChooseSpec::Tagged(TagKey::from(IT_TAG))))
        }
        Value::ManaValueOf(spec) if matches!(spec.as_ref(), ChooseSpec::Source) => {
            Value::ManaValueOf(Box::new(ChooseSpec::Tagged(TagKey::from(IT_TAG))))
        }
        Value::Add(left, right) => Value::Add(
            Box::new(remap_source_stat_value_to_it(*left)),
            Box::new(remap_source_stat_value_to_it(*right)),
        ),
        other => other,
    }
}

fn parse_lose_life(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    let clause_words = words(tokens);
    if let Some(mut amount) = parse_life_equal_to_value(tokens)? {
        if matches!(player, PlayerAst::ItsController | PlayerAst::ItsOwner)
            && (clause_words
                .windows(2)
                .any(|window| window == ["its", "power"])
                || clause_words
                    .windows(2)
                    .any(|window| window == ["its", "toughness"])
                || clause_words
                    .windows(3)
                    .any(|window| window == ["its", "mana", "value"]))
        {
            amount = remap_source_stat_value_to_it(amount);
        }
        return Ok(EffectAst::LoseLife { amount, player });
    }
    if clause_words.as_slice() == ["the", "game"] {
        return Ok(EffectAst::LoseGame { player });
    }

    let (mut amount, used) = parse_life_amount(tokens, "life loss")?;

    let rest = &tokens[used..];
    validate_life_keyword(rest)?;
    let trailing = trim_commas(&rest[1..]);
    if !trailing.is_empty() {
        if let Some(resolved) = parse_life_amount_from_trailing(&amount, &trailing)? {
            amount = resolved;
            return Ok(EffectAst::LoseLife { amount, player });
        }
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing life-loss clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(EffectAst::LoseLife { amount, player })
}

fn parse_gain_life(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    if let Some(amount) = parse_life_equal_to_value(tokens)? {
        return Ok(EffectAst::GainLife { amount, player });
    }

    let (mut amount, used) = parse_life_amount(tokens, "life gain")?;

    let rest = &tokens[used..];
    validate_life_keyword(rest)?;
    let trailing = trim_commas(&rest[1..]);
    if !trailing.is_empty() {
        let trailing_words = words(&trailing);
        if trailing_words
            .windows(6)
            .any(|window| window == ["then", "shuffle", "your", "graveyard", "into", "your"])
            && trailing_words.contains(&"library")
        {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing life-gain shuffle-graveyard clause (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        if let Some(resolved) = parse_life_amount_from_trailing(&amount, &trailing)? {
            amount = resolved;
            return Ok(EffectAst::GainLife { amount, player });
        }
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing life-gain clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    Ok(EffectAst::GainLife { amount, player })
}

fn parse_gain_control(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    let has_dynamic_power_bound = clause_words.contains(&"power")
        && clause_words.contains(&"number")
        && clause_words
            .windows(2)
            .any(|window| window == ["you", "control"]);
    if has_dynamic_power_bound {
        return Err(CardTextError::ParseError(format!(
            "unsupported dynamic power-bound control clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let mut idx = 0;
    if tokens
        .get(idx)
        .is_some_and(|token| token.is_word("control"))
    {
        idx += 1;
    } else {
        return Err(CardTextError::ParseError(
            "missing control keyword".to_string(),
        ));
    }

    if tokens.get(idx).is_some_and(|token| token.is_word("of")) {
        idx += 1;
    }

    let duration_idx = tokens[idx..]
        .iter()
        .position(|token| token.is_word("during") || token.is_word("until"))
        .map(|offset| idx + offset)
        .or_else(|| {
            tokens[idx..]
                .windows(4)
                .position(|window| {
                    window[0].is_word("for")
                        && window[1].is_word("as")
                        && window[2].is_word("long")
                        && window[3].is_word("as")
                })
                .map(|offset| idx + offset)
        });

    let target_tokens = if let Some(dur_idx) = duration_idx {
        &tokens[idx..dur_idx]
    } else {
        &tokens[idx..]
    };
    if target_tokens
        .iter()
        .any(|token| token.is_word("if") || token.is_word("unless"))
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported conditional gain-control clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let target_ast = parse_target_phrase(target_tokens)?;
    let duration_tokens = duration_idx
        .map(|dur_idx| &tokens[dur_idx..])
        .unwrap_or(&[]);
    let duration = parse_control_duration(duration_tokens)?;
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };
    match target_ast {
        TargetAst::Player(filter, _) => Ok(EffectAst::ControlPlayer {
            player: PlayerFilter::Target(Box::new(filter)),
            duration,
        }),
        _ => {
            let until = match duration {
                ControlDurationAst::UntilEndOfTurn => Until::EndOfTurn,
                ControlDurationAst::Forever => Until::Forever,
                ControlDurationAst::AsLongAsYouControlSource => Until::YouStopControllingThis,
                ControlDurationAst::DuringNextTurn => {
                    return Err(CardTextError::ParseError(
                        "unsupported control duration for permanents".to_string(),
                    ));
                }
            };
            Ok(EffectAst::GainControl {
                target: target_ast,
                player,
                duration: until,
            })
        }
    }
}

fn parse_control_duration(tokens: &[Token]) -> Result<ControlDurationAst, CardTextError> {
    if tokens.is_empty() {
        return Ok(ControlDurationAst::Forever);
    }

    let words = words(tokens);
    let has_for_as_long_as = words
        .windows(4)
        .any(|window| window == ["for", "as", "long", "as"]);
    if has_for_as_long_as
        && words.contains(&"you")
        && words.contains(&"control")
        && (words.contains(&"this")
            || words.contains(&"thiss")
            || words.contains(&"source")
            || words.contains(&"creature")
            || words.contains(&"permanent"))
    {
        return Ok(ControlDurationAst::AsLongAsYouControlSource);
    }

    let has_during = words.contains(&"during");
    let has_next = words.contains(&"next");
    let has_turn = words.contains(&"turn");
    if has_during && has_next && has_turn {
        return Ok(ControlDurationAst::DuringNextTurn);
    }

    let has_until = words.contains(&"until");
    let has_end = words.contains(&"end");
    if has_until && has_end && has_turn {
        return Ok(ControlDurationAst::UntilEndOfTurn);
    }

    Err(CardTextError::ParseError(
        "unsupported control duration".to_string(),
    ))
}

fn parse_put_into_hand(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    let clause_words = words(tokens);

    // "Put them/it back in any order." (typically after looking at the top cards of a library).
    if clause_words.contains(&"back")
        && clause_words.contains(&"any")
        && clause_words.contains(&"order")
        && matches!(clause_words.first().copied(), Some("it" | "them"))
    {
        return Ok(EffectAst::ReorderTopOfLibrary {
            tag: TagKey::from(IT_TAG),
        });
    }

    if clause_words.contains(&"from") && clause_words.contains(&"among") {
        return Err(CardTextError::ParseError(format!(
            "unsupported put-from-among clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let has_it = clause_words.contains(&"it");
    let has_them = clause_words.contains(&"them");
    let has_hand = clause_words.contains(&"hand");
    let has_into = clause_words.contains(&"into");

    if has_hand && has_into && (has_it || has_them) {
        // "Put N of them into your hand and the rest on the bottom of your library in any order."
        if has_them
            && clause_words.contains(&"rest")
            && clause_words.contains(&"bottom")
            && clause_words.contains(&"library")
            && clause_words.iter().any(|w| *w == "and" || *w == "then")
        {
            let (count, used) = parse_number(tokens).ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "missing put count (clause: '{}')",
                    clause_words.join(" ")
                ))
            })?;
            let mut idx = used;
            if tokens.get(idx).is_some_and(|t| t.is_word("of")) {
                idx += 1;
            }
            if !tokens.get(idx).is_some_and(|t| t.is_word("them")) {
                return Err(CardTextError::ParseError(format!(
                    "unsupported multi-destination put clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }

            let dest_player = if clause_words.contains(&"your") {
                PlayerAst::You
            } else if clause_words.contains(&"their")
                || clause_words.starts_with(&["that", "player"])
                || clause_words.starts_with(&["that", "players"])
            {
                PlayerAst::That
            } else {
                player
            };

            return Ok(EffectAst::PutSomeIntoHandRestOnBottomOfLibrary {
                player: dest_player,
                count: count as u32,
            });
        }

        // "Put N of them into your hand and the rest into your graveyard."
        if has_them
            && clause_words.contains(&"rest")
            && clause_words.contains(&"graveyard")
            && clause_words.iter().any(|w| *w == "and" || *w == "then")
        {
            let (count, used) = parse_number(tokens).ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "missing put count (clause: '{}')",
                    clause_words.join(" ")
                ))
            })?;
            // Accept optional "of" before "them".
            let mut idx = used;
            if tokens.get(idx).is_some_and(|t| t.is_word("of")) {
                idx += 1;
            }
            if !tokens.get(idx).is_some_and(|t| t.is_word("them")) {
                return Err(CardTextError::ParseError(format!(
                    "unsupported multi-destination put clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }

            // The chooser is typically the player whose hand is referenced.
            let dest_player = if clause_words.contains(&"your") {
                PlayerAst::You
            } else if clause_words.contains(&"their")
                || clause_words.starts_with(&["that", "player"])
                || clause_words.starts_with(&["that", "players"])
            {
                PlayerAst::That
            } else {
                player
            };

            return Ok(EffectAst::PutSomeIntoHandRestIntoGraveyard {
                player: dest_player,
                count: count as u32,
            });
        }

        return Ok(EffectAst::PutIntoHand {
            player,
            object: ObjectRefAst::It,
        });
    }

    if let Some(on_idx) = tokens.iter().position(|token| token.is_word("on"))
        && tokens
            .get(on_idx + 1)
            .is_some_and(|token| token.is_word("top"))
        && tokens
            .get(on_idx + 2)
            .is_some_and(|token| token.is_word("of"))
    {
        let target_tokens = trim_commas(&tokens[..on_idx]);
        if target_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing target before 'on top of' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let destination_words = words(&tokens[on_idx + 3..]);
        if !destination_words.contains(&"library") {
            return Err(CardTextError::ParseError(format!(
                "unsupported put destination after 'on top of' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let target = if let Some((count, used)) = parse_number(&target_tokens)
            && target_tokens
                .get(used)
                .is_some_and(|token| token.is_word("card") || token.is_word("cards"))
        {
            let inner = parse_target_phrase(&target_tokens[used..])?;
            TargetAst::WithCount(Box::new(inner), ChoiceCount::exactly(count as usize))
        } else {
            parse_target_phrase(&target_tokens)?
        };
        return Ok(EffectAst::MoveToZone {
            target,
            zone: Zone::Library,
            to_top: true,
            battlefield_controller: ReturnControllerAst::Preserve,
        });
    }

    if let Some(on_idx) = tokens.iter().position(|token| token.is_word("on")) {
        let mut bottom_idx = on_idx + 1;
        if tokens
            .get(bottom_idx)
            .is_some_and(|token| token.is_word("the"))
        {
            bottom_idx += 1;
        }
        if tokens
            .get(bottom_idx)
            .is_some_and(|token| token.is_word("bottom"))
            && tokens
                .get(bottom_idx + 1)
                .is_some_and(|token| token.is_word("of"))
        {
            let target_tokens = trim_commas(&tokens[..on_idx]);
            if target_tokens.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "missing target before 'on bottom of' (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            let destination_words = words(&tokens[bottom_idx + 2..]);
            if !destination_words.contains(&"library") {
                return Err(CardTextError::ParseError(format!(
                    "unsupported put destination after 'on bottom of' (clause: '{}')",
                    clause_words.join(" ")
                )));
            }

            let target_words = words(&target_tokens);
            let is_rest_target =
                target_words.as_slice() == ["the", "rest"] || target_words.as_slice() == ["rest"];
            if is_rest_target {
                return Ok(EffectAst::PutRestOnBottomOfLibrary);
            }

            let target = if let Some((count, used)) = parse_number(&target_tokens)
                && target_tokens
                    .get(used)
                    .is_some_and(|token| token.is_word("card") || token.is_word("cards"))
            {
                let inner = parse_target_phrase(&target_tokens[used..])?;
                TargetAst::WithCount(Box::new(inner), ChoiceCount::exactly(count as usize))
            } else {
                parse_target_phrase(&target_tokens)?
            };

            return Ok(EffectAst::MoveToZone {
                target,
                zone: Zone::Library,
                to_top: false,
                battlefield_controller: ReturnControllerAst::Preserve,
            });
        }
    }

    if let Some(onto_idx) = tokens.iter().position(|token| token.is_word("onto")) {
        let target_tokens = trim_commas(&tokens[..onto_idx]);
        if target_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing target before 'onto' (clause: '{}')",
                clause_words.join(" ")
            )));
        }

        let destination_words: Vec<&str> = words(&tokens[onto_idx + 1..])
            .into_iter()
            .filter(|word| !is_article(word))
            .collect();
        if destination_words.first() != Some(&"battlefield") {
            return Err(CardTextError::ParseError(format!(
                "unsupported put destination after 'onto' (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let destination_tail = &destination_words[1..];
        let supported_control_tail = destination_tail.is_empty()
            || destination_tail == ["under", "your", "control"]
            || destination_tail == ["under", "its", "owners", "control"]
            || destination_tail == ["under", "their", "owners", "control"]
            || destination_tail == ["under", "that", "players", "control"];
        if !supported_control_tail {
            return Err(CardTextError::ParseError(format!(
                "unsupported put destination after 'onto' (clause: '{}')",
                clause_words.join(" ")
            )));
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

        if target_tokens
            .first()
            .is_some_and(|token| token.is_word("all") || token.is_word("each"))
        {
            let mut filter = parse_object_filter(&target_tokens[1..], false)?;
            let target_words = words(&target_tokens[1..]);
            if target_words
                .windows(2)
                .any(|window| window == ["from", "it"])
            {
                filter.zone = Some(Zone::Hand);
                if filter.owner.is_none() {
                    filter.owner = Some(PlayerFilter::You);
                }
                filter
                    .tagged_constraints
                    .retain(|constraint| constraint.tag.as_str() != IT_TAG);
            }
            if clause_words.contains(&"among") && clause_words.contains(&"them") {
                filter.zone = Some(Zone::Exile);
                if filter.owner.is_none() {
                    filter.owner = Some(PlayerFilter::IteratedPlayer);
                }
                if clause_words.contains(&"permanent") {
                    filter.card_types = vec![
                        CardType::Artifact,
                        CardType::Creature,
                        CardType::Enchantment,
                        CardType::Land,
                        CardType::Planeswalker,
                        CardType::Battle,
                    ];
                }
            }
            return Ok(EffectAst::ReturnAllToBattlefield {
                filter,
                tapped: false,
            });
        }

        return Ok(EffectAst::MoveToZone {
            target: parse_target_phrase(&target_tokens)?,
            zone: Zone::Battlefield,
            to_top: false,
            battlefield_controller,
        });
    }

    Err(CardTextError::ParseError(format!(
        "unsupported put clause (clause: '{}')",
        clause_words.join(" ")
    )))
}

fn parse_counter_type_word(word: &str) -> Option<CounterType> {
    match word {
        "+1/+1" => Some(CounterType::PlusOnePlusOne),
        "-1/-1" => Some(CounterType::MinusOneMinusOne),
        "-0/-1" => Some(CounterType::MinusOneMinusOne),
        "+1/+0" => Some(CounterType::PlusOnePlusZero),
        "+0/+1" => Some(CounterType::PlusZeroPlusOne),
        "+1/+2" => Some(CounterType::PlusOnePlusTwo),
        "+2/+2" => Some(CounterType::PlusTwoPlusTwo),
        "-0/-2" => Some(CounterType::MinusZeroMinusTwo),
        "-2/-2" => Some(CounterType::MinusTwoMinusTwo),
        "deathtouch" => Some(CounterType::Deathtouch),
        "flying" => Some(CounterType::Flying),
        "haste" => Some(CounterType::Haste),
        "hexproof" => Some(CounterType::Hexproof),
        "indestructible" => Some(CounterType::Indestructible),
        "lifelink" => Some(CounterType::Lifelink),
        "menace" => Some(CounterType::Menace),
        "reach" => Some(CounterType::Reach),
        "trample" => Some(CounterType::Trample),
        "vigilance" => Some(CounterType::Vigilance),
        "loyalty" => Some(CounterType::Loyalty),
        "charge" => Some(CounterType::Charge),
        "stun" => Some(CounterType::Stun),
        "depletion" => Some(CounterType::Depletion),
        "storage" => Some(CounterType::Storage),
        "ki" => Some(CounterType::Ki),
        "energy" => Some(CounterType::Energy),
        "age" => Some(CounterType::Age),
        "finality" => Some(CounterType::Finality),
        "time" => Some(CounterType::Time),
        "brain" => Some(CounterType::Brain),
        "level" => Some(CounterType::Level),
        "lore" => Some(CounterType::Lore),
        "oil" => Some(CounterType::Oil),
        _ => None,
    }
}

fn intern_counter_name(word: &str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    // Card parsing happens across many cards; deduplicate to avoid leaking one allocation per card.
    static INTERNER: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();

    let map = INTERNER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().expect("counter name interner lock poisoned");
    if let Some(existing) = map.get(word) {
        return *existing;
    }

    let leaked: &'static str = Box::leak(word.to_string().into_boxed_str());
    map.insert(word.to_string(), leaked);
    leaked
}

fn parse_counter_type_from_tokens(tokens: &[Token]) -> Option<CounterType> {
    let token_words = words(tokens);

    if let Some(counter_idx) = token_words
        .iter()
        .position(|word| *word == "counter" || *word == "counters")
    {
        if counter_idx == 0 {
            return None;
        }

        let prev = token_words[counter_idx - 1];
        if let Some(counter_type) = parse_counter_type_word(prev) {
            return Some(counter_type);
        }

        if prev == "strike" && counter_idx >= 2 {
            match token_words[counter_idx - 2] {
                "double" => return Some(CounterType::DoubleStrike),
                "first" => return Some(CounterType::FirstStrike),
                _ => {}
            }
        }

        // "a counter of that kind" doesn't name the counter type.
        if matches!(
            prev,
            "a" | "an" | "one" | "two" | "three" | "four" | "five" | "six" | "another"
        ) {
            return None;
        }

        if prev.chars().all(|c| c.is_ascii_alphabetic()) {
            return Some(CounterType::Named(intern_counter_name(prev)));
        }
    }

    None
}

fn describe_counter_type_for_mode(counter_type: CounterType) -> String {
    match counter_type {
        CounterType::PlusOnePlusOne => "+1/+1".to_string(),
        CounterType::MinusOneMinusOne => "-1/-1".to_string(),
        CounterType::PlusOnePlusZero => "+1/+0".to_string(),
        CounterType::PlusZeroPlusOne => "+0/+1".to_string(),
        CounterType::PlusOnePlusTwo => "+1/+2".to_string(),
        CounterType::PlusTwoPlusTwo => "+2/+2".to_string(),
        CounterType::MinusZeroMinusTwo => "-0/-2".to_string(),
        CounterType::MinusTwoMinusTwo => "-2/-2".to_string(),
        CounterType::Named(name) => name.to_string(),
        other => format!("{other:?}").to_ascii_lowercase(),
    }
}

fn describe_counter_phrase_for_mode(count: u32, counter_type: CounterType) -> String {
    let counter_name = describe_counter_type_for_mode(counter_type);
    if count == 1 {
        format!("a {counter_name} counter")
    } else {
        format!("{count} {counter_name} counters")
    }
}

fn sentence_case_mode_text(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.push(first.to_ascii_uppercase());
    out.extend(chars);
    out
}

fn parse_counter_descriptor(tokens: &[Token]) -> Result<(u32, CounterType), CardTextError> {
    let descriptor = trim_commas(tokens);
    let (count, used) = parse_number(&descriptor).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing counter amount (clause: '{}')",
            words(&descriptor).join(" ")
        ))
    })?;
    let rest = &descriptor[used..];
    if !rest
        .iter()
        .any(|token| token.is_word("counter") || token.is_word("counters"))
    {
        return Err(CardTextError::ParseError(format!(
            "missing counter keyword (clause: '{}')",
            words(&descriptor).join(" ")
        )));
    }
    let counter_type = parse_counter_type_from_tokens(rest).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "unsupported counter type (clause: '{}')",
            words(&descriptor).join(" ")
        ))
    })?;
    Ok((count, counter_type))
}

fn parse_referential_counter_count_value(tokens: &[Token]) -> Option<(Value, usize)> {
    let words_all = words(tokens);
    if words_all.is_empty() {
        return None;
    }

    let (source_spec, mut idx): (ChooseSpec, usize) = if words_all.starts_with(&["its"])
        || words_all.starts_with(&["those"])
        || words_all.starts_with(&["thiss"])
    {
        (ChooseSpec::Tagged(TagKey::from(IT_TAG)), 1)
    } else if words_all.starts_with(&["this"]) {
        (ChooseSpec::Source, 1)
    } else {
        return None;
    };

    let Some(word) = words_all.get(idx).copied() else {
        return None;
    };

    let counter_type = if word == "counter" || word == "counters" {
        idx += 1;
        None
    } else if let Some(counter_type) = parse_counter_type_word(word) {
        if !matches!(words_all.get(idx + 1).copied(), Some("counter" | "counters")) {
            return None;
        }
        idx += 2;
        Some(counter_type)
    } else {
        return None;
    };

    Some((Value::CountersOn(Box::new(source_spec), counter_type), idx))
}

fn parse_put_counter_count_value(tokens: &[Token]) -> Result<(Value, usize), CardTextError> {
    let clause = words(tokens).join(" ");
    let words_all = words(tokens);

    if words_all.starts_with(&["that", "many"]) || words_all.starts_with(&["that", "much"]) {
        return Ok((Value::EventValue(EventValueSpec::Amount), 2));
    }
    if words_all.starts_with(&["another"]) {
        return Ok((Value::Fixed(1), 1));
    }
    if let Some((value, used)) = parse_referential_counter_count_value(tokens) {
        return Ok((value, used));
    }
    if words_all.starts_with(&["a", "number", "of"]) {
        if let Some(value) = parse_add_mana_equal_amount_value(tokens)
            .or_else(|| parse_equal_to_aggregate_filter_value(tokens))
            .or_else(|| parse_equal_to_number_of_filter_value(tokens))
        {
            return Ok((value, 3));
        }
        if let Some(value) = parse_devotion_value_from_add_clause(tokens)? {
            return Ok((value, 3));
        }
        if let Some(value) = parse_dynamic_cost_modifier_value(tokens)? {
            return Ok((value, 3));
        }
        return Err(CardTextError::ParseError(format!(
            "missing counter amount (clause: '{}')",
            clause
        )));
    }

    parse_value(tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing counter amount (clause: '{}')",
            clause
        ))
    })
}

fn target_from_counter_source_spec(spec: &ChooseSpec, span: Option<TextSpan>) -> Option<TargetAst> {
    match spec {
        ChooseSpec::Source => Some(TargetAst::Source(span)),
        ChooseSpec::Tagged(tag) => Some(TargetAst::Tagged(tag.clone(), span)),
        ChooseSpec::Target(inner) => target_from_counter_source_spec(inner, span),
        _ => None,
    }
}

fn parse_put_counters(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let (count_value, used) = parse_put_counter_count_value(tokens)?;

    let rest = &tokens[used..];
    let on_idx = rest
        .iter()
        .position(|token| token.is_word("on"))
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing counter target (clause: '{}')",
                words(tokens).join(" ")
            ))
        })?;

    let mut target_tokens = rest[on_idx + 1..].to_vec();
    if let Some(equal_idx) = target_tokens.iter().position(|token| token.is_word("equal"))
        && target_tokens
            .get(equal_idx + 1)
            .is_some_and(|token| token.is_word("to"))
        && equal_idx > 0
    {
        target_tokens = trim_commas(&target_tokens[..equal_idx]);
    }
    let mut trailing_predicate: Option<PredicateAst> = None;
    if let Some(if_idx) = target_tokens.iter().position(|token| token.is_word("if")) {
        let predicate_tokens = trim_commas(&target_tokens[if_idx + 1..]);
        if !predicate_tokens.is_empty() {
            trailing_predicate = Some(parse_predicate(&predicate_tokens)?);
            target_tokens = trim_commas(&target_tokens[..if_idx]);
        }
    }
    while target_tokens
        .last()
        .is_some_and(|token| token.is_word("instead"))
    {
        target_tokens.pop();
    }

    let wrap_conditional = |effect: EffectAst| {
        if let Some(predicate) = trailing_predicate.clone() {
            EffectAst::Conditional {
                predicate,
                if_true: vec![effect],
                if_false: Vec::new(),
            }
        } else {
            effect
        }
    };

    let counter_type = if let Some(counter_type) = parse_counter_type_from_tokens(rest) {
        counter_type
    } else if let Value::CountersOn(_, Some(counter_type)) = &count_value {
        *counter_type
    } else if let Value::CountersOn(spec, None) = &count_value {
        let target = parse_target_phrase(&target_tokens)?;
        let from = target_from_counter_source_spec(spec.as_ref(), span_from_tokens(tokens))
            .ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "unsupported counter source reference (clause: '{}')",
                    words(tokens).join(" ")
                ))
            })?;
        return Ok(wrap_conditional(EffectAst::MoveAllCounters { from, to: target }));
    } else {
        return Err(CardTextError::ParseError(format!(
            "unsupported counter type (clause: '{}')",
            words(tokens).join(" ")
        )));
    };

    if let Value::Fixed(fixed_count) = count_value
        && fixed_count >= 0
        && let Some(mut effect) = parse_put_or_remove_counter_choice(
            fixed_count as u32,
            counter_type,
            &target_tokens,
            tokens,
        )?
    {
        let mut predicate = trailing_predicate.clone();
        if let Some(PredicateAst::ItMatches(filter)) = predicate.as_ref()
            && let EffectAst::PutOrRemoveCounters { target, .. } = &mut effect
            && merge_it_match_filter_into_target(target, filter)
        {
            predicate = None;
        }
        return Ok(if let Some(predicate) = predicate {
            EffectAst::Conditional {
                predicate,
                if_true: vec![effect],
                if_false: Vec::new(),
            }
        } else {
            effect
        });
    }

    if let Some((target_count, used)) = parse_counter_target_count_prefix(&target_tokens)? {
        let target_phrase = &target_tokens[used..];
        if target_phrase.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing counter target after count clause (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        let mut target = parse_target_phrase(target_phrase)?;
        let mut predicate = trailing_predicate.clone();
        if let Some(PredicateAst::ItMatches(filter)) = predicate.as_ref()
            && merge_it_match_filter_into_target(&mut target, filter)
        {
            predicate = None;
        }
        let effect = EffectAst::PutCounters {
            counter_type,
            count: count_value.clone(),
            target,
            target_count: Some(target_count),
            distributed: false,
        };
        return Ok(if let Some(predicate) = predicate {
            EffectAst::Conditional {
                predicate,
                if_true: vec![effect],
                if_false: Vec::new(),
            }
        } else {
            effect
        });
    }

    if target_tokens
        .first()
        .is_some_and(|token| token.is_word("each"))
    {
        let filter = parse_object_filter(&target_tokens[1..], false)?;
        return Ok(wrap_conditional(EffectAst::PutCountersAll {
            counter_type,
            count: count_value,
            filter,
        }));
    }
    if let Some(for_each_idx) = (0..target_tokens.len().saturating_sub(1))
        .find(|idx| target_tokens[*idx].is_word("for") && target_tokens[*idx + 1].is_word("each"))
    {
        let base_target_tokens = trim_commas(&target_tokens[..for_each_idx]);
        let count_filter_tokens = trim_commas(&target_tokens[for_each_idx + 2..]);
        if !base_target_tokens.is_empty() && !count_filter_tokens.is_empty() {
            let mut target = parse_target_phrase(&base_target_tokens)?;
            let mut predicate = trailing_predicate.clone();
            if let Some(PredicateAst::ItMatches(filter)) = predicate.as_ref()
                && merge_it_match_filter_into_target(&mut target, filter)
            {
                predicate = None;
            }
            let mut count =
                if let Some(dynamic) = parse_create_for_each_dynamic_count(&count_filter_tokens) {
                    dynamic
                } else {
                    Value::Count(parse_object_filter(&count_filter_tokens, false)?)
                };
            if let Value::Fixed(multiplier) = count_value.clone()
                && multiplier > 1
            {
                let base = count.clone();
                for _ in 1..multiplier {
                    count = Value::Add(Box::new(count), Box::new(base.clone()));
                }
            }
            let effect = EffectAst::PutCounters {
                counter_type,
                count,
                target,
                target_count: None,
                distributed: false,
            };
            return Ok(if let Some(predicate) = predicate {
                EffectAst::Conditional {
                    predicate,
                    if_true: vec![effect],
                    if_false: Vec::new(),
                }
            } else {
                effect
            });
        }
    }
    let mut target = parse_target_phrase(&target_tokens)?;
    let mut predicate = trailing_predicate.clone();
    if let Some(PredicateAst::ItMatches(filter)) = predicate.as_ref()
        && merge_it_match_filter_into_target(&mut target, filter)
    {
        predicate = None;
    }
    let effect = EffectAst::PutCounters {
        counter_type,
        count: count_value,
        target,
        target_count: None,
        distributed: false,
    };
    Ok(if let Some(predicate) = predicate {
        EffectAst::Conditional {
            predicate,
            if_true: vec![effect],
            if_false: Vec::new(),
        }
    } else {
        effect
    })
}

fn parse_sentence_put_multiple_counters_on_target(
    tokens: &[Token],
) -> Result<Option<Vec<EffectAst>>, CardTextError> {
    let clause_words = words(tokens);
    if !matches!(clause_words.first().copied(), Some("put") | Some("puts")) {
        return Ok(None);
    }

    let Some(on_idx) = tokens.iter().position(|token| token.is_word("on")) else {
        return Ok(None);
    };
    if on_idx < 2 {
        return Ok(None);
    }

    let before_on = trim_commas(&tokens[1..on_idx]);
    let Some(and_idx) = before_on.iter().position(|token| token.is_word("and")) else {
        return Ok(None);
    };
    if and_idx == 0 || and_idx + 1 >= before_on.len() {
        return Ok(None);
    }

    let first_desc = trim_commas(&before_on[..and_idx]);
    let second_desc = trim_commas(&before_on[and_idx + 1..]);
    if first_desc.is_empty() || second_desc.is_empty() {
        return Ok(None);
    }
    if first_desc
        .iter()
        .any(|token| matches!(token, Token::Comma(_)))
        || second_desc
            .iter()
            .any(|token| matches!(token, Token::Comma(_)))
    {
        return Ok(None);
    }
    let first_words = words(&first_desc);
    let second_words = words(&second_desc);
    if !first_words
        .iter()
        .any(|word| *word == "counter" || *word == "counters")
        || !second_words
            .iter()
            .any(|word| *word == "counter" || *word == "counters")
    {
        return Ok(None);
    }

    let (first_count, first_counter) = match parse_counter_descriptor(&first_desc) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    let (second_count, second_counter) = match parse_counter_descriptor(&second_desc) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };

    let target_tokens = trim_commas(&tokens[on_idx + 1..]);
    if target_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing counter target after on clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let target_words = words(&target_tokens);
    if !target_words
        .iter()
        .any(|word| *word == "target" || *word == "targets")
    {
        return Ok(None);
    }

    let target = parse_target_phrase(&target_tokens)?;
    if matches!(target, TargetAst::WithCount(_, _)) {
        return Ok(None);
    }

    let first_effect = EffectAst::PutCounters {
        counter_type: first_counter,
        count: Value::Fixed(first_count as i32),
        target: target.clone(),
        target_count: None,
        distributed: false,
    };
    let second_effect = EffectAst::PutCounters {
        counter_type: second_counter,
        count: Value::Fixed(second_count as i32),
        target: TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(tokens)),
        target_count: None,
        distributed: false,
    };

    Ok(Some(vec![first_effect, second_effect]))
}

fn parse_put_or_remove_counter_choice(
    put_count: u32,
    put_counter_type: CounterType,
    target_tokens: &[Token],
    clause_tokens: &[Token],
) -> Result<Option<EffectAst>, CardTextError> {
    let Some(or_idx) = target_tokens
        .windows(2)
        .position(|window| window[0].is_word("or") && window[1].is_word("remove"))
    else {
        return Ok(None);
    };

    let base_target_tokens = trim_commas(&target_tokens[..or_idx]);
    if base_target_tokens.is_empty() {
        return Ok(None);
    }

    let remove_tokens = trim_commas(&target_tokens[or_idx + 1..]);
    if remove_tokens.len() < 2 || !remove_tokens[0].is_word("remove") {
        return Ok(None);
    }

    let mut idx = 1usize;
    let (remove_count, used_remove_count) =
        parse_value(&remove_tokens[idx..]).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing counter removal amount in put-or-remove clause (clause: '{}')",
                words(clause_tokens).join(" ")
            ))
        })?;
    idx += used_remove_count;

    let from_idx = remove_tokens[idx..]
        .iter()
        .position(|token| token.is_word("from"))
        .map(|offset| idx + offset)
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing 'from' in put-or-remove clause (clause: '{}')",
                words(clause_tokens).join(" ")
            ))
        })?;

    let remove_descriptor_tokens = trim_commas(&remove_tokens[idx..from_idx]);
    let remove_counter_type = if remove_descriptor_tokens.is_empty() {
        put_counter_type
    } else {
        if !remove_descriptor_tokens
            .iter()
            .any(|token| token.is_word("counter") || token.is_word("counters"))
        {
            return Err(CardTextError::ParseError(format!(
                "missing counter keyword in put-or-remove remove clause (clause: '{}')",
                words(clause_tokens).join(" ")
            )));
        }
        parse_counter_type_from_tokens(&remove_descriptor_tokens).unwrap_or(put_counter_type)
    };

    let remove_target_tokens = trim_commas(&remove_tokens[from_idx + 1..]);
    let remove_target_words = words(&remove_target_tokens);
    let referential_remove_target = matches!(
        remove_target_words.as_slice(),
        ["it"]
            | ["that", "permanent"]
            | ["that", "artifact"]
            | ["that", "creature"]
            | ["that", "saga"]
            | ["this", "permanent"]
            | ["this", "artifact"]
            | ["this", "creature"]
    );
    if !referential_remove_target {
        return Err(CardTextError::ParseError(format!(
            "unsupported put-or-remove remove target (clause: '{}')",
            words(clause_tokens).join(" ")
        )));
    }

    let (target, target_count) = if let Some((target_count, used_target_count)) =
        parse_counter_target_count_prefix(&base_target_tokens)?
    {
        let target_phrase = &base_target_tokens[used_target_count..];
        if target_phrase.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing counter target before put-or-remove remove clause (clause: '{}')",
                words(clause_tokens).join(" ")
            )));
        }
        (parse_target_phrase(target_phrase)?, Some(target_count))
    } else {
        (parse_target_phrase(&base_target_tokens)?, None)
    };

    let target_phrase = words(&base_target_tokens).join(" ");
    let put_mode_text = format!(
        "Put {} on {}",
        describe_counter_phrase_for_mode(put_count, put_counter_type),
        target_phrase
    );
    let remove_mode_text = {
        let remove_text = words(&remove_tokens).join(" ");
        sentence_case_mode_text(&remove_text)
    };

    Ok(Some(EffectAst::PutOrRemoveCounters {
        put_counter_type,
        put_count: Value::Fixed(put_count as i32),
        remove_counter_type,
        remove_count,
        put_mode_text,
        remove_mode_text,
        target,
        target_count,
    }))
}

fn parse_counter_target_count_prefix(
    tokens: &[Token],
) -> Result<Option<(ChoiceCount, usize)>, CardTextError> {
    if tokens.is_empty() {
        return Ok(None);
    }

    let mut idx = 0usize;
    let mut each_prefix = false;

    if tokens[idx].is_word("each") {
        each_prefix = true;
        idx += 1;
        if tokens.get(idx).is_some_and(|token| token.is_word("of")) {
            idx += 1;
        }
    }

    if each_prefix
        && tokens.get(idx).is_some_and(|token| token.is_word("x"))
        && tokens
            .get(idx + 1)
            .is_some_and(|token| token.is_word("target"))
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported dynamic target count 'each of X target' (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    if each_prefix && tokens.get(idx).is_some_and(|token| token.is_word("target")) {
        return Ok(Some((ChoiceCount::any_number(), idx)));
    }

    if tokens.get(idx).is_some_and(|token| token.is_word("any"))
        && tokens
            .get(idx + 1)
            .is_some_and(|token| token.is_word("number"))
    {
        idx += 2;
        if tokens.get(idx).is_some_and(|token| token.is_word("of")) {
            idx += 1;
        }
        return Ok(Some((ChoiceCount::any_number(), idx)));
    }

    if tokens.get(idx).is_some_and(|token| token.is_word("up"))
        && tokens.get(idx + 1).is_some_and(|token| token.is_word("to"))
    {
        let Some((value, used)) = parse_number(&tokens[idx + 2..]) else {
            return Err(CardTextError::ParseError(format!(
                "missing count after 'up to' in counter target clause (clause: '{}')",
                words(tokens).join(" ")
            )));
        };
        idx += 2 + used;
        if tokens.get(idx).is_some_and(|token| token.is_word("of")) {
            idx += 1;
        }
        return Ok(Some((ChoiceCount::up_to(value as usize), idx)));
    }

    if let Some((first, used_first)) = parse_number(&tokens[idx..]) {
        let mut pos = idx + used_first;
        let mut values = vec![first];
        loop {
            while matches!(tokens.get(pos), Some(Token::Comma(_))) {
                pos += 1;
            }
            if tokens.get(pos).is_some_and(|token| token.is_word("or")) {
                pos += 1;
                while matches!(tokens.get(pos), Some(Token::Comma(_))) {
                    pos += 1;
                }
            }

            let Some((next, used_next)) = parse_number(&tokens[pos..]) else {
                break;
            };
            values.push(next);
            pos += used_next;
        }

        if values.len() >= 2 {
            if tokens.get(pos).is_some_and(|token| token.is_word("of")) {
                pos += 1;
            }
            let min = values.iter().copied().min().unwrap_or(first) as usize;
            let max = values.iter().copied().max().unwrap_or(first) as usize;
            return Ok(Some((
                ChoiceCount {
                    min,
                    max: Some(max),
                    dynamic_x: false,
                },
                pos,
            )));
        }
    }

    if let Some((value, used)) = parse_number(&tokens[idx..]) {
        idx += used;
        if tokens.get(idx).is_some_and(|token| token.is_word("of")) {
            idx += 1;
        }
        return Ok(Some((ChoiceCount::exactly(value as usize), idx)));
    }

    Ok(None)
}

fn parse_tap(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    if tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "tap clause missing target".to_string(),
        ));
    }
    let words = words(tokens);
    if matches!(words.first().copied(), Some("all" | "each")) {
        let filter = parse_object_filter(&tokens[1..], false)?;
        return Ok(EffectAst::TapAll { filter });
    }
    // Handle "tap or untap <target>" as a choice between tapping and untapping.
    if tokens.first().is_some_and(|t| t.is_word("or"))
        && tokens.get(1).is_some_and(|t| t.is_word("untap"))
    {
        let target_tokens = &tokens[2..];
        let target = parse_target_phrase(target_tokens)?;
        return Ok(EffectAst::TapOrUntap {
            target: target.clone(),
        });
    }
    let target = parse_target_phrase(tokens)?;
    Ok(EffectAst::Tap { target })
}

fn parse_sacrifice(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.contains(&"unless") {
        return Err(CardTextError::ParseError(format!(
            "unsupported sacrifice-unless clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let has_greatest_mana_value = clause_words.contains(&"greatest")
        && clause_words.contains(&"mana")
        && clause_words.contains(&"value");
    if has_greatest_mana_value {
        return Err(CardTextError::ParseError(format!(
            "unsupported greatest-mana-value sacrifice clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let has_for_each_graveyard_history = clause_words.contains(&"for")
        && clause_words.contains(&"each")
        && clause_words.contains(&"graveyard")
        && clause_words.contains(&"turn");
    if has_for_each_graveyard_history {
        return Err(CardTextError::ParseError(format!(
            "unsupported graveyard-history sacrifice clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    if tokens
        .first()
        .is_some_and(|token| token.is_word("all") || token.is_word("each"))
    {
        let mut idx = 1usize;
        let mut other = false;
        if tokens
            .get(idx)
            .is_some_and(|token| token.is_word("other") || token.is_word("another"))
        {
            other = true;
            idx += 1;
        }
        let mut filter = parse_object_filter(&tokens[idx..], other)?;
        if other {
            filter.other = true;
        }
        return Ok(EffectAst::SacrificeAll { filter, player });
    }

    let mut idx = 0;
    let mut count = 1u32;
    let mut other = false;
    if let Some((value, used)) = parse_number(&tokens[idx..]) {
        count = value;
        idx += used;
    }
    if tokens
        .get(idx)
        .is_some_and(|token| token.is_word("another"))
    {
        other = true;
        idx += 1;
    }
    if count == 1
        && let Some((value, used)) = parse_number(&tokens[idx..])
    {
        count = value;
        idx += used;
    }

    let filter_tokens = trim_sacrifice_choice_suffix_tokens(&tokens[idx..]);
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing sacrifice object after chooser suffix (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    let mut filter = if let Ok(target) = parse_target_phrase(filter_tokens) {
        target_ast_to_object_filter(target).unwrap_or(parse_object_filter(filter_tokens, other)?)
    } else {
        parse_object_filter(filter_tokens, other)?
    };
    if other {
        filter.other = true;
    }
    if filter.source && count != 1 {
        return Err(CardTextError::ParseError(format!(
            "source sacrifice only supports count 1 (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    let sacrifice_words = words(tokens);
    let excludes_attached_object = sacrifice_words.windows(3).any(|window| {
        matches!(
            window,
            ["than", "enchanted", "creature"]
                | ["than", "enchanted", "permanent"]
                | ["than", "equipped", "creature"]
                | ["than", "equipped", "permanent"]
        )
    });
    if excludes_attached_object
        && filter.controller.is_none()
        && let Some(controller) = controller_filter_for_token_player(player)
    {
        filter.controller = Some(controller);
    }

    Ok(EffectAst::Sacrifice {
        filter,
        player,
        count,
    })
}

fn trim_sacrifice_choice_suffix_tokens(tokens: &[Token]) -> &[Token] {
    let token_words = words(tokens);
    let suffix_word_count = if token_words.ends_with(&["of", "their", "choice"])
        || token_words.ends_with(&["of", "your", "choice"])
        || token_words.ends_with(&["of", "its", "choice"])
    {
        3usize
    } else if token_words.ends_with(&["of", "his", "or", "her", "choice"]) {
        5usize
    } else {
        0usize
    };

    if suffix_word_count == 0 {
        return tokens;
    }

    let keep_words = token_words.len().saturating_sub(suffix_word_count);
    let cut_idx = token_index_for_word_index(tokens, keep_words).unwrap_or(tokens.len());
    &tokens[..cut_idx]
}

fn parse_discard(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    let clause_words = words(tokens);
    if clause_words.contains(&"hand") {
        return Ok(EffectAst::DiscardHand { player });
    }

    let (count, used) = parse_value(tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing discard count (clause: '{}')",
            clause_words.join(" ")
        ))
    })?;

    let rest = &tokens[used..];
    let rest_words = words(rest);
    let Some(card_word_idx) = rest_words
        .iter()
        .position(|word| *word == "card" || *word == "cards")
    else {
        return Err(CardTextError::ParseError(
            "missing card keyword".to_string(),
        ));
    };

    let card_token_idx = token_index_for_word_index(rest, card_word_idx).unwrap_or(rest.len());
    let qualifier_tokens = trim_commas(&rest[..card_token_idx]);
    let mut discard_filter = None;
    if !qualifier_tokens.is_empty() {
        let mut filter = if let Ok(filter) = parse_object_filter(&qualifier_tokens, false) {
            filter
        } else if let Some(filter) = parse_discard_color_qualifier_filter(&qualifier_tokens) {
            filter
        } else {
            return Err(CardTextError::ParseError(format!(
                "unsupported discard card qualifier (clause: '{}')",
                clause_words.join(" ")
            )));
        };
        filter.zone = Some(Zone::Hand);
        discard_filter = Some(filter);
    }

    let trailing_tokens = if card_word_idx + 1 < rest_words.len() {
        let trailing_token_idx =
            token_index_for_word_index(rest, card_word_idx + 1).unwrap_or(rest.len());
        &rest[trailing_token_idx..]
    } else {
        &[]
    };
    let trailing_words = words(trailing_tokens);
    let random = trailing_words.as_slice() == ["at", "random"];
    if !trailing_words.is_empty() && !random {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing discard clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(EffectAst::Discard {
        count,
        player,
        random,
        filter: discard_filter,
    })
}

fn parse_discard_color_qualifier_filter(tokens: &[Token]) -> Option<ObjectFilter> {
    let qualifier_words: Vec<&str> = words(tokens)
        .into_iter()
        .filter(|word| !is_article(word))
        .collect();
    if qualifier_words.is_empty() {
        return None;
    }

    let mut colors = crate::color::ColorSet::new();
    let mut saw_color = false;
    for word in qualifier_words {
        if word == "or" {
            continue;
        }
        let color = parse_color(word)?;
        colors = colors.union(color);
        saw_color = true;
    }

    if !saw_color {
        return None;
    }

    let mut filter = ObjectFilter::default();
    filter.colors = Some(colors);
    Some(filter)
}

#[derive(Debug, Clone, PartialEq)]
enum DelayedReturnTimingAst {
    NextEndStep(PlayerFilter),
    NextUpkeep(PlayerAst),
    EndOfCombat,
}

fn parse_delayed_return_timing_words(words: &[&str]) -> Option<DelayedReturnTimingAst> {
    if matches!(
        words,
        ["at", "end", "of", "combat"] | ["at", "the", "end", "of", "combat"]
    ) {
        return Some(DelayedReturnTimingAst::EndOfCombat);
    }

    if matches!(
        words,
        ["at", "beginning", "of", "next", "end", "step"]
            | ["at", "beginning", "of", "the", "next", "end", "step"]
            | ["at", "the", "beginning", "of", "next", "end", "step"]
            | ["at", "the", "beginning", "of", "the", "next", "end", "step"]
    ) {
        return Some(DelayedReturnTimingAst::NextEndStep(PlayerFilter::Any));
    }

    if matches!(
        words,
        ["at", "beginning", "of", "your", "next", "end", "step"]
            | ["at", "the", "beginning", "of", "your", "next", "end", "step"]
    ) {
        return Some(DelayedReturnTimingAst::NextEndStep(PlayerFilter::You));
    }

    if matches!(
        words,
        ["at", "beginning", "of", "next", "upkeep"]
            | ["at", "beginning", "of", "the", "next", "upkeep"]
            | ["at", "the", "beginning", "of", "next", "upkeep"]
            | ["at", "the", "beginning", "of", "the", "next", "upkeep"]
    ) {
        return Some(DelayedReturnTimingAst::NextUpkeep(PlayerAst::Any));
    }

    if matches!(
        words,
        ["at", "beginning", "of", "your", "next", "upkeep"]
            | ["at", "the", "beginning", "of", "your", "next", "upkeep"]
    ) {
        return Some(DelayedReturnTimingAst::NextUpkeep(PlayerAst::You));
    }

    None
}

fn wrap_return_with_delayed_timing(
    effect: EffectAst,
    timing: Option<DelayedReturnTimingAst>,
) -> EffectAst {
    let Some(timing) = timing else {
        return effect;
    };

    match timing {
        DelayedReturnTimingAst::NextEndStep(player) => EffectAst::DelayedUntilNextEndStep {
            player,
            effects: vec![effect],
        },
        DelayedReturnTimingAst::NextUpkeep(player) => EffectAst::DelayedUntilNextUpkeep {
            player,
            effects: vec![effect],
        },
        DelayedReturnTimingAst::EndOfCombat => EffectAst::DelayedUntilEndOfCombat {
            effects: vec![effect],
        },
    }
}

fn parse_return(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.contains(&"unless") {
        return Err(CardTextError::ParseError(format!(
            "unsupported return-unless clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let to_idx = (0..tokens.len())
        .rev()
        .find(|idx| {
            if !tokens[*idx].is_word("to") {
                return false;
            }
            let tail_words = words(&tokens[*idx + 1..]);
            tail_words.contains(&"hand")
                || tail_words.contains(&"hands")
                || tail_words.contains(&"battlefield")
        })
        .ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing return destination (clause: '{}')",
                words(tokens).join(" ")
            ))
        })?;

    let mut target_tokens_vec = tokens[..to_idx].to_vec();
    let mut random = false;
    let mut random_idx = 0usize;
    while random_idx + 1 < target_tokens_vec.len() {
        if target_tokens_vec[random_idx].is_word("at")
            && target_tokens_vec[random_idx + 1].is_word("random")
        {
            random = true;
            target_tokens_vec.drain(random_idx..random_idx + 2);
            break;
        }
        random_idx += 1;
    }
    let target_tokens = target_tokens_vec.as_slice();
    let destination_tokens_full = &tokens[to_idx + 1..];
    let destination_words_full = words(destination_tokens_full);
    let mut delayed_timing = None;
    let mut destination_word_cutoff = destination_words_full.len();
    for word_idx in 0..destination_words_full.len() {
        if destination_words_full[word_idx] != "at" {
            continue;
        }
        if let Some(timing) = parse_delayed_return_timing_words(&destination_words_full[word_idx..])
        {
            delayed_timing = Some(timing);
            destination_word_cutoff = word_idx;
            break;
        }
    }

    let destination_tokens = if destination_word_cutoff < destination_words_full.len() {
        let token_cutoff = token_index_for_word_index(destination_tokens_full, destination_word_cutoff)
            .unwrap_or(destination_tokens_full.len());
        &destination_tokens_full[..token_cutoff]
    } else {
        destination_tokens_full
    };

    let mut destination_words = words(destination_tokens);
    let mut destination_excluded_subtypes: Vec<Subtype> = Vec::new();
    if let Some(except_idx) = destination_words
        .windows(2)
        .position(|window| window == ["except", "for"])
    {
        let exception_words = &destination_words[except_idx + 2..];
        if exception_words.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing return exception qualifiers (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        for word in exception_words {
            if matches!(*word, "and" | "or") {
                continue;
            }
            let Some(subtype) = parse_subtype_word(word)
                .or_else(|| word.strip_suffix('s').and_then(parse_subtype_word))
            else {
                return Err(CardTextError::ParseError(format!(
                    "unsupported return exception qualifier '{}' (clause: '{}')",
                    word,
                    words(tokens).join(" ")
                )));
            };
            if !destination_excluded_subtypes.contains(&subtype) {
                destination_excluded_subtypes.push(subtype);
            }
        }
        if destination_excluded_subtypes.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing subtype return exception qualifiers (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        destination_words.truncate(except_idx);
    }
    let is_hand = destination_words.contains(&"hand") || destination_words.contains(&"hands");
    let is_battlefield = destination_words.contains(&"battlefield");
    let tapped = destination_words.contains(&"tapped");
    let return_controller = if destination_words
        .windows(3)
        .any(|window| window == ["under", "your", "control"])
    {
        ReturnControllerAst::You
    } else if destination_words
        .iter()
        .any(|word| *word == "owner" || *word == "owners")
        && destination_words.contains(&"control")
    {
        ReturnControllerAst::Owner
    } else {
        ReturnControllerAst::Preserve
    };
    if destination_words.contains(&"transformed") {
        return Err(CardTextError::ParseError(format!(
            "unsupported transformed return clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    let has_delayed_timing_words = destination_words_full.contains(&"beginning")
        || destination_words_full.contains(&"upkeep")
        || destination_words_full
            .windows(3)
            .any(|window| window == ["end", "of", "combat"])
        || destination_words_full.contains(&"end")
            && (destination_words_full.contains(&"next") || destination_words_full.contains(&"step"));
    if delayed_timing.is_none() && has_delayed_timing_words {
        return Err(CardTextError::ParseError(format!(
            "unsupported delayed return timing clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }
    if !is_hand && !is_battlefield {
        return Err(CardTextError::ParseError(format!(
            "unsupported return destination (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let target_words = words(target_tokens);
    if let Some(and_idx) = target_tokens.iter().position(|token| token.is_word("and"))
        && and_idx > 0
    {
        let tail_words = words(&target_tokens[and_idx + 1..]);
        let starts_multi_target = tail_words.first() == Some(&"target")
            || (tail_words.starts_with(&["up", "to"]) && tail_words.contains(&"target"));
        if starts_multi_target {
            return Err(CardTextError::ParseError(format!(
                "unsupported multi-target return clause (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
    }
    if !target_words.contains(&"target")
        && target_words.contains(&"exiled")
        && target_words.contains(&"cards")
    {
        let filter = parse_object_filter(target_tokens, false)?;
        let effect = if is_battlefield {
            EffectAst::ReturnAllToBattlefield { filter, tapped }
        } else {
            EffectAst::ReturnAllToHand { filter }
        };
        return Ok(wrap_return_with_delayed_timing(effect, delayed_timing));
    }
    if target_words
        .first()
        .is_some_and(|word| *word == "all" || *word == "each")
    {
        let has_unsupported_return_all_qualifier = target_words.contains(&"dealt")
            || target_words.contains(&"without") && target_words.contains(&"counter");
        if has_unsupported_return_all_qualifier {
            return Err(CardTextError::ParseError(format!(
                "unsupported qualified return-all filter (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        if target_tokens.len() < 2 {
            return Err(CardTextError::ParseError(
                "missing return-all filter".to_string(),
            ));
        }
        let return_filter_tokens = &target_tokens[1..];
        if is_hand
            && let Some((choice_idx, consumed)) = find_color_choice_phrase(return_filter_tokens)
        {
            let base_filter_tokens = trim_commas(&return_filter_tokens[..choice_idx]);
            let trailing = trim_commas(&return_filter_tokens[choice_idx + consumed..]);
            if !trailing.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "unsupported trailing color-choice return-all clause (clause: '{}')",
                    words(tokens).join(" ")
                )));
            }
            if base_filter_tokens.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "missing return-all filter before color-choice clause (clause: '{}')",
                    words(tokens).join(" ")
                )));
            }
            let mut filter = parse_object_filter(&base_filter_tokens, false)?;
            for subtype in destination_excluded_subtypes {
                if !filter.excluded_subtypes.contains(&subtype) {
                    filter.excluded_subtypes.push(subtype);
                }
            }
            return Ok(wrap_return_with_delayed_timing(
                EffectAst::ReturnAllToHandOfChosenColor { filter },
                delayed_timing,
            ));
        }
        let mut filter = parse_object_filter(return_filter_tokens, false)?;
        for subtype in destination_excluded_subtypes {
            if !filter.excluded_subtypes.contains(&subtype) {
                filter.excluded_subtypes.push(subtype);
            }
        }
        let effect = if is_battlefield {
            EffectAst::ReturnAllToBattlefield { filter, tapped }
        } else {
            EffectAst::ReturnAllToHand { filter }
        };
        return Ok(wrap_return_with_delayed_timing(effect, delayed_timing));
    }
    if !destination_excluded_subtypes.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "unsupported return exception on non-return-all clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let target = parse_target_phrase(target_tokens)?;
    let effect = if is_battlefield {
        EffectAst::ReturnToBattlefield {
            target,
            tapped,
            controller: return_controller,
        }
    } else {
        EffectAst::ReturnToHand { target, random }
    };
    Ok(wrap_return_with_delayed_timing(effect, delayed_timing))
}

fn parse_exchange(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["control", "of"]) {
        return Err(CardTextError::ParseError(format!(
            "unsupported exchange clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    // Heterogeneous "exchange control of A and B" forms (e.g. "this artifact and target ...")
    // cannot be represented by the current single-filter ExchangeControl primitive.
    if let Some(and_idx) = tokens.iter().position(|token| token.is_word("and")) {
        let left_words = words(&tokens[..and_idx]);
        let right_words = words(&tokens[and_idx + 1..]);
        let left_mentions_this = left_words.contains(&"this");
        let right_mentions_this = right_words.contains(&"this");
        let left_mentions_target = left_words.contains(&"target");
        let right_mentions_target = right_words.contains(&"target");
        if left_mentions_this
            || right_mentions_this
            || left_mentions_target
            || right_mentions_target
        {
            return Err(CardTextError::ParseError(format!(
                "unsupported heterogeneous exchange clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    }

    let mut idx = 2usize;
    let mut count = 2u32;
    if let Some((value, used)) = parse_number(&tokens[idx..]) {
        count = value;
        idx += used;
    }
    if tokens.get(idx).is_some_and(|token| token.is_word("target")) {
        idx += 1;
    }
    if idx >= tokens.len() {
        return Err(CardTextError::ParseError(
            "missing exchange target filter".to_string(),
        ));
    }

    let mut shared_type = None;
    let mut filter_tokens = &tokens[idx..];
    let tail_words = words(filter_tokens);
    if let Some(rel_word_idx) = tail_words
        .windows(2)
        .position(|window| window[0] == "that" && matches!(window[1], "share" | "shares"))
    {
        let rel_token_idx =
            token_index_for_word_index(filter_tokens, rel_word_idx).unwrap_or(filter_tokens.len());
        let (head, tail) = filter_tokens.split_at(rel_token_idx);
        filter_tokens = head;

        let share_words = words(tail);
        let share_head = if share_words.starts_with(&["that", "share"]) {
            &share_words[2..]
        } else if share_words.starts_with(&["that", "shares"]) {
            &share_words[2..]
        } else {
            &share_words[..]
        };
        let share_head = if share_head.first().copied() == Some("a") {
            &share_head[1..]
        } else {
            share_head
        };
        if share_head.starts_with(&["permanent", "type"]) {
            shared_type = Some(SharedTypeConstraintAst::PermanentType);
        } else if share_head.starts_with(&["card", "type"]) {
            shared_type = Some(SharedTypeConstraintAst::CardType);
        } else {
            return Err(CardTextError::ParseError(format!(
                "unsupported exchange share-type clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    }

    let filter = parse_object_filter(filter_tokens, false)?;
    Ok(EffectAst::ExchangeControl {
        filter,
        count,
        shared_type,
    })
}

fn parse_become(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let Some(SubjectAst::Player(player)) = subject else {
        return Err(CardTextError::ParseError(format!(
            "unsupported become clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    };

    let amount = parse_value(tokens).map(|(value, _)| value).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing life total amount (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;
    Ok(EffectAst::SetLifeTotal { amount, player })
}

fn parse_switch(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    use crate::effect::Until;

    let clause_words = words(tokens);

    // Split off trailing duration, if present.
    let (duration, remainder) =
        if let Some((duration, remainder)) = parse_restriction_duration(tokens)? {
            (duration, remainder)
        } else {
            (Until::EndOfTurn, trim_commas(tokens).to_vec())
        };

    let remainder_words = words(&remainder);
    let Some(power_idx) = remainder.iter().position(|token| token.is_word("power")) else {
        return Err(CardTextError::ParseError(format!(
            "unsupported switch clause (clause: '{}')",
            clause_words.join(" ")
        )));
    };

    // Target phrase is everything up to "power".
    let target_tokens = &remainder[..power_idx];
    let target_words = words(target_tokens);
    let target = if target_words.is_empty()
        || matches!(
            target_words.as_slice(),
            ["this"]
                | ["this", "creature"]
                | ["this", "creatures"]
                | ["this", "permanent"]
                | ["it"]
        ) {
        if target_words == ["it"] {
            TargetAst::Tagged(TagKey::from(IT_TAG), span_from_tokens(target_tokens))
        } else {
            TargetAst::Source(span_from_tokens(target_tokens))
        }
    } else {
        parse_target_phrase(target_tokens)?
    };

    // Require "... power and toughness ..." somewhere in remainder.
    if !remainder_words.contains(&"power") || !remainder_words.contains(&"toughness") {
        return Err(CardTextError::ParseError(format!(
            "unsupported switch clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(EffectAst::SwitchPowerToughness { target, duration })
}

fn parse_skip(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let Some(SubjectAst::Player(player)) = subject else {
        return Err(CardTextError::ParseError(format!(
            "unsupported skip clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    };

    let words = words(tokens);
    let skips_next_combat_phase_this_turn = words.contains(&"combat")
        && words.contains(&"phase")
        && words.contains(&"next")
        && words.contains(&"this")
        && words.contains(&"turn");
    if skips_next_combat_phase_this_turn {
        return Ok(EffectAst::SkipNextCombatPhaseThisTurn { player });
    }
    if words.contains(&"combat")
        && (words.contains(&"phase") || words.contains(&"phases"))
        && words.contains(&"turn")
    {
        return Ok(EffectAst::SkipCombatPhases { player });
    }
    if words.contains(&"draw") && words.contains(&"step") {
        return Ok(EffectAst::SkipDrawStep { player });
    }
    if words.contains(&"turn") {
        return Ok(EffectAst::SkipTurn { player });
    }

    Err(CardTextError::ParseError(format!(
        "unsupported skip clause (clause: '{}')",
        words.join(" ")
    )))
}

fn parse_transform(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    if tokens.is_empty() {
        return Ok(EffectAst::Transform {
            target: TargetAst::Source(None),
        });
    }
    let target = parse_target_phrase(tokens)?;
    Ok(EffectAst::Transform { target })
}

fn parse_flip(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    if tokens.is_empty() {
        return Ok(EffectAst::Flip {
            target: TargetAst::Source(None),
        });
    }

    let target_words = words(tokens);
    if target_words == ["it"]
        || target_words == ["this"]
        || target_words == ["this", "creature"]
        || target_words == ["this", "permanent"]
    {
        return Ok(EffectAst::Flip {
            target: TargetAst::Source(span_from_tokens(tokens)),
        });
    }

    let target = parse_target_phrase(tokens)?;
    Ok(EffectAst::Flip { target })
}

fn parse_regenerate(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let words = words(tokens);
    if matches!(words.first().copied(), Some("all" | "each")) {
        if tokens.len() < 2 {
            return Err(CardTextError::ParseError(
                "regenerate clause missing filter after each/all".to_string(),
            ));
        }
        let filter = parse_object_filter(&tokens[1..], false)?;
        return Ok(EffectAst::RegenerateAll { filter });
    }
    let target = parse_target_phrase(tokens)?;
    Ok(EffectAst::Regenerate { target })
}

fn parse_mill(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let (count, used) = parse_value(tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing mill count (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;

    let rest = &tokens[used..];
    if rest
        .first()
        .and_then(Token::as_word)
        .is_some_and(|word| word != "card" && word != "cards")
    {
        return Err(CardTextError::ParseError(
            "missing card keyword".to_string(),
        ));
    }
    let trailing_words: Vec<&str> = rest.iter().skip(1).filter_map(Token::as_word).collect();
    if !trailing_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing mill clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    Ok(EffectAst::Mill { count, player })
}

fn parse_equal_to_number_of_filter_value(tokens: &[Token]) -> Option<Value> {
    let words_all = words(tokens);
    let equal_idx = words_all
        .windows(2)
        .position(|window| window == ["equal", "to"])?;
    let mut number_word_idx = equal_idx + 2;
    if words_all.get(number_word_idx).copied() == Some("the") {
        number_word_idx += 1;
    }
    if words_all.get(number_word_idx).copied() != Some("number")
        || words_all.get(number_word_idx + 1).copied() != Some("of")
    {
        return None;
    }
    let filter_start_word_idx = number_word_idx + 2;
    let filter_start_token_idx = token_index_for_word_index(tokens, filter_start_word_idx)?;
    let filter_tokens = trim_edge_punctuation(&tokens[filter_start_token_idx..]);
    let filter = parse_object_filter(&filter_tokens, false).ok()?;
    Some(Value::Count(filter))
}

fn parse_equal_to_number_of_filter_plus_or_minus_fixed_value(tokens: &[Token]) -> Option<Value> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["equal", "to"]) {
        return None;
    }

    let mut number_word_idx = 2usize;
    if clause_words.get(number_word_idx).copied() == Some("the") {
        number_word_idx += 1;
    }
    if clause_words.get(number_word_idx).copied() != Some("number")
        || clause_words.get(number_word_idx + 1).copied() != Some("of")
    {
        return None;
    }

    let filter_start_word_idx = number_word_idx + 2;
    let operator_word_idx = (filter_start_word_idx + 1..clause_words.len())
        .find(|idx| matches!(clause_words[*idx], "plus" | "minus"))?;
    let operator = clause_words[operator_word_idx];

    let filter_start_token_idx = token_index_for_word_index(tokens, filter_start_word_idx)?;
    let operator_token_idx = token_index_for_word_index(tokens, operator_word_idx)?;
    let filter_tokens = trim_commas(&tokens[filter_start_token_idx..operator_token_idx]);
    let filter = parse_object_filter(&filter_tokens, false).ok()?;

    let offset_start_token_idx = token_index_for_word_index(tokens, operator_word_idx + 1)?;
    let offset_tokens = trim_commas(&tokens[offset_start_token_idx..]);
    let (offset_value, used) = parse_number(&offset_tokens)?;
    let trailing_words = words(&offset_tokens[used..]);
    if !trailing_words.is_empty() {
        return None;
    }

    let signed_offset = if operator == "minus" {
        -(offset_value as i32)
    } else {
        offset_value as i32
    };
    Some(Value::Add(
        Box::new(Value::Count(filter)),
        Box::new(Value::Fixed(signed_offset)),
    ))
}

fn parse_equal_to_number_of_opponents_you_have_value(tokens: &[Token]) -> Option<Value> {
    let clause_words = words(tokens);
    if matches!(
        clause_words.as_slice(),
        ["equal", "to", "the", "number", "of", "opponents", "you", "have"]
            | ["equal", "to", "number", "of", "opponents", "you", "have"]
    ) {
        return Some(Value::CountPlayers(PlayerFilter::Opponent));
    }
    None
}

fn parse_equal_to_number_of_counters_on_reference_value(tokens: &[Token]) -> Option<Value> {
    let clause_words = words(tokens);
    if !clause_words.starts_with(&["equal", "to"]) {
        return None;
    }

    let mut idx = 2usize;
    if clause_words.get(idx).copied() == Some("the") {
        idx += 1;
    }
    if clause_words.get(idx).copied() != Some("number")
        || clause_words.get(idx + 1).copied() != Some("of")
    {
        return None;
    }
    idx += 2;

    if clause_words
        .get(idx)
        .is_some_and(|word| is_article(word) || *word == "one")
    {
        idx += 1;
    }

    let mut counter_type = None;
    if let Some(word) = clause_words.get(idx).copied()
        && let Some(parsed) = parse_counter_type_word(word)
    {
        counter_type = Some(parsed);
        idx += 1;
    }

    if !matches!(clause_words.get(idx).copied(), Some("counter" | "counters")) {
        return None;
    }
    idx += 1;

    if clause_words.get(idx).copied() != Some("on") {
        return None;
    }
    idx += 1;

    let reference = &clause_words[idx..];
    if reference.is_empty() {
        return None;
    }

    if matches!(
        reference,
        ["it"] | ["this"] | ["this", "creature"] | ["this", "permanent"] | ["this", "source"]
    ) {
        return Some(match counter_type {
            Some(counter_type) => Value::CountersOnSource(counter_type),
            None => Value::CountersOn(Box::new(ChooseSpec::Source), None),
        });
    }

    if matches!(
        reference,
        ["that"]
            | ["that", "creature"]
            | ["that", "permanent"]
            | ["that", "object"]
            | ["those"]
            | ["those", "creatures"]
            | ["those", "permanents"]
    ) {
        return Some(Value::CountersOn(
            Box::new(ChooseSpec::Tagged(TagKey::from(IT_TAG))),
            counter_type,
        ));
    }

    None
}

fn parse_equal_to_aggregate_filter_value(tokens: &[Token]) -> Option<Value> {
    let clause_words = words(tokens);
    let equal_idx = clause_words
        .windows(2)
        .position(|window| window == ["equal", "to"])?;

    let mut idx = equal_idx + 2;
    if clause_words.get(idx).copied() == Some("the") {
        idx += 1;
    }

    let aggregate = match clause_words.get(idx).copied() {
        Some("total") => "total",
        Some("greatest") => "greatest",
        _ => return None,
    };
    idx += 1;

    let value_kind = if clause_words.get(idx).copied() == Some("power") {
        idx += 1;
        "power"
    } else if clause_words.get(idx).copied() == Some("toughness") {
        idx += 1;
        "toughness"
    } else if clause_words.get(idx).copied() == Some("mana")
        && clause_words.get(idx + 1).copied() == Some("value")
    {
        idx += 2;
        "mana_value"
    } else {
        return None;
    };

    if !matches!(clause_words.get(idx).copied(), Some("of" | "among")) {
        return None;
    }
    idx += 1;

    let object_start_token_idx = token_index_for_word_index(tokens, idx)?;
    let filter_tokens = &tokens[object_start_token_idx..];
    let filter = parse_object_filter(filter_tokens, false).ok()?;

    match (aggregate, value_kind) {
        ("total", "power") => Some(Value::TotalPower(filter)),
        ("total", "toughness") => Some(Value::TotalToughness(filter)),
        ("total", "mana_value") => Some(Value::TotalManaValue(filter)),
        ("greatest", "power") => Some(Value::GreatestPower(filter)),
        ("greatest", "mana_value") => Some(Value::GreatestManaValue(filter)),
        _ => None,
    }
}

fn parse_get(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let words = words(tokens);
    if words.contains(&"poison") && words.contains(&"counter") {
        let player = match subject {
            Some(SubjectAst::Player(player)) => player,
            _ => PlayerAst::Implicit,
        };
        return Ok(EffectAst::PoisonCounters {
            count: Value::Fixed(1),
            player,
        });
    }

    let energy_count = tokens.iter().filter(|token| token.is_word("e")).count();
    if energy_count > 0 {
        let player = match subject {
            Some(SubjectAst::Player(player)) => player,
            _ => PlayerAst::Implicit,
        };
        let count = parse_add_mana_equal_amount_value(tokens)
            .or(parse_equal_to_number_of_filter_value(tokens))
            .or(parse_dynamic_cost_modifier_value(tokens)?)
            .unwrap_or(Value::Fixed(energy_count as i32));
        return Ok(EffectAst::EnergyCounters { count, player });
    }

    if let Some(mod_token) = tokens.first().and_then(Token::as_word)
        && let Ok((power, toughness)) = parse_pt_modifier_values(mod_token)
    {
        let (power, toughness, duration, condition) =
            parse_get_modifier_values_with_tail(tokens, power, toughness)?;
        let target = match subject {
            Some(SubjectAst::This) => TargetAst::Source(None),
            _ => {
                return Err(CardTextError::ParseError(
                    "unsupported get clause (missing subject)".to_string(),
                ));
            }
        };
        return Ok(EffectAst::Pump {
            power,
            toughness,
            target,
            duration,
            condition,
        });
    }

    Err(CardTextError::ParseError(format!(
        "unsupported get clause (clause: '{}')",
        words.join(" ")
    )))
}

fn parse_add_mana(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };
    parser_trace_stack("parse_add_mana:entry", tokens);
    let clause_words = words(tokens);

    let has_card_word = clause_words
        .iter()
        .any(|word| *word == "card" || *word == "cards");
    if clause_words.contains(&"exiled") && has_card_word && clause_words.contains(&"colors") {
        return Ok(EffectAst::AddManaImprintedColors);
    }

    if (clause_words.contains(&"commander") || clause_words.contains(&"commanders"))
        && clause_words.contains(&"color")
        && clause_words.contains(&"identity")
    {
        let amount = parse_value(tokens)
            .map(|(value, _)| value)
            .unwrap_or(Value::Fixed(1));
        return Ok(EffectAst::AddManaCommanderIdentity { amount, player });
    }

    if let Some(available_colors) = parse_any_combination_mana_colors(tokens)? {
        let amount = parse_value(tokens)
            .map(|(value, _)| value)
            .unwrap_or(Value::Fixed(1));
        return Ok(EffectAst::AddManaAnyColor {
            amount,
            player,
            available_colors: Some(available_colors),
        });
    }

    if let Some(available_colors) = parse_or_mana_color_choices(tokens)? {
        return Ok(EffectAst::AddManaAnyColor {
            amount: Value::Fixed(1),
            player,
            available_colors: Some(available_colors),
        });
    }

    // "Add one mana of the chosen color."
    let has_explicit_symbol = tokens
        .iter()
        .filter_map(Token::as_word)
        .any(|word| parse_mana_symbol(word).is_ok());
    if !has_explicit_symbol
        && let Some(chosen_idx) = clause_words
            .windows(2)
            .position(|window| window == ["chosen", "color"])
    {
        let prefix = &clause_words[..chosen_idx];
        let references_mana_of_chosen_color =
            prefix.ends_with(&["mana", "of", "the"]) || prefix.ends_with(&["mana", "of"]);
        if references_mana_of_chosen_color {
            let tail_words = &clause_words[chosen_idx + 2..];
            let has_only_pool_tail = tail_words.is_empty()
                || tail_words.iter().all(|word| {
                    matches!(
                        *word,
                        "to" | "your"
                            | "their"
                            | "its"
                            | "that"
                            | "player"
                            | "players"
                            | "mana"
                            | "pool"
                    )
                });
            if has_only_pool_tail {
                let amount = parse_value(tokens)
                    .map(|(value, _)| value)
                    .unwrap_or(Value::Fixed(1));
                return Ok(EffectAst::AddManaChosenColor {
                    amount,
                    player,
                    fixed_option: None,
                });
            }
        }
    }

    let any_one = clause_words
        .windows(3)
        .any(|window| window == ["any", "one", "color"] || window == ["any", "one", "type"]);
    let any_color = clause_words
        .windows(2)
        .any(|window| window == ["any", "color"] || window == ["one", "color"]);
    let any_type = clause_words
        .windows(2)
        .any(|window| window == ["any", "type"] || window == ["one", "type"]);
    if any_color || any_type {
        let mut amount = parse_value(tokens)
            .map(|(value, _)| value)
            .unwrap_or(Value::Fixed(1));
        let allow_colorless = any_type;
        let phrase_end = tokens
            .iter()
            .enumerate()
            .find_map(|(idx, token)| {
                let word = token.as_word()?;
                if (word == "color" && any_color) || (word == "type" && any_type) {
                    Some(idx + 1)
                } else {
                    None
                }
            })
            .unwrap_or(tokens.len());
        let tail_tokens = trim_leading_commas(&tokens[phrase_end..]);

        if tail_tokens.is_empty() || is_mana_pool_tail_tokens(tail_tokens) {
            if any_type {
                return Err(CardTextError::ParseError(format!(
                    "unsupported any-type mana clause without producer filter (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            if any_one {
                return Ok(EffectAst::AddManaAnyOneColor { amount, player });
            }
            return Ok(EffectAst::AddManaAnyColor {
                amount,
                player,
                available_colors: None,
            });
        }

        if let Some(filter) = parse_land_could_produce_filter(tail_tokens)? {
            parser_trace_stack("parse_add_mana:land-could-produce", tokens);
            return Ok(EffectAst::AddManaFromLandCouldProduce {
                amount,
                player,
                land_filter: filter,
                allow_colorless,
                same_type: any_one,
            });
        }

        if matches!(amount, Value::X)
            && let Some(dynamic_amount) = parse_where_x_is_number_of_filter_value(tail_tokens)
        {
            amount = dynamic_amount;
            if any_type {
                return Err(CardTextError::ParseError(format!(
                    "unsupported any-type mana clause without producer filter (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            if any_one {
                return Ok(EffectAst::AddManaAnyOneColor { amount, player });
            }
            return Ok(EffectAst::AddManaAnyColor {
                amount,
                player,
                available_colors: None,
            });
        }

        return Err(CardTextError::ParseError(format!(
            "unsupported trailing mana clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    let for_each_idx = tokens
        .windows(2)
        .position(|window| window[0].is_word("for") && window[1].is_word("each"));
    let mana_scan_end = for_each_idx.unwrap_or(tokens.len());

    let mut mana = Vec::new();
    let mut last_mana_idx = None;
    for (idx, token) in tokens[..mana_scan_end].iter().enumerate() {
        if let Some(word) = token.as_word() {
            if word == "mana" || word == "to" || word == "your" || word == "pool" {
                continue;
            }
            if let Ok(symbol) = parse_mana_symbol(word) {
                mana.push(symbol);
                last_mana_idx = Some(idx);
            }
        }
    }

    if !mana.is_empty() {
        if let Some(amount) = parse_add_mana_that_much_value(tokens) {
            parser_trace_stack("parse_add_mana:scaled-that-much", tokens);
            return Ok(EffectAst::AddManaScaled {
                mana,
                amount,
                player,
            });
        }
        if let Some(amount) = parse_devotion_value_from_add_clause(tokens)? {
            parser_trace_stack("parse_add_mana:scaled-devotion", tokens);
            return Ok(EffectAst::AddManaScaled {
                mana,
                amount,
                player,
            });
        }
        if let Some(for_each_idx) = for_each_idx {
            let amount_tokens = &tokens[for_each_idx..];
            let amount = parse_dynamic_cost_modifier_value(amount_tokens)?.ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "unsupported dynamic mana amount (clause: '{}')",
                    words(tokens).join(" ")
                ))
            })?;
            parser_trace_stack("parse_add_mana:scaled", tokens);
            return Ok(EffectAst::AddManaScaled {
                mana,
                amount,
                player,
            });
        }
        if let Some(amount) = parse_add_mana_equal_amount_value(tokens) {
            parser_trace_stack("parse_add_mana:scaled-equal", tokens);
            return Ok(EffectAst::AddManaScaled {
                mana,
                amount,
                player,
            });
        }
        let trailing_words = if let Some(last_idx) = last_mana_idx {
            words(&tokens[last_idx + 1..])
        } else {
            Vec::new()
        };
        if !trailing_words.is_empty() {
            let chosen_color_tail =
                trailing_words.starts_with(&["or", "one", "mana", "of", "the", "chosen", "color"]);
            let pool_tail = if chosen_color_tail {
                trailing_words[7..].to_vec()
            } else {
                Vec::new()
            };
            let has_only_pool_tail = chosen_color_tail
                && (pool_tail.is_empty()
                    || pool_tail
                        .iter()
                        .all(|word| matches!(*word, "to" | "your" | "mana" | "pool")));
            if chosen_color_tail && has_only_pool_tail {
                if mana.len() != 1 {
                    return Err(CardTextError::ParseError(format!(
                        "unsupported chosen-color mana clause with multiple symbols (clause: '{}')",
                        clause_words.join(" ")
                    )));
                }
                let Some(color) = mana_symbol_to_color(mana[0]) else {
                    return Err(CardTextError::ParseError(format!(
                        "unsupported chosen-color mana clause with non-colored symbol (clause: '{}')",
                        clause_words.join(" ")
                    )));
                };
                parser_trace_stack("parse_add_mana:chosen-color-option", tokens);
                return Ok(EffectAst::AddManaChosenColor {
                    amount: Value::Fixed(1),
                    player,
                    fixed_option: Some(color),
                });
            }
        }
        let has_only_pool_tail = !trailing_words.is_empty()
            && trailing_words
                .iter()
                .all(|word| matches!(*word, "to" | "your" | "mana" | "pool"));
        if !trailing_words.is_empty() && !has_only_pool_tail {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing mana clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        parser_trace_stack("parse_add_mana:flat", tokens);
        return Ok(EffectAst::AddMana { mana, player });
    }

    Err(CardTextError::ParseError(format!(
        "missing mana symbols (clause: '{}')",
        clause_words.join(" ")
    )))
}

fn mana_symbol_to_color(symbol: ManaSymbol) -> Option<crate::color::Color> {
    match symbol {
        ManaSymbol::White => Some(crate::color::Color::White),
        ManaSymbol::Blue => Some(crate::color::Color::Blue),
        ManaSymbol::Black => Some(crate::color::Color::Black),
        ManaSymbol::Red => Some(crate::color::Color::Red),
        ManaSymbol::Green => Some(crate::color::Color::Green),
        _ => None,
    }
}

fn parse_or_mana_color_choices(
    tokens: &[Token],
) -> Result<Option<Vec<crate::color::Color>>, CardTextError> {
    let clause_words = words(tokens);
    if !clause_words.contains(&"or") {
        return Ok(None);
    }

    let mut colors = Vec::new();
    let mut has_or = false;
    for token in tokens {
        let Some(word) = token.as_word() else {
            continue;
        };
        if word == "or" {
            has_or = true;
            continue;
        }
        if matches!(word, "to" | "your" | "their" | "its" | "mana" | "pool") {
            continue;
        }
        if let Ok(symbol) = parse_mana_symbol(word) {
            let Some(color) = mana_symbol_to_color(symbol) else {
                return Ok(None);
            };
            if !colors.contains(&color) {
                colors.push(color);
            }
            continue;
        }
        return Ok(None);
    }

    if !has_or || colors.len() < 2 {
        return Ok(None);
    }

    Ok(Some(colors))
}

fn parse_any_combination_mana_colors(
    tokens: &[Token],
) -> Result<Option<Vec<crate::color::Color>>, CardTextError> {
    let clause_words = words(tokens);
    let Some(combination_idx) = clause_words
        .windows(3)
        .position(|window| window == ["any", "combination", "of"])
    else {
        return Ok(None);
    };

    let mut colors = Vec::new();
    for word in &clause_words[combination_idx + 3..] {
        if matches!(
            *word,
            "and" | "or" | "mana" | "to" | "your" | "their" | "its" | "pool"
        ) {
            continue;
        }
        let symbol = parse_mana_symbol(word).map_err(|_| {
            CardTextError::ParseError(format!(
                "unsupported restricted mana symbol '{}' in any-combination clause (clause: '{}')",
                word,
                clause_words.join(" ")
            ))
        })?;
        let color = mana_symbol_to_color(symbol).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "unsupported non-colored mana symbol '{}' in any-combination clause (clause: '{}')",
                word,
                clause_words.join(" ")
            ))
        })?;
        if !colors.contains(&color) {
            colors.push(color);
        }
    }

    if colors.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing color options in any-combination mana clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(Some(colors))
}

fn trim_leading_commas(tokens: &[Token]) -> &[Token] {
    let start = tokens
        .iter()
        .position(|token| !matches!(token, Token::Comma(_)))
        .unwrap_or(tokens.len());
    &tokens[start..]
}

fn is_mana_pool_tail_tokens(tokens: &[Token]) -> bool {
    let words = words(tokens);
    if words.is_empty() || words[0] != "to" || !words.contains(&"mana") || !words.contains(&"pool")
    {
        return false;
    }
    words.iter().all(|word| {
        matches!(
            *word,
            "to" | "your" | "their" | "its" | "that" | "player" | "players" | "mana" | "pool"
        )
    })
}

fn parse_land_could_produce_filter(
    tokens: &[Token],
) -> Result<Option<ObjectFilter>, CardTextError> {
    let words = words(tokens);
    if words.len() < 3 || words[0] != "that" {
        return Ok(None);
    }

    let marker_word_idx = if let Some(could_idx) = words
        .windows(2)
        .position(|window| window == ["could", "produce"])
    {
        if could_idx + 2 != words.len() {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing mana clause (tail: '{}')",
                words.join(" ")
            )));
        }
        could_idx
    } else if let Some(produced_idx) = words.iter().position(|word| *word == "produced") {
        if produced_idx + 1 != words.len() {
            return Err(CardTextError::ParseError(format!(
                "unsupported trailing mana clause (tail: '{}')",
                words.join(" ")
            )));
        }
        produced_idx
    } else {
        return Ok(None);
    };

    let marker_token_idx =
        token_index_for_word_index(tokens, marker_word_idx).ok_or_else(|| {
            CardTextError::ParseError(format!(
                "missing mana production marker in tail '{}'",
                words.join(" ")
            ))
        })?;
    let filter_tokens = trim_leading_commas(&tokens[1..marker_token_idx]);
    if filter_tokens.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing land filter in mana clause (tail: '{}')",
            words.join(" ")
        )));
    }
    let filter = parse_object_filter(filter_tokens, false)?;
    Ok(Some(filter))
}

fn looks_like_pt_word(word: &str) -> bool {
    let Some((power, toughness)) = word.split_once('/') else {
        return false;
    };
    let is_component = |part: &str| {
        let part = part.trim_matches(|ch| matches!(ch, '+' | '-'));
        part == "x" || part == "*" || part.parse::<i32>().is_ok()
    };
    is_component(power) && is_component(toughness)
}

fn parse_unsigned_pt_word(word: &str) -> Option<(i32, i32)> {
    let (power, toughness) = word.split_once('/')?;
    if power.starts_with('+')
        || toughness.starts_with('+')
        || power.starts_with('-')
        || toughness.starts_with('-')
    {
        return None;
    }
    let power = power.parse::<i32>().ok()?;
    let toughness = toughness.parse::<i32>().ok()?;
    Some((power, toughness))
}

fn is_probable_token_name_word(word: &str) -> bool {
    if !word
        .chars()
        .all(|ch| ch.is_ascii_alphabetic() || ch == '\'' || ch == '-')
    {
        return false;
    }
    !matches!(
        word,
        "legendary"
            | "artifact"
            | "enchantment"
            | "creature"
            | "token"
            | "tokens"
            | "white"
            | "blue"
            | "black"
            | "red"
            | "green"
            | "colorless"
    )
}

fn parse_copy_modifiers_from_tail(
    tail_words: &[&str],
) -> (
    Option<ColorSet>,
    Option<Vec<CardType>>,
    Option<Vec<Subtype>>,
    Vec<CardType>,
    Vec<Subtype>,
    Vec<Supertype>,
    Option<(i32, i32)>,
    Vec<StaticAbility>,
) {
    let mut set_colors = None;
    let mut set_card_types = None;
    let mut set_subtypes = None;
    let mut added_card_types = Vec::new();
    let mut added_subtypes = Vec::new();
    let mut removed_supertypes = Vec::new();
    let mut set_base_power_toughness = None;
    let mut granted_abilities = Vec::new();

    let except_idx = tail_words.iter().rposition(|word| *word == "except");
    let modifier_words = except_idx
        .map(|idx| &tail_words[idx + 1..])
        .unwrap_or_default();
    if modifier_words.is_empty() {
        return (
            set_colors,
            set_card_types,
            set_subtypes,
            added_card_types,
            added_subtypes,
            removed_supertypes,
            set_base_power_toughness,
            granted_abilities,
        );
    }

    if modifier_words
        .windows(2)
        .any(|window| window == ["isnt", "legendary"] || window == ["isn't", "legendary"])
        || modifier_words
            .windows(3)
            .any(|window| window == ["is", "not", "legendary"])
    {
        removed_supertypes.push(Supertype::Legendary);
    }

    if let Some((power, toughness)) = modifier_words
        .iter()
        .find_map(|word| parse_unsigned_pt_word(word))
    {
        set_base_power_toughness = Some((power, toughness));
    }

    let has_grant_verb = modifier_words.contains(&"has")
        || modifier_words.contains(&"have")
        || modifier_words.contains(&"gain")
        || modifier_words.contains(&"gains");
    let has_modifier_keyword = |keyword: &str| {
        modifier_words
            .windows(2)
            .any(|window| window == ["with", keyword])
            || (has_grant_verb && modifier_words.contains(&keyword))
    };
    if has_modifier_keyword("flying") {
        granted_abilities.push(StaticAbility::flying());
    }
    if has_modifier_keyword("trample") {
        granted_abilities.push(StaticAbility::trample());
    }
    if let Some(idx) = modifier_words
        .windows(6)
        .position(|window| window == ["this", "token", "gets", "+1/+1", "for", "each"])
        .or_else(|| {
            modifier_words
                .windows(6)
                .position(|window| window == ["this", "creature", "gets", "+1/+1", "for", "each"])
        })
    {
        let mut tail = modifier_words.get(idx + 6..).unwrap_or_default();
        while tail
            .first()
            .is_some_and(|word| is_article(word) || matches!(*word, "a" | "an" | "the"))
        {
            tail = &tail[1..];
        }
        if let Some(subtype_word) = tail.first().copied() {
            let subtype = parse_subtype_word(subtype_word)
                .or_else(|| subtype_word.strip_suffix('s').and_then(parse_subtype_word));
            let you_control = tail.windows(2).any(|window| window == ["you", "control"]);
            if let Some(subtype) = subtype
                && you_control
            {
                let mut filter = ObjectFilter::default();
                filter.zone = Some(Zone::Battlefield);
                filter.controller = Some(PlayerFilter::You);
                filter.subtypes = vec![subtype];
                let count = AnthemCountExpression::MatchingFilter(filter);
                let anthem = Anthem::for_source(0, 0).with_values(
                    AnthemValue::scaled(1, count.clone()),
                    AnthemValue::scaled(1, count),
                );
                granted_abilities.push(StaticAbility::new(anthem));
            }
        }
    }

    let addition_idx = modifier_words.windows(6).position(|window| {
        window == ["in", "addition", "to", "its", "other", "types"]
            || window == ["in", "addition", "to", "their", "other", "types"]
    });
    if let Some(addition_idx) = addition_idx {
        let descriptor_words = &modifier_words[..addition_idx];
        for word in descriptor_words {
            if let Some(card_type) = parse_card_type(word)
                && !added_card_types.contains(&card_type)
            {
                added_card_types.push(card_type);
            }
            if let Some(subtype) = parse_subtype_word(word)
                .or_else(|| word.strip_suffix('s').and_then(parse_subtype_word))
                && !added_subtypes.contains(&subtype)
            {
                added_subtypes.push(subtype);
            }
        }
    } else {
        let starts_with_identity_clause = modifier_words.starts_with(&["its"])
            || modifier_words.starts_with(&["it", "is"])
            || modifier_words.starts_with(&["theyre"])
            || modifier_words.starts_with(&["they", "are"]);
        if starts_with_identity_clause {
            let descriptor_end = modifier_words
                .iter()
                .position(|word| matches!(*word, "with" | "has" | "have" | "gain" | "gains"))
                .unwrap_or(modifier_words.len());
            let descriptor_words = &modifier_words[..descriptor_end];
            let mut colors = ColorSet::new();
            let mut card_types = Vec::new();
            let mut subtypes = Vec::new();
            for word in descriptor_words {
                if is_article(word)
                    || matches!(*word, "its" | "it" | "is" | "they" | "are")
                    || looks_like_pt_word(word)
                {
                    continue;
                }
                if let Some(color) = parse_color(word) {
                    colors = colors.union(color);
                }
                if let Some(card_type) = parse_card_type(word)
                    && !card_types.contains(&card_type)
                {
                    card_types.push(card_type);
                }
                if let Some(subtype) = parse_subtype_word(word)
                    .or_else(|| word.strip_suffix('s').and_then(parse_subtype_word))
                    && !subtypes.contains(&subtype)
                {
                    subtypes.push(subtype);
                }
            }
            if !colors.is_empty() {
                set_colors = Some(colors);
            }
            if !card_types.is_empty() {
                set_card_types = Some(card_types);
            }
            if !subtypes.is_empty() {
                set_subtypes = Some(subtypes);
            }
        }
    }

    (
        set_colors,
        set_card_types,
        set_subtypes,
        added_card_types,
        added_subtypes,
        removed_supertypes,
        set_base_power_toughness,
        granted_abilities,
    )
}

fn parse_next_end_step_token_delay_flags(tail_words: &[&str]) -> (bool, bool) {
    let has_beginning_of_end_step = tail_words
        .windows(6)
        .any(|window| window == ["beginning", "of", "the", "next", "end", "step"])
        || tail_words
            .windows(5)
            .any(|window| window == ["beginning", "of", "next", "end", "step"])
        || tail_words
            .windows(5)
            .any(|window| window == ["beginning", "of", "the", "end", "step"])
        || tail_words
            .windows(4)
            .any(|window| window == ["beginning", "of", "end", "step"]);
    if !has_beginning_of_end_step {
        return (false, false);
    }

    let has_sacrifice_reference = tail_words.contains(&"sacrifice")
        && (tail_words.contains(&"token")
            || tail_words.contains(&"tokens")
            || tail_words.contains(&"permanent")
            || tail_words.contains(&"permanents")
            || tail_words.contains(&"it")
            || tail_words.contains(&"them"));
    let has_exile_reference = tail_words.contains(&"exile")
        && (tail_words.contains(&"token")
            || tail_words.contains(&"tokens")
            || tail_words.contains(&"permanent")
            || tail_words.contains(&"permanents")
            || tail_words.contains(&"it")
            || tail_words.contains(&"them"));

    (has_sacrifice_reference, has_exile_reference)
}

fn trailing_create_at_next_end_step_clause(tail_words: &[&str]) -> Option<(usize, PlayerFilter)> {
    let suffixes: &[(&[&str], PlayerFilter)] = &[
        (
            &[
                "at",
                "the",
                "beginning",
                "of",
                "your",
                "next",
                "end",
                "step",
            ],
            PlayerFilter::You,
        ),
        (
            &["at", "the", "beginning", "of", "the", "next", "end", "step"],
            PlayerFilter::Any,
        ),
        (
            &["at", "the", "beginning", "of", "next", "end", "step"],
            PlayerFilter::Any,
        ),
        (
            &["at", "the", "beginning", "of", "the", "end", "step"],
            PlayerFilter::Any,
        ),
        (
            &["at", "the", "beginning", "of", "end", "step"],
            PlayerFilter::Any,
        ),
    ];

    for (suffix, player) in suffixes {
        if tail_words.len() < suffix.len() {
            continue;
        }
        let start = tail_words.len() - suffix.len();
        if tail_words[start..] != **suffix {
            continue;
        }
        if tail_words[..start]
            .iter()
            .any(|word| matches!(*word, "when" | "whenever"))
        {
            continue;
        }
        return Some((start, player.clone()));
    }

    None
}

fn split_copy_source_tail_modifiers(source_tokens: &[Token]) -> (Vec<Token>, bool, bool) {
    let mut split_idx: Option<usize> = None;
    for idx in 0..source_tokens.len() {
        if !source_tokens[idx].is_word("and") {
            continue;
        }
        let tail_tokens = trim_commas(&source_tokens[idx + 1..]);
        let tail_words = words(&tail_tokens);
        if tail_words.is_empty() {
            continue;
        }
        let starts_reference = matches!(
            tail_words.first().copied(),
            Some("that" | "it" | "those" | "thats" | "its")
        );
        if !starts_reference {
            continue;
        }
        if !tail_words.contains(&"tapped") && !tail_words.contains(&"attacking") {
            continue;
        }
        split_idx = Some(idx);
        break;
    }

    let Some(split_idx) = split_idx else {
        return (source_tokens.to_vec(), false, false);
    };

    let modifier_tokens = trim_commas(&source_tokens[split_idx + 1..]);
    let modifier_words = words(&modifier_tokens);
    let enters_tapped = modifier_words.contains(&"tapped");
    let enters_attacking = modifier_words.contains(&"attacking");
    let source_tokens = trim_commas(&source_tokens[..split_idx]).to_vec();
    (source_tokens, enters_tapped, enters_attacking)
}

fn parse_create(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };
    let clause_words = words(tokens);
    let has_unsupported_dynamic_count = clause_words.starts_with(&["a", "number", "of"])
        || clause_words.starts_with(&["the", "number", "of"]);
    if has_unsupported_dynamic_count {
        return Err(CardTextError::ParseError(format!(
            "unsupported dynamic token count in create clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let mut idx = 0;
    let mut count_value = Value::Fixed(1);
    if tokens.first().is_some_and(|token| token.is_word("that"))
        && tokens.get(1).is_some_and(|token| token.is_word("many"))
    {
        count_value = Value::EventValue(EventValueSpec::Amount);
        idx = 2;
    } else if tokens.first().is_some_and(|token| token.is_word("x")) {
        count_value = Value::X;
        idx = 1;
    } else if let Some((parsed_count, used)) = parse_number(tokens) {
        count_value = Value::Fixed(parsed_count as i32);
        idx = used;
    }

    if tokens
        .get(idx)
        .is_some_and(|token| token.is_word("a") || token.is_word("an"))
    {
        idx += 1;
    }

    let remaining_words = words(&tokens[idx..]);
    let token_idx = remaining_words
        .iter()
        .position(|word| *word == "token" || *word == "tokens")
        .ok_or_else(|| CardTextError::ParseError("create clause missing token".to_string()))?;

    let mut name_words: Vec<&str> = remaining_words[..token_idx]
        .iter()
        .copied()
        .filter(|word| !is_article(word))
        .collect();
    let mut tail_tokens = tokens[idx + token_idx + 1..].to_vec();
    let mut delayed_create_player = None;
    let initial_tail_words = words(&tail_tokens);
    if let Some((clause_start, player)) =
        trailing_create_at_next_end_step_clause(&initial_tail_words)
    {
        delayed_create_player = Some(player);
        if let Some(cut_idx) = token_index_for_word_index(&tail_tokens, clause_start) {
            tail_tokens.truncate(cut_idx);
        }
    }
    let mut attached_to_target: Option<TargetAst> = None;
    let pre_attach_tail_words = words(&tail_tokens);
    let pre_attach_for_each_idx = pre_attach_tail_words
        .windows(2)
        .position(|window| window == ["for", "each"]);
    if let Some(attached_word_idx) = pre_attach_tail_words
        .iter()
        .position(|word| *word == "attached")
        && pre_attach_tail_words.get(attached_word_idx + 1) == Some(&"to")
        && (pre_attach_for_each_idx.is_none()
            || pre_attach_for_each_idx.is_some_and(|for_each_idx| attached_word_idx < for_each_idx))
        && let Some(attached_token_idx) =
            token_index_for_word_index(&tail_tokens, attached_word_idx)
    {
        let target_tokens = trim_commas(&tail_tokens[attached_token_idx + 2..]);
        if target_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing attachment target in create clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        attached_to_target = Some(parse_target_phrase(&target_tokens)?);
        tail_tokens.truncate(attached_token_idx);
    }
    let tail_words = words(&tail_tokens);
    let with_idx = tail_words.iter().position(|word| *word == "with");
    let raw_for_each_idx = tail_words
        .windows(2)
        .position(|window| window == ["for", "each"]);
    let for_each_idx = raw_for_each_idx.filter(|idx| {
        let prefix_words = &tail_words[..*idx];
        let looks_like_token_rules_text = prefix_words.windows(2).any(|window| {
            matches!(
                window,
                ["it", "has"]
                    | ["it", "gains"]
                    | ["it", "gets"]
                    | ["this", "token"]
                    | ["that", "token"]
            )
        }) || (prefix_words.contains(&"token")
            && (prefix_words.contains(&"has")
                || prefix_words.contains(&"have")
                || prefix_words.contains(&"gets")
                || prefix_words.contains(&"gains")));
        if looks_like_token_rules_text {
            return false;
        }

        let Some(with_idx) = with_idx else {
            return true;
        };
        if with_idx >= *idx {
            return true;
        }
        let between_with_and_for_each = &tail_words[with_idx + 1..*idx];
        let has_rules_text_hint = between_with_and_for_each.iter().any(|word| {
            matches!(
                *word,
                "this"
                    | "that"
                    | "it"
                    | "token"
                    | "tokens"
                    | "gets"
                    | "get"
                    | "gains"
                    | "gain"
                    | "has"
                    | "have"
                    | "when"
                    | "whenever"
                    | "at"
                    | "sacrifice"
                    | "draw"
                    | "add"
                    | "deals"
                    | "deal"
                    | "counter"
                    | "counters"
            )
        });
        !has_rules_text_hint
    });
    let mut for_each_dynamic_count: Option<Value> = None;
    let mut for_each_object_filter: Option<ObjectFilter> = None;
    if let Some(for_each_idx) = for_each_idx {
        let filter_tokens = &tail_tokens[for_each_idx + 2..];
        if filter_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "missing filter after 'for each' in create clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        if let Some(dynamic) = parse_create_for_each_dynamic_count(filter_tokens) {
            for_each_dynamic_count = Some(dynamic);
        } else {
            let filter = parse_object_filter(filter_tokens, false)?;
            for_each_object_filter = Some(filter);
        }
    }
    let resolve_create_count = |references_iterated_object: bool| {
        if let Some(dynamic) = for_each_dynamic_count.clone() {
            return dynamic;
        }
        if let Some(filter) = for_each_object_filter.clone() {
            if references_iterated_object {
                return count_value.clone();
            }
            return Value::Count(filter);
        }
        count_value.clone()
    };
    let wrap_for_each_when_needed = |effect: EffectAst, references_iterated_object: bool| {
        if references_iterated_object && let Some(filter) = for_each_object_filter.clone() {
            EffectAst::ForEachObject {
                filter,
                effects: vec![effect],
            }
        } else {
            effect
        }
    };
    let wrap_delayed_create = |effect: EffectAst| {
        if let Some(player) = delayed_create_player {
            EffectAst::DelayedUntilNextEndStep {
                player,
                effects: vec![effect],
            }
        } else {
            effect
        }
    };
    let mut tapped = false;
    let mut attacking = false;
    let mut modifier_tail_words = tail_words.clone();
    let mut rules_text_range: Option<(usize, usize)> = None;
    if let Some(named_idx) = tail_words.iter().position(|word| *word == "named") {
        let range_end = for_each_idx.unwrap_or(tail_words.len());
        if named_idx + 1 < range_end {
            let after_named = &tail_words[named_idx + 1..range_end];
            let name_end = after_named
                .iter()
                .position(|word| matches!(*word, "with" | "that" | "which" | "thats"))
                .map(|offset| named_idx + 1 + offset)
                .unwrap_or(range_end);
            if named_idx + 1 < name_end {
                name_words.push("named");
                name_words.extend(tail_words[named_idx + 1..name_end].iter().copied());
            }
        }
    }
    name_words.retain(|word| {
        if *word == "tapped" {
            tapped = true;
            return false;
        }
        if *word == "attacking" {
            attacking = true;
            return false;
        }
        true
    });
    let name_words_primary_len = name_words.len();
    if name_words.is_empty() {
        if tail_words
            .iter()
            .any(|word| *word == "copy" || *word == "copies")
        {
            let (
                set_colors,
                set_card_types,
                set_subtypes,
                added_card_types,
                added_subtypes,
                removed_supertypes,
                set_base_power_toughness,
                granted_abilities,
            ) = parse_copy_modifiers_from_tail(&tail_words);
            let half_pt = tail_words.contains(&"half")
                && tail_words.contains(&"power")
                && tail_words.contains(&"toughness");
            let has_haste = tail_words.windows(2).any(|window| {
                matches!(
                    window,
                    ["has", "haste"] | ["gain", "haste"] | ["gains", "haste"]
                )
            }) || tail_words.contains(&"haste");
            let mut enters_tapped = false;
            let mut enters_attacking = false;
            let (sacrifice_at_next_end_step, exile_at_next_end_step) =
                parse_next_end_step_token_delay_flags(&tail_words);
            if let Some(of_idx) = tail_tokens.iter().position(|token| token.is_word("of")) {
                let source_tokens = &tail_tokens[of_idx + 1..];
                let source_end = source_tokens
                    .iter()
                    .position(|token| matches!(token, Token::Comma(_)) || token.is_word("except"))
                    .unwrap_or(source_tokens.len());
                let source_tokens = &source_tokens[..source_end];
                let (source_tokens, parsed_tapped, parsed_attacking) =
                    split_copy_source_tail_modifiers(source_tokens);
                enters_tapped = parsed_tapped;
                enters_attacking = parsed_attacking;
                if !source_tokens.is_empty() {
                    let source = parse_target_phrase(&source_tokens)?;
                    let references_iterated_object = target_references_it(&source);
                    let create = EffectAst::CreateTokenCopyFromSource {
                        source,
                        count: resolve_create_count(references_iterated_object),
                        player,
                        enters_tapped,
                        enters_attacking,
                        half_power_toughness_round_up: half_pt,
                        has_haste,
                        exile_at_end_of_combat: false,
                        sacrifice_at_next_end_step,
                        exile_at_next_end_step,
                        set_colors,
                        set_card_types,
                        set_subtypes,
                        added_card_types,
                        added_subtypes,
                        removed_supertypes,
                        set_base_power_toughness,
                        granted_abilities,
                    };
                    return Ok(wrap_delayed_create(wrap_for_each_when_needed(
                        create,
                        references_iterated_object,
                    )));
                }
            }
            let references_iterated_object = true;
            let create = EffectAst::CreateTokenCopy {
                object: ObjectRefAst::It,
                count: resolve_create_count(references_iterated_object),
                player,
                enters_tapped,
                enters_attacking,
                half_power_toughness_round_up: half_pt,
                has_haste,
                exile_at_end_of_combat: false,
                sacrifice_at_next_end_step,
                exile_at_next_end_step,
                set_colors,
                set_card_types,
                set_subtypes,
                added_card_types,
                added_subtypes,
                removed_supertypes,
                set_base_power_toughness,
                granted_abilities,
            };
            return Ok(wrap_delayed_create(wrap_for_each_when_needed(
                create,
                references_iterated_object,
            )));
        }
        return Err(CardTextError::ParseError(
            "create clause missing token name".to_string(),
        ));
    }
    if let Some(with_idx) = tail_words.iter().position(|word| *word == "with") {
        let with_tail_end = for_each_idx.unwrap_or(tail_words.len());
        if with_idx + 1 < with_tail_end {
            let with_words = &tail_words[with_idx + 1..with_tail_end];
            let rules_text_start = with_words.iter().position(|word| {
                matches!(
                    *word,
                    "when"
                        | "whenever"
                        | "if"
                        | "t"
                        | "this"
                        | "that"
                        | "it"
                        | "those"
                        | "sacrifice"
                        | "add"
                        | "draw"
                        | "deals"
                        | "deal"
                )
            });
            let mut include_end = rules_text_start.unwrap_or(with_words.len());
            if include_end > 0
                && let Some(named_pos) = with_words[..include_end]
                    .iter()
                    .position(|word| *word == "named")
            {
                include_end = named_pos;
            }
            let preserve_rules_tail = rules_text_start
                .is_some_and(|start| start < with_words.len())
                && with_words[include_end..].iter().any(|word| {
                    matches!(
                        *word,
                        "when"
                            | "whenever"
                            | "at"
                            | "sacrifice"
                            | "return"
                            | "counter"
                            | "draw"
                            | "add"
                            | "deals"
                            | "deal"
                            | "gets"
                            | "gain"
                            | "gains"
                            | "cant"
                            | "can"
                            | "block"
                    )
                });
            if preserve_rules_tail {
                let start = with_idx + 1 + include_end;
                if start < with_tail_end {
                    rules_text_range = Some((start, with_tail_end));
                }
            }
            if include_end > 0 {
                name_words.extend(with_words[..include_end].iter().copied());
                if preserve_rules_tail {
                    // Keep quoted token rules text tails so token lowering can
                    // reconstruct granted abilities instead of dropping them.
                    name_words.extend(with_words[include_end..].iter().copied());
                }
            } else {
                // Preserve quoted token rules text so token compilation can
                // attach the ability to the created token definition.
                name_words.extend(with_words.iter().copied());
            }
        }
    }
    if let Some(pt_idx) = name_words.iter().position(|word| looks_like_pt_word(word))
        && pt_idx > 0
        && pt_idx < name_words_primary_len
    {
        let prefix_words = &name_words[..pt_idx];
        let keep_prefix = prefix_words.contains(&"legendary")
            || prefix_words
                .first()
                .is_some_and(|word| is_probable_token_name_word(word));
        if !keep_prefix {
            name_words = name_words[pt_idx..].to_vec();
        }
    }
    let name = normalize_token_name(&name_words);

    if let Some((start, end)) = rules_text_range {
        if start < end && end <= modifier_tail_words.len() {
            modifier_tail_words = modifier_tail_words[..start]
                .iter()
                .chain(modifier_tail_words[end..].iter())
                .copied()
                .collect();
        }
    }

    tapped |= modifier_tail_words.contains(&"tapped");
    attacking |= modifier_tail_words.contains(&"attacking");
    let (sacrifice_at_next_end_step, exile_at_next_end_step) =
        parse_next_end_step_token_delay_flags(&modifier_tail_words);
    let references_iterated_object = attached_to_target
        .as_ref()
        .is_some_and(target_references_it);
    let create = EffectAst::CreateTokenWithMods {
        name,
        count: resolve_create_count(references_iterated_object),
        player,
        attached_to: attached_to_target,
        tapped,
        attacking,
        exile_at_end_of_combat: false,
        sacrifice_at_end_of_combat: false,
        sacrifice_at_next_end_step,
        exile_at_next_end_step,
    };
    Ok(wrap_delayed_create(wrap_for_each_when_needed(
        create,
        references_iterated_object,
    )))
}

fn parse_create_for_each_dynamic_count(tokens: &[Token]) -> Option<Value> {
    let clause_words = words(tokens);
    if clause_words.starts_with(&["creature", "that", "died", "this", "turn"])
        || clause_words.starts_with(&["creatures", "that", "died", "this", "turn"])
    {
        return Some(Value::CreaturesDiedThisTurn);
    }
    if (clause_words.contains(&"spell") || clause_words.contains(&"spells"))
        && (clause_words.contains(&"cast") || clause_words.contains(&"casts"))
        && clause_words.contains(&"turn")
    {
        let player = if clause_words
            .iter()
            .any(|word| matches!(*word, "you" | "your" | "youve"))
        {
            PlayerFilter::You
        } else if clause_words
            .iter()
            .any(|word| matches!(*word, "opponent" | "opponents"))
        {
            PlayerFilter::Opponent
        } else {
            PlayerFilter::Any
        };

        let other_than_first = clause_words
            .windows(4)
            .any(|window| window == ["other", "than", "the", "first"]);
        if other_than_first {
            return Some(Value::Add(
                Box::new(Value::SpellsCastThisTurn(player)),
                Box::new(Value::Fixed(-1)),
            ));
        }
        if clause_words.contains(&"this") && clause_words.contains(&"turn") {
            return Some(Value::SpellsCastThisTurn(player));
        }
    }
    if clause_words.starts_with(&[
        "color", "of", "mana", "spent", "to", "cast", "this", "spell",
    ]) || clause_words.starts_with(&[
        "colors", "of", "mana", "spent", "to", "cast", "this", "spell",
    ]) || clause_words
        .starts_with(&["color", "of", "mana", "used", "to", "cast", "this", "spell"])
        || clause_words.starts_with(&[
            "colors", "of", "mana", "used", "to", "cast", "this", "spell",
        ])
    {
        return Some(Value::ColorsOfManaSpentToCastThisSpell);
    }
    if clause_words.starts_with(&["basic", "land", "type", "among", "lands", "you", "control"])
        || clause_words.starts_with(&["basic", "land", "types", "among", "lands", "you", "control"])
        || clause_words.starts_with(&[
            "basic", "land", "type", "among", "the", "lands", "you", "control",
        ])
        || clause_words.starts_with(&[
            "basic", "land", "types", "among", "the", "lands", "you", "control",
        ])
    {
        return Some(Value::BasicLandTypesAmong(
            ObjectFilter::land().you_control(),
        ));
    }
    None
}

fn normalize_token_name(words: &[&str]) -> String {
    words.join(" ")
}

fn parse_investigate(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    if tokens.is_empty() {
        return Ok(EffectAst::Investigate {
            count: Value::Fixed(1),
        });
    }

    let (count, used) = if let Some(first) = tokens.first().and_then(Token::as_word) {
        match first {
            "once" => (Value::Fixed(1), 1),
            "twice" => (Value::Fixed(2), 1),
            _ => parse_value(tokens).ok_or_else(|| {
                CardTextError::ParseError(format!(
                    "missing investigate count (clause: '{}')",
                    words(tokens).join(" ")
                ))
            })?,
        }
    } else {
        return Err(CardTextError::ParseError(format!(
            "missing investigate count (clause: '{}')",
            words(tokens).join(" ")
        )));
    };

    let trailing = trim_commas(&tokens[used..]);
    let trailing_words = words(&trailing);
    let trailing_ok = trailing_words.is_empty()
        || trailing_words.as_slice() == ["time"]
        || trailing_words.as_slice() == ["times"];
    if !trailing_ok {
        return Err(CardTextError::ParseError(format!(
            "unsupported trailing investigate clause (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    Ok(EffectAst::Investigate { count })
}

#[cfg(test)]
mod parse_parsing_tests {
    use super::*;

    #[test]
    fn parse_investigate_defaults_to_one() {
        let ast = parse_investigate(&[]).expect("parse investigate");
        assert!(matches!(
            ast,
            EffectAst::Investigate {
                count: Value::Fixed(1)
            }
        ));
    }

    #[test]
    fn parse_investigate_twice() {
        let tokens = tokenize_line("twice", 0);
        let ast = parse_investigate(&tokens).expect("parse investigate twice");
        assert!(matches!(
            ast,
            EffectAst::Investigate {
                count: Value::Fixed(2)
            }
        ));
    }

    #[test]
    fn parse_investigate_n_times() {
        let tokens = tokenize_line("three times", 0);
        let ast = parse_investigate(&tokens).expect("parse investigate three times");
        assert!(matches!(
            ast,
            EffectAst::Investigate {
                count: Value::Fixed(3)
            }
        ));
    }

    #[test]
    fn parse_look_top_x_cards_of_library() {
        let tokens = tokenize_line("the top X cards of your library", 0);
        let ast = parse_look(&tokens, None).expect("parse look with X count");
        assert!(matches!(
            ast,
            EffectAst::LookAtTopCards {
                player: PlayerAst::You,
                count: Value::X,
                ..
            }
        ));
    }

    #[test]
    fn parse_attached_prevent_all_damage_dealt_by_enchanted_creature() {
        let tokens = tokenize_line(
            "Prevent all damage that would be dealt by enchanted creature.",
            0,
        );
        let abilities = parse_static_ability_line(&tokens)
            .expect("parse static ability line")
            .expect("expected static ability");
        assert_eq!(abilities.len(), 1);
        assert_eq!(abilities[0].id(), StaticAbilityId::AttachedAbilityGrant);
    }

    #[test]
    fn parse_prevent_damage_to_source_remove_counter_static_line() {
        let tokens = tokenize_line(
            "If damage would be dealt to this creature, prevent that damage. Remove a +1/+1 counter from this creature.",
            0,
        );
        let abilities = parse_static_ability_line(&tokens)
            .expect("parse static ability line")
            .expect("expected static ability");
        assert!(abilities
            .iter()
            .any(|ability| ability.id() == StaticAbilityId::PreventDamageToSelfRemoveCounter));
    }

    #[test]
    fn parse_prevent_all_damage_to_source_by_creatures_static_line() {
        let tokens = tokenize_line(
            "Prevent all damage that would be dealt to this creature by creatures.",
            0,
        );
        let abilities = parse_static_ability_line(&tokens)
            .expect("parse static ability line")
            .expect("expected static ability");
        assert!(abilities
            .iter()
            .any(|ability| ability.id() == StaticAbilityId::PreventAllDamageToSelfByCreatures));
    }

    #[test]
    fn parse_line_prevent_all_damage_to_source_by_creatures_prefers_static() {
        let parsed = parse_line(
            "Prevent all damage that would be dealt to this creature by creatures.",
            0,
        )
        .expect("parse line");
        let ability = match parsed {
            LineAst::StaticAbility(ability) => ability,
            LineAst::StaticAbilities(mut abilities) if abilities.len() == 1 => abilities
                .pop()
                .expect("single static ability"),
            other => panic!("expected static ability parse, got {other:?}"),
        };
        assert_eq!(ability.id(), StaticAbilityId::PreventAllDamageToSelfByCreatures);
    }

    #[test]
    fn parse_line_prevent_damage_to_source_remove_counter_prefers_static() {
        let line =
            "If damage would be dealt to this creature, prevent that damage. Remove a +1/+1 counter from this creature.";
        let parsed = parse_line(line, 0).expect("parse line");
        let ability = match parsed {
            LineAst::StaticAbility(ability) => ability,
            LineAst::StaticAbilities(mut abilities) if abilities.len() == 1 => abilities
                .pop()
                .expect("single static ability"),
            other => panic!("expected static ability parse, got {other:?}"),
        };
        assert_eq!(ability.id(), StaticAbilityId::PreventDamageToSelfRemoveCounter);
    }

    #[test]
    fn parse_keyword_for_mirrodin_line() {
        let tokens = tokenize_line("For Mirrodin!", 0);
        let actions = parse_ability_line(&tokens).expect("expected keyword actions");
        assert!(actions
            .iter()
            .any(|action| matches!(action, KeywordAction::ForMirrodin)));
    }

    #[test]
    fn parse_keyword_living_weapon_line() {
        let tokens = tokenize_line("Living weapon", 0);
        let actions = parse_ability_line(&tokens).expect("expected keyword actions");
        assert!(actions
            .iter()
            .any(|action| matches!(action, KeywordAction::LivingWeapon)));
    }

    #[test]
    fn parse_clash_clause() {
        let tokens = tokenize_line("Clash with an opponent.", 0);
        let effects = parse_effect_sentence(&tokens).expect("parse effect sentence");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::Clash {
                opponent: ClashOpponentAst::Opponent
            }
        )));
    }

    #[test]
    fn parse_clash_with_defending_player_clause() {
        let tokens = tokenize_line("Clash with defending player.", 0);
        let effects = parse_effect_sentence(&tokens).expect("parse effect sentence");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::Clash {
                opponent: ClashOpponentAst::DefendingPlayer
            }
        )));
    }

    #[test]
    fn parse_clash_then_return_clause() {
        let tokens = tokenize_line(
            "Clash with an opponent, then return target creature to its owner's hand.",
            0,
        );
        let effects = parse_effect_sentence(&tokens).expect("parse effect sentence");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::Clash {
                opponent: ClashOpponentAst::Opponent
            }
        )));
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, EffectAst::ReturnToHand { .. })));
    }

    #[test]
    fn parse_soulbond_shared_power_toughness_line() {
        let tokens = tokenize_line(
            "As long as this creature is paired with another creature, each of those creatures gets +2/+2.",
            0,
        );
        let abilities = parse_static_ability_line(&tokens)
            .expect("parse static ability line")
            .expect("expected static abilities");
        assert_eq!(abilities.len(), 1);
        assert!(abilities[0]
            .display()
            .contains("paired with another creature"));
        assert!(abilities[0].display().contains("+2/+2"));
    }

    #[test]
    fn parse_soulbond_shared_keyword_line() {
        let tokens = tokenize_line(
            "As long as this creature is paired with another creature, both creatures have flying.",
            0,
        );
        let abilities = parse_static_ability_line(&tokens)
            .expect("parse static ability line")
            .expect("expected static abilities");
        assert_eq!(abilities.len(), 1);
        assert!(abilities[0]
            .display()
            .contains("both creatures have Flying"));
    }

    #[test]
    fn parse_if_you_win_as_if_result_predicate() {
        let tokens = tokenize_line("If you win, put a +1/+1 counter on this creature.", 0);
        let effects = parse_effect_sentence(&tokens).expect("parse effect sentence");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::IfResult {
                predicate: IfResultPredicate::Did,
                ..
            }
        )));
    }

    #[test]
    fn parse_if_that_spell_is_countered_this_way_as_if_result_predicate() {
        let tokens = tokenize_line(
            "If that spell is countered this way, exile it instead of putting it into its owners graveyard.",
            0,
        );
        let effects = parse_effect_sentence(&tokens).expect("parse countered-this-way predicate");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::IfResult {
                predicate: IfResultPredicate::Did,
                ..
            }
        )));
    }

    #[test]
    fn parse_predicate_that_player_has_cards_in_hand_or_more() {
        let tokens = tokenize_line("that player has seven or more cards in hand", 0);
        let predicate = parse_predicate(&tokens).expect("parse cards-in-hand predicate");
        assert!(matches!(
            predicate,
            PredicateAst::PlayerCardsInHandOrMore {
                player: PlayerAst::That,
                count: 7
            }
        ));
    }

    #[test]
    fn parse_predicate_that_player_has_cards_in_hand_or_fewer() {
        let tokens = tokenize_line("that player has two or fewer cards in hand", 0);
        let predicate = parse_predicate(&tokens).expect("parse cards-in-hand predicate");
        assert!(matches!(
            predicate,
            PredicateAst::PlayerCardsInHandOrFewer {
                player: PlayerAst::That,
                count: 2
            }
        ));
    }

    #[test]
    fn parse_predicate_creature_died_this_turn() {
        let tokens = tokenize_line("a creature died this turn", 0);
        let predicate = parse_predicate(&tokens).expect("parse creature-died predicate");
        assert!(matches!(predicate, PredicateAst::CreatureDiedThisTurn));
    }

    #[test]
    fn parse_predicate_its_your_turn() {
        let tokens = tokenize_line("its your turn", 0);
        let predicate = parse_predicate(&tokens).expect("parse your-turn predicate");
        assert!(matches!(predicate, PredicateAst::YourTurn));
    }

    #[test]
    fn parse_predicate_cards_in_your_graveyard_threshold() {
        let tokens = tokenize_line("there are seven or more cards in your graveyard", 0);
        let predicate = parse_predicate(&tokens).expect("parse graveyard-threshold predicate");
        assert!(matches!(
            predicate,
            PredicateAst::PlayerControlsAtLeast {
                player: PlayerAst::You,
                filter,
                count: 7
            } if filter.zone == Some(Zone::Graveyard)
        ));
    }

    #[test]
    fn parse_predicate_instant_or_sorcery_cards_in_graveyard_threshold() {
        let tokens = tokenize_line(
            "there are two or more instant and or sorcery cards in your graveyard",
            0,
        );
        let predicate = parse_predicate(&tokens).expect("parse instants-or-sorceries threshold");
        assert!(matches!(
            predicate,
            PredicateAst::PlayerControlsAtLeast {
                player: PlayerAst::You,
                filter,
                count: 2
            } if filter.zone == Some(Zone::Graveyard)
                && filter.card_types.contains(&CardType::Instant)
                && filter.card_types.contains(&CardType::Sorcery)
        ));
    }

    #[test]
    fn parse_predicate_card_types_among_cards_in_graveyard_threshold() {
        let tokens = tokenize_line(
            "there are four or more card types among cards in your graveyard",
            0,
        );
        let predicate = parse_predicate(&tokens).expect("parse delirium predicate");
        assert!(matches!(
            predicate,
            PredicateAst::PlayerHasCardTypesInGraveyardOrMore {
                player: PlayerAst::You,
                count: 4
            }
        ));
    }

    #[test]
    fn parse_if_its_your_turn_sentence_clause() {
        crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Fated Predicate Parse Probe",
        )
        .parse_text(
            "This spell deals 5 damage to target creature.\nIf it's your turn, scry 2.",
        )
        .expect("parse if-its-your-turn conditional clause");
    }

    #[test]
    fn parse_threshold_cards_in_graveyard_clause() {
        crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Threshold Predicate Parse Probe",
        )
        .parse_text("If there are seven or more cards in your graveyard, creatures can't block this turn.")
        .expect("parse threshold-style graveyard card count predicate");
    }

    #[test]
    fn parse_choose_target_creature_prelude_sentence() {
        let tokens = tokenize_line("Choose target creature. It gets +2/+2 until end of turn.", 0);
        let effects =
            parse_effect_sentences(&tokens).expect("parse choose-target prelude sentence");
        assert!(matches!(
            effects.first(),
            Some(EffectAst::TargetOnly {
                target: TargetAst::Object(_, _, _)
            })
        ));
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, EffectAst::Pump { .. })));
    }

    #[test]
    fn parse_choose_target_opponent_prelude_sentence() {
        let tokens = tokenize_line("Choose target opponent. That player discards a card.", 0);
        let effects =
            parse_effect_sentences(&tokens).expect("parse choose-target-opponent prelude");
        assert!(matches!(
            effects.first(),
            Some(EffectAst::TargetOnly {
                target: TargetAst::Player(_, _)
            })
        ));
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, EffectAst::Discard { .. })));
    }

    #[test]
    fn parse_spells_cost_modifier_colored_increase() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Mana Cost Increase Parse Probe",
        )
        .parse_text("Black spells you cast cost {B} more to cast.")
        .expect("parse colored cost increase");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if static_ability
                .cost_increase_mana_cost()
                .is_some_and(|modifier| modifier.increase.to_oracle() == "{B}")
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected colored mana-symbol cost increase in parsed static abilities"
        );
    }

    #[test]
    fn parse_spells_cost_modifier_multicolor_increase() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Multicolor Cost Reduction Parse Probe",
        )
        .parse_text("Cleric spells you cast cost {W}{B} less to cast.")
        .expect("parse multicolor cost reduction");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if static_ability
                .cost_reduction_mana_cost()
                .is_some_and(|modifier| modifier.reduction.to_oracle() == "{W}{B}")
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected multicolor mana-symbol cost reduction in parsed static abilities"
        );
    }

    #[test]
    fn parse_spells_cost_modifier_with_during_other_turns_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Naiad Condition Parse Probe",
        )
        .parse_text("During turns other than yours, spells you cast cost {1} less to cast.")
        .expect("parse turn-conditioned cost reduction");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.cost_reduction()
                && matches!(
                    (&modifier.reduction, &modifier.condition),
                    (
                        Value::Fixed(1),
                        Some(crate::ConditionExpr::Not(inner))
                    ) if matches!(inner.as_ref(), crate::ConditionExpr::YourTurn)
                )
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected turn-conditioned generic cost reduction for spells you cast"
        );
    }

    #[test]
    fn parse_spells_cost_modifier_with_as_long_as_tapped_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Centaur Omenreader Parse Probe",
        )
        .parse_text("As long as this creature is tapped, creature spells you cast cost {2} less to cast.")
        .expect("parse tapped-conditioned cost reduction");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.cost_reduction()
                && modifier.reduction == Value::Fixed(2)
                && matches!(
                    modifier.condition,
                    Some(crate::ConditionExpr::SourceIsTapped)
                )
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected source-tapped condition on creature spell cost reduction"
        );
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_during_your_turn_and_mixed_mana() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Discontinuity Parse Probe",
        )
        .parse_text(
            "During your turn, this spell costs {2}{U}{U} less to cast.\nDraw a card.",
        )
        .expect("parse this-spell mixed-mana reduction with turn condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction_mana_cost()
                && modifier.reduction.to_oracle() == "{2}{U}{U}"
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::YourTurn
                )
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected this-spell mixed-mana reduction with during-your-turn condition"
        );
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_opponent_drew_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Even the Score Parse Probe",
        )
        .parse_text(
            "This spell costs {U}{U}{U} less to cast if an opponent has drawn four or more cards this turn.\nDraw X cards.",
        )
        .expect("parse this-spell colored reduction with draw condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction_mana_cost()
                && modifier.reduction.to_oracle() == "{U}{U}{U}"
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::OpponentDrewCardsThisTurnOrMore(4)
                )
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected conditional this-spell colored reduction with opponent-draw condition"
        );
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_opponent_cast_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Ertai's Scorn Parse Probe",
        )
        .parse_text(
            "This spell costs {U} less to cast if an opponent cast two or more spells this turn.\nCounter target spell.",
        )
        .expect("parse this-spell colored reduction with cast condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction_mana_cost()
                && modifier.reduction.to_oracle() == "{U}"
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::OpponentCastSpellsThisTurnOrMore(2)
                )
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected conditional this-spell colored reduction with opponent-cast condition"
        );
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_you_control_condition_expr() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Wizard Discount Parse Probe",
        )
        .parse_text("This spell costs {1} less to cast if you control a wizard.\nDraw a card.")
        .expect("parse this-spell reduction with you-control condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(1)
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::ConditionExpr { .. }
                )
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected this-spell reduction with parsed condition expression"
        );
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_targets_object_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Tapped Target Discount Parse Probe",
        )
        .parse_text(
            "This spell costs {2} less to cast if it targets a tapped creature.\nDestroy target creature.",
        )
        .expect("parse this-spell reduction with target condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(2)
                && let crate::static_abilities::ThisSpellCostCondition::TargetsObject(filter) =
                    &modifier.condition
                && filter.tapped
                && filter.card_types.contains(&CardType::Creature)
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected tapped-creature target condition");
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_graveyard_count_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Graveyard Count Discount Parse Probe",
        )
        .parse_text(
            "This spell costs {3} less to cast if you have nine or more cards in your graveyard.\nDraw a card.",
        )
        .expect("parse this-spell reduction with graveyard-count condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(3)
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::YouHaveCardsInYourGraveyardOrMore(9)
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected graveyard-count condition");
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_creature_attacking_you_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Attack Trap Parse Probe",
        )
        .parse_text(
            "This spell costs {2} less to cast if a creature is attacking you.\nDestroy target attacking creature.",
        )
        .expect("parse this-spell reduction with attacking-you condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(2)
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::CreatureIsAttackingYou
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected attacking-you condition");
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_night_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Night Discount Parse Probe",
        )
        .parse_text(
            "This spell costs {2} less to cast if it's night.\nThis spell deals 3 damage to any target.",
        )
        .expect("parse this-spell reduction with night condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(2)
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::IsNight
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected night condition");
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_sacrificed_artifact_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Artifact Sacrifice Discount Parse Probe",
        )
        .parse_text(
            "This spell costs {3} less to cast if you've sacrificed an artifact this turn.\nThis spell can't be countered.\nThis spell deals 4 damage to target creature.",
        )
        .expect("parse this-spell reduction with artifact-sacrifice condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(3)
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::YouSacrificedArtifactThisTurn
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected artifact-sacrifice condition");
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_creature_left_battlefield_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Creature Left Battlefield Discount Parse Probe",
        )
        .parse_text(
            "This spell costs {2} less to cast if a creature left the battlefield under your control this turn.\nDraw a card.",
        )
        .expect("parse this-spell reduction with creature-left condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(2)
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::CreatureLeftBattlefieldUnderYourControlThisTurn
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected creature-left-battlefield condition");
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_committed_crime_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Crime Discount Parse Probe",
        )
        .parse_text(
            "This spell costs {1} less to cast if you've committed a crime this turn.\nDraw two cards.",
        )
        .expect("parse this-spell reduction with committed-crime condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(1)
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::YouCommittedCrimeThisTurn
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected committed-crime condition");
    }

    #[test]
    fn parse_this_spell_cost_modifier_with_only_named_other_creatures_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Mothrider Condition Parse Probe",
        )
        .parse_text(
            "This spell costs {2} less to cast if you have no other creature cards in hand or if the only other creature cards in your hand are named Mothrider Cavalry.\nFlying\nOther creatures you control get +1/+1.",
        )
        .expect("parse this-spell reduction with named-creatures-in-hand condition");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::Fixed(2)
                && let crate::static_abilities::ThisSpellCostCondition::OnlyCreatureCardsInHandNamed(name) =
                    &modifier.condition
                && name == "mothrider cavalry"
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected named-creatures-in-hand condition");
    }

    #[test]
    fn parse_if_this_spell_costs_x_less_where_difference_condition() {
        let card = crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Starting Life Difference Discount Parse Probe",
        )
        .parse_text(
            "If your life total is less than your starting life total, this spell costs {X} less to cast, where X is the difference.",
        )
        .expect("parse leading-if this-spell X reduction");

        let mut found = false;
        for ability in &card.abilities {
            let crate::ability::AbilityKind::Static(static_ability) = &ability.kind else {
                continue;
            };
            if let Some(modifier) = static_ability.this_spell_cost_reduction()
                && modifier.reduction == Value::X
                && matches!(
                    modifier.condition,
                    crate::static_abilities::ThisSpellCostCondition::LifeTotalLessThanStarting
                )
            {
                found = true;
                break;
            }
        }
        assert!(found, "expected starting-life-difference condition");
    }

    #[test]
    fn parse_object_filter_spell_or_permanent_builds_zone_disjunction() {
        let tokens = tokenize_line("spell or permanent", 0);
        let filter = parse_object_filter(&tokens, false).expect("parse mixed spell/permanent");
        assert_eq!(filter.any_of.len(), 2);
        assert!(
            filter
                .any_of
                .iter()
                .any(|branch| branch.zone == Some(Zone::Stack)),
            "expected stack branch for spell targets"
        );
        assert!(
            filter
                .any_of
                .iter()
                .any(|branch| branch.zone == Some(Zone::Battlefield)),
            "expected battlefield branch for permanent targets"
        );
    }

    #[test]
    fn parse_object_filter_permanent_spell_stays_stack_only() {
        let tokens = tokenize_line("blue permanent spell", 0);
        let filter = parse_object_filter(&tokens, false).expect("parse permanent spell filter");
        assert!(
            filter.any_of.is_empty(),
            "permanent spell should not become a spell/permanent disjunction"
        );
        assert_eq!(filter.zone, Some(Zone::Stack));
        assert!(
            !filter.card_types.is_empty() || !filter.all_card_types.is_empty(),
            "permanent spell filter should preserve permanent card-type restriction"
        );
    }

    #[test]
    fn parse_object_filter_spell_or_nonland_permanent_preserves_nonland_branch() {
        let tokens = tokenize_line("spell or nonland permanent opponent controls", 0);
        let filter =
            parse_object_filter(&tokens, false).expect("parse spell-or-nonland-permanent filter");
        assert_eq!(filter.any_of.len(), 2);
        let battlefield_branch = filter
            .any_of
            .iter()
            .find(|branch| branch.zone == Some(Zone::Battlefield))
            .expect("expected battlefield branch");
        assert!(
            battlefield_branch.excluded_card_types.contains(&CardType::Land),
            "nonland qualifier should stay on battlefield permanent branch"
        );
    }

    #[test]
    fn parse_object_filter_permanents_and_permanent_spells_split_branches() {
        let tokens = tokenize_line("nonland permanents you control and permanent spells you control", 0);
        let filter =
            parse_object_filter(&tokens, false).expect("parse permanents and permanent spells");
        assert_eq!(filter.any_of.len(), 2);
        let stack_branch = filter
            .any_of
            .iter()
            .find(|branch| branch.zone == Some(Zone::Stack))
            .expect("expected stack branch");
        assert!(
            !stack_branch.card_types.is_empty() || !stack_branch.all_card_types.is_empty(),
            "permanent-spell branch should keep permanent type restriction"
        );
    }

    #[test]
    fn parse_object_filter_spell_from_hand_keeps_origin_zone() {
        let tokens = tokenize_line("instant or sorcery spell from your hand", 0);
        let filter = parse_object_filter(&tokens, false).expect("parse spell-origin filter");
        assert_eq!(filter.zone, Some(Zone::Hand));
        assert_eq!(filter.owner, Some(PlayerFilter::You));
    }

    #[test]
    fn parse_object_filter_spell_with_source_linked_exile_reference_stays_on_stack() {
        let tokens = tokenize_line("spell with the same name as a card exiled with this creature", 0);
        let filter =
            parse_object_filter(&tokens, false).expect("parse spell with source-linked exile ref");
        assert_eq!(filter.zone, Some(Zone::Stack));
        assert!(
            filter.tagged_constraints.iter().any(|constraint| {
                constraint.tag.as_str() == crate::tag::SOURCE_EXILED_TAG
            }),
            "expected source-linked exile tagged constraint"
        );
    }

    #[test]
    fn parse_target_phrase_spell_cast_from_graveyard_uses_spell_origin_zone() {
        let tokens = tokenize_line("target spell cast from a graveyard", 0);
        let target = parse_target_phrase(&tokens).expect("parse target spell cast from graveyard");
        let TargetAst::Object(filter, _, _) = target else {
            panic!("expected object target");
        };
        assert_eq!(filter.zone, Some(Zone::Graveyard));
    }

    #[test]
    fn parse_trigger_clause_player_subject_attack_uses_one_or_more() {
        let tokens = tokenize_line("you attack", 0);
        let trigger = parse_trigger_clause(&tokens).expect("parse trigger clause");
        match trigger {
            TriggerSpec::AttacksOneOrMore(filter) => {
                assert_eq!(filter.controller, Some(PlayerFilter::You));
                assert!(filter.card_types.contains(&CardType::Creature));
            }
            other => panic!("expected AttacksOneOrMore trigger, got {other:?}"),
        }
    }

    #[test]
    fn parse_trigger_clause_opponent_attacks_you_uses_one_or_more() {
        let tokens = tokenize_line("an opponent attacks you or a planeswalker you control", 0);
        let trigger = parse_trigger_clause(&tokens).expect("parse trigger clause");
        match trigger {
            TriggerSpec::AttacksYouOrPlaneswalkerYouControlOneOrMore(filter) => {
                assert_eq!(filter.controller, Some(PlayerFilter::Opponent));
                assert!(filter.card_types.contains(&CardType::Creature));
            }
            other => panic!(
                "expected AttacksYouOrPlaneswalkerYouControlOneOrMore trigger, got {other:?}"
            ),
        }
    }

    #[test]
    fn parse_trigger_clause_player_subject_combat_damage_uses_one_or_more() {
        let tokens = tokenize_line("you deal combat damage to a player", 0);
        let trigger = parse_trigger_clause(&tokens).expect("parse trigger clause");
        match trigger {
            TriggerSpec::DealsCombatDamageToPlayerOneOrMore(filter) => {
                assert_eq!(filter.controller, Some(PlayerFilter::You));
                assert!(filter.card_types.contains(&CardType::Creature));
            }
            other => panic!(
                "expected DealsCombatDamageToPlayerOneOrMore trigger, got {other:?}"
            ),
        }
    }

    #[test]
    fn parse_trigger_clause_this_deals_combat_damage_to_creature() {
        let tokens = tokenize_line("this creature deals combat damage to a creature", 0);
        let trigger = parse_trigger_clause(&tokens).expect("parse trigger clause");
        match trigger {
            TriggerSpec::ThisDealsCombatDamageTo(filter) => {
                assert!(filter.card_types.contains(&CardType::Creature));
            }
            other => panic!("expected ThisDealsCombatDamageTo trigger, got {other:?}"),
        }
    }

    #[test]
    fn parse_trigger_clause_filtered_source_deals_combat_damage_to_creature() {
        let tokens = tokenize_line("a sliver deals combat damage to a creature", 0);
        let trigger = parse_trigger_clause(&tokens).expect("parse trigger clause");
        match trigger {
            TriggerSpec::DealsCombatDamageTo { source, target } => {
                assert!(source.card_types.contains(&CardType::Creature));
                assert!(
                    source.description().contains("sliver"),
                    "expected sliver source filter, got {}",
                    source.description()
                );
                assert!(target.card_types.contains(&CardType::Creature));
            }
            other => panic!("expected DealsCombatDamageTo trigger, got {other:?}"),
        }
    }

    #[test]
    fn parse_trigger_clause_combat_damage_to_one_of_your_opponents() {
        let tokens = tokenize_line("a creature deals combat damage to one of your opponents", 0);
        let trigger = parse_trigger_clause(&tokens).expect("parse trigger clause");
        match trigger {
            TriggerSpec::DealsCombatDamageToPlayer(filter) => {
                assert!(filter.card_types.contains(&CardType::Creature));
            }
            other => panic!("expected DealsCombatDamageToPlayer trigger, got {other:?}"),
        }
    }

    #[test]
    fn parse_trigger_clause_this_deals_combat_damage_without_recipient() {
        let tokens = tokenize_line("this creature deals combat damage", 0);
        let trigger = parse_trigger_clause(&tokens).expect("parse trigger clause");
        match trigger {
            TriggerSpec::ThisDealsCombatDamage => {}
            other => panic!("expected ThisDealsCombatDamage trigger, got {other:?}"),
        }
    }

    #[test]
    fn parse_prevent_next_time_damage_sentence_source_of_your_choice_any_target() {
        let tokens = tokenize_line(
            "The next time a source of your choice would deal damage to any target this turn, prevent that damage.",
            0,
        );
        let effects = parse_effect_sentence(&tokens).expect("parse effect sentence");
        assert!(effects.iter().any(|e| matches!(
            e,
            EffectAst::PreventNextTimeDamage {
                source: PreventNextTimeDamageSourceAst::Choice,
                target: PreventNextTimeDamageTargetAst::AnyTarget
            }
        )));
    }

    #[test]
    fn parse_redirect_next_damage_sentence_to_target_creature() {
        let tokens = tokenize_line(
            "The next 1 damage that would be dealt to this creature this turn is dealt to target creature you control instead.",
            0,
        );
        let effects = parse_effect_sentence(&tokens).expect("parse effect sentence");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::RedirectNextDamageFromSourceToTarget {
                amount: Value::Fixed(1),
                ..
            }
        )));
    }

    #[test]
    fn parse_redirect_next_time_source_damage_to_this_creature() {
        let tokens = tokenize_line(
            "The next time a source of your choice would deal damage to target creature this turn, that damage is dealt to this creature instead.",
            0,
        );
        let effects = parse_effect_sentence(&tokens).expect("parse effect sentence");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::RedirectNextTimeDamageToSource {
                source: PreventNextTimeDamageSourceAst::Choice,
                ..
            }
        )));
    }

    #[test]
    fn parse_activated_discard_random_cost_to_effect_cost() {
        let tokens = tokenize_line(
            "{R}, Discard a card at random: This creature gets +3/+0 until end of turn.",
            0,
        );
        let parsed = parse_activated_line(&tokens)
            .expect("parse activated line")
            .expect("expected activated ability");

        let AbilityKind::Activated(activated) = parsed.ability.kind else {
            panic!("expected activated ability");
        };

        let has_random_discard_cost = activated.mana_cost.costs().iter().any(|cost| {
            cost.effect_ref().is_some_and(|effect| {
                effect
                    .downcast_ref::<crate::effects::DiscardEffect>()
                    .is_some_and(|discard| discard.random)
            })
        });
        assert!(
            has_random_discard_cost,
            "expected random discard effect-backed cost"
        );
    }

    #[test]
    fn parse_gain_life_equal_to_sacrificed_creature_toughness_clause() {
        let tokens = tokenize_line("life equal to the sacrificed creature's toughness", 0);
        let effect = parse_gain_life(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse gain life equal to sacrificed creature toughness");
        assert!(matches!(
            effect,
            EffectAst::GainLife {
                amount: Value::ToughnessOf(spec),
                player: PlayerAst::You,
            } if matches!(
                spec.as_ref(),
                ChooseSpec::Tagged(tag) if tag.as_str() == IT_TAG
            )
        ));
    }

    #[test]
    fn parse_gain_life_equal_to_devotion_clause() {
        let tokens = tokenize_line("life equal to your devotion to green", 0);
        let effect = parse_gain_life(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse gain life equal to devotion");
        assert!(matches!(
            effect,
            EffectAst::GainLife {
                amount: Value::Devotion {
                    player: PlayerFilter::You,
                    color: crate::color::Color::Green
                },
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_gain_life_equal_to_life_lost_this_way_clause() {
        let tokens = tokenize_line("life equal to the life lost this way", 0);
        let effect = parse_gain_life(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse gain life equal to life lost this way");
        assert!(matches!(
            effect,
            EffectAst::GainLife {
                amount: Value::EventValue(EventValueSpec::LifeAmount),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_number_of_named_cards_in_graveyards() {
        let tokens = tokenize_line(
            "cards equal to the number of cards named accumulated knowledge in all graveyards",
            0,
        );
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to number-of filter");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::Count(filter),
                player: PlayerAst::You,
            } if filter.zone == Some(Zone::Graveyard)
                && filter.name.as_deref() == Some("accumulated knowledge")
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_greatest_power_among_creatures() {
        let tokens = tokenize_line(
            "cards equal to the greatest power among creatures you control",
            0,
        );
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to aggregate filter");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::GreatestPower(filter),
                player: PlayerAst::You,
            } if filter.controller == Some(PlayerFilter::You)
                && filter.card_types.contains(&CardType::Creature)
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_number_of_hand_plus_one() {
        let tokens = tokenize_line(
            "cards equal to the number of cards in your hand plus one",
            0,
        );
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to number-of filter plus fixed");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::Add(left, right),
                player: PlayerAst::You,
            } if matches!(
                (left.as_ref(), right.as_ref()),
                (Value::Count(filter), Value::Fixed(1))
                    if filter.zone == Some(Zone::Hand)
                        && filter.owner == Some(PlayerFilter::You)
            )
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_that_spells_mana_value() {
        let tokens = tokenize_line("cards equal to that spells mana value", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to tagged mana value");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::ManaValueOf(spec),
                player: PlayerAst::You,
            } if matches!(
                spec.as_ref(),
                ChooseSpec::Tagged(tag) if tag.as_str() == IT_TAG
            )
        ));
    }

    #[test]
    fn parse_draw_another_card_as_fixed_one() {
        let tokens = tokenize_line("another card", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw another card");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::Fixed(1),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_devotion() {
        let tokens = tokenize_line("cards equal to your devotion to red", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to devotion");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::Devotion {
                    player: PlayerFilter::You,
                    color: crate::color::Color::Red,
                },
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_number_of_opponents_you_have() {
        let tokens = tokenize_line("cards equal to the number of opponents you have", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to number of opponents");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::CountPlayers(PlayerFilter::Opponent),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_number_of_oil_counters_on_it() {
        let tokens = tokenize_line("cards equal to the number of oil counters on it", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to counters on source");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::CountersOnSource(CounterType::Oil),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_cards_equal_to_sacrificed_permanent_mana_value() {
        let tokens = tokenize_line(
            "cards equal to the mana value of the sacrificed permanent",
            0,
        );
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw equal to sacrificed permanent mana value");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::ManaValueOf(spec),
                player: PlayerAst::You,
            } if matches!(
                spec.as_ref(),
                ChooseSpec::Tagged(tag) if tag.as_str() == IT_TAG
            )
        ));
    }

    #[test]
    fn parse_draw_as_many_cards_as_discarded_this_way() {
        let tokens = tokenize_line("as many cards as they discarded this way", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw as-many previous-event amount");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::EventValue(EventValueSpec::Amount),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_that_many_cards_plus_one() {
        let tokens = tokenize_line("that many cards plus one", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw that-many cards plus one");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::EventValueOffset(EventValueSpec::Amount, 1),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_three_cards_instead_trailing_clause() {
        let tokens = tokenize_line("three cards instead", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw with trailing instead clause");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::Fixed(3),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_an_additional_card_clause() {
        let tokens = tokenize_line("an additional card", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw with additional card wording");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::Fixed(1),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_two_additional_cards_clause() {
        let tokens = tokenize_line("two additional cards", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw with numeric additional cards wording");
        assert!(matches!(
            effect,
            EffectAst::Draw {
                count: Value::Fixed(2),
                player: PlayerAst::You,
            }
        ));
    }

    #[test]
    fn parse_draw_card_next_turns_upkeep_trailing_clause() {
        let tokens = tokenize_line("a card at the beginning of the next turns upkeep", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw delayed until next turn's upkeep");
        assert!(matches!(
            effect,
            EffectAst::DelayedUntilNextUpkeep {
                player: PlayerAst::Any,
                effects,
            } if matches!(
                effects.as_slice(),
                [EffectAst::Draw {
                    count: Value::Fixed(1),
                    player: PlayerAst::You,
                }]
            )
        ));
    }

    #[test]
    fn parse_draw_card_next_end_step_trailing_clause() {
        let tokens = tokenize_line("a card at the beginning of the next end step", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw delayed until next end step");
        assert!(matches!(
            effect,
            EffectAst::DelayedUntilNextEndStep {
                player: PlayerFilter::Any,
                effects,
            } if matches!(
                effects.as_slice(),
                [EffectAst::Draw {
                    count: Value::Fixed(1),
                    player: PlayerAst::You,
                }]
            )
        ));
    }

    #[test]
    fn parse_draw_card_if_you_have_no_cards_in_hand_trailing_clause() {
        let tokens = tokenize_line("a card if you have no cards in hand", 0);
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw with trailing if predicate");
        assert!(matches!(
            effect,
            EffectAst::Conditional {
                predicate: PredicateAst::YouHaveNoCardsInHand,
                if_true,
                if_false,
            } if if_false.is_empty()
                && matches!(
                    if_true.as_slice(),
                    [EffectAst::Draw {
                        count: Value::Fixed(1),
                        player: PlayerAst::You,
                    }]
                )
        ));
    }

    #[test]
    fn parse_draw_card_unless_target_opponent_action() {
        let tokens = tokenize_line(
            "a card unless target opponent sacrifices a creature of their choice or pays 3 life",
            0,
        );
        let effect = parse_draw(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse draw with trailing unless clause");
        assert!(matches!(
            effect,
            EffectAst::UnlessAction {
                player: PlayerAst::TargetOpponent,
                effects,
                ..
            } if matches!(
                effects.as_slice(),
                [EffectAst::Draw {
                    count: Value::Fixed(1),
                    player: PlayerAst::You,
                }]
                )
        ));
    }

    #[test]
    fn parse_discard_a_red_or_green_card_qualifier() {
        let tokens = tokenize_line("a red or green card", 0);
        let effect = parse_discard(&tokens, Some(SubjectAst::Player(PlayerAst::You)))
            .expect("parse discard with color disjunction qualifier");
        assert!(matches!(
            effect,
            EffectAst::Discard {
                count: Value::Fixed(1),
                player: PlayerAst::You,
                random: false,
                filter: Some(filter),
            } if filter.zone == Some(Zone::Hand)
        ));
    }

    #[test]
    fn parse_surge_of_strength_additional_discard_cost() {
        crate::cards::CardDefinitionBuilder::new(
            crate::ids::CardId::new(),
            "Surge of Strength Parse Probe",
        )
        .parse_text(
            "As an additional cost to cast this spell, discard a red or green card.\nTarget creature gains trample and gets +X/+0 until end of turn, where X is that creature's mana value.",
        )
        .expect("parse surge of strength additional discard cost");
    }

    #[test]
    fn parse_put_counters_that_many_amount() {
        let tokens = tokenize_line("that many +1/+1 counters on this creature", 0);
        let effect = parse_put_counters(&tokens).expect("parse put counters with that-many amount");
        assert!(matches!(
            effect,
            EffectAst::PutCounters {
                counter_type: CounterType::PlusOnePlusOne,
                count: Value::EventValue(EventValueSpec::Amount),
                ..
            }
        ));
    }

    #[test]
    fn parse_put_counters_x_amount() {
        let tokens = tokenize_line("x +1/+1 counters on target creature", 0);
        let effect = parse_put_counters(&tokens).expect("parse put counters with x amount");
        assert!(matches!(
            effect,
            EffectAst::PutCounters {
                counter_type: CounterType::PlusOnePlusOne,
                count: Value::X,
                ..
            }
        ));
    }

    #[test]
    fn parse_put_counters_where_x_replacement() {
        let tokens = tokenize_line(
            "Put X +1/+1 counters on target creature, where X is that creature's power.",
            0,
        );
        let effects = parse_effect_sentence(&tokens).expect("parse put counters where-X sentence");
        assert!(effects.iter().any(|effect| matches!(
            effect,
            EffectAst::PutCounters {
                counter_type: CounterType::PlusOnePlusOne,
                count: Value::PowerOf(_),
                ..
            }
        )));
    }

    #[test]
    fn parse_put_counters_equal_to_devotion_amount() {
        let tokens = tokenize_line(
            "a number of +1/+1 counters on it equal to your devotion to green",
            0,
        );
        let effect =
            parse_put_counters(&tokens).expect("parse put counters with devotion-derived amount");
        assert!(matches!(
            effect,
            EffectAst::PutCounters {
                counter_type: CounterType::PlusOnePlusOne,
                count: Value::Devotion {
                    player: PlayerFilter::You,
                    color: crate::color::Color::Green
                },
                ..
            }
        ));
    }

    #[test]
    fn parse_put_counters_those_counters_moves_all() {
        let tokens = tokenize_line("those counters on target creature you control", 0);
        let effect =
            parse_put_counters(&tokens).expect("parse put those-counters transfer as move-all");
        assert!(matches!(
            effect,
            EffectAst::MoveAllCounters {
                from: TargetAst::Tagged(tag, _),
                ..
            } if tag.as_str() == IT_TAG
        ));
    }
}

fn parse_remove(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    if let Some(from_idx) = tokens.iter().position(|token| token.is_word("from")) {
        let tail_words = words(&tokens[from_idx + 1..]);
        if tail_words == ["combat"] {
            let target_tokens = trim_commas(&tokens[..from_idx]);
            if target_tokens.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "missing remove-from-combat target (clause: '{}')",
                    words(tokens).join(" ")
                )));
            }
            let target = parse_target_phrase(&target_tokens)?;
            return Ok(EffectAst::RemoveFromCombat { target });
        }
    }

    let mut idx = 0;
    let mut up_to = false;
    if tokens.get(idx).is_some_and(|token| token.is_word("up"))
        && tokens.get(idx + 1).is_some_and(|token| token.is_word("to"))
    {
        up_to = true;
        idx += 2;
    }

    let (amount, used) = parse_value(&tokens[idx..]).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing counter removal amount (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;
    idx += used;

    let counter_idx = tokens[idx..]
        .iter()
        .position(|token| token.is_word("counter") || token.is_word("counters"))
        .map(|offset| idx + offset)
        .ok_or_else(|| CardTextError::ParseError("missing counter keyword".to_string()))?;
    let counter_descriptor = trim_commas(&tokens[idx..counter_idx]);
    let counter_type = parse_counter_type_from_tokens(&counter_descriptor);
    if counter_idx >= tokens.len() {
        return Err(CardTextError::ParseError(
            "missing counter keyword".to_string(),
        ));
    }
    idx = counter_idx + 1;

    if tokens.get(idx).is_some_and(|token| token.is_word("from")) {
        idx += 1;
    }

    let target_tokens = trim_commas(&tokens[idx..]);
    if target_tokens
        .first()
        .is_some_and(|token| token.is_word("each") || token.is_word("all"))
    {
        let filter = parse_object_filter(&target_tokens[1..], false)?;
        return Ok(EffectAst::RemoveCountersAll {
            amount,
            filter,
            counter_type,
            up_to,
        });
    }

    let for_each_idx = (0..target_tokens.len().saturating_sub(1))
        .find(|i| target_tokens[*i].is_word("for") && target_tokens[*i + 1].is_word("each"));
    if let Some(for_each_idx) = for_each_idx {
        let base_target_tokens = trim_commas(&target_tokens[..for_each_idx]);
        let count_filter_tokens = trim_commas(&target_tokens[for_each_idx + 2..]);
        if !base_target_tokens.is_empty() && !count_filter_tokens.is_empty() {
            if let (Ok(target), Ok(count_filter)) = (
                parse_target_phrase(&base_target_tokens),
                parse_object_filter(&count_filter_tokens, false),
            ) {
                return Ok(EffectAst::ForEachObject {
                    filter: count_filter,
                    effects: vec![EffectAst::RemoveUpToAnyCounters {
                        amount,
                        target,
                        counter_type,
                        up_to,
                    }],
                });
            }
        }
    }

    let target_tokens = trim_commas(&tokens[idx..]);
    let target = parse_target_phrase(&target_tokens)?;

    Ok(EffectAst::RemoveUpToAnyCounters {
        amount,
        target,
        counter_type,
        up_to,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelayedDestroyTimingAst {
    EndOfCombat,
    NextEndStep,
}

fn parse_delayed_destroy_timing_words(words: &[&str]) -> Option<DelayedDestroyTimingAst> {
    if matches!(
        words,
        ["at", "end", "of", "combat"] | ["at", "the", "end", "of", "combat"]
    ) {
        return Some(DelayedDestroyTimingAst::EndOfCombat);
    }

    if matches!(
        words,
        ["at", "beginning", "of", "next", "end", "step"]
            | ["at", "beginning", "of", "the", "next", "end", "step"]
            | ["at", "the", "beginning", "of", "next", "end", "step"]
            | ["at", "the", "beginning", "of", "the", "next", "end", "step"]
    ) {
        return Some(DelayedDestroyTimingAst::NextEndStep);
    }

    None
}

fn wrap_destroy_with_delayed_timing(
    effect: EffectAst,
    timing: Option<DelayedDestroyTimingAst>,
) -> EffectAst {
    let Some(timing) = timing else {
        return effect;
    };

    match timing {
        DelayedDestroyTimingAst::EndOfCombat => EffectAst::DelayedUntilEndOfCombat {
            effects: vec![effect],
        },
        DelayedDestroyTimingAst::NextEndStep => EffectAst::DelayedUntilNextEndStep {
            player: PlayerFilter::Any,
            effects: vec![effect],
        },
    }
}

fn parse_destroy(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    let original_clause_words = words(tokens);
    let mut delayed_timing = None;
    let mut timing_cut_word_idx = original_clause_words.len();
    for word_idx in 0..original_clause_words.len() {
        if original_clause_words[word_idx] != "at" {
            continue;
        }
        if let Some(timing) = parse_delayed_destroy_timing_words(&original_clause_words[word_idx..])
        {
            delayed_timing = Some(timing);
            timing_cut_word_idx = word_idx;
            break;
        }
    }

    let core_tokens = if timing_cut_word_idx < original_clause_words.len() {
        let token_cutoff =
            token_index_for_word_index(tokens, timing_cut_word_idx).unwrap_or(tokens.len());
        trim_commas(&tokens[..token_cutoff])
    } else {
        trim_commas(tokens)
    };
    let clause_words = words(&core_tokens);
    if clause_words.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing destroy target before delayed timing clause (clause: '{}')",
            original_clause_words.join(" ")
        )));
    }

    if delayed_timing.is_none()
        && (original_clause_words
            .windows(3)
            .any(|window| window == ["end", "of", "combat"])
            || (original_clause_words.contains(&"beginning")
                && original_clause_words.contains(&"end")))
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported delayed destroy timing clause (clause: '{}')",
            original_clause_words.join(" ")
        )));
    }
    if let Some(target) = parse_destroy_combat_history_target(&core_tokens)? {
        return Ok(wrap_destroy_with_delayed_timing(
            EffectAst::Destroy { target },
            delayed_timing,
        ));
    }
    let has_combat_history = (clause_words.contains(&"dealt")
        && clause_words.contains(&"damage")
        && clause_words.contains(&"turn"))
        || clause_words
            .windows(2)
            .any(|window| matches!(window, ["was", "blocked"] | ["was", "blocking"]))
        || clause_words.windows(2).any(|window| {
            matches!(
                window,
                ["blocking", "it"] | ["blocked", "it"] | ["it", "blocked"]
            )
        });
    if has_combat_history {
        return Err(CardTextError::ParseError(format!(
            "unsupported combat-history destroy clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    if matches!(clause_words.first().copied(), Some("all" | "each")) {
        if let Some(attached_idx) = core_tokens.iter().position(|token| token.is_word("attached"))
            && core_tokens
                .get(attached_idx + 1)
                .is_some_and(|token| token.is_word("to"))
            && attached_idx > 1
        {
            let mut filter_tokens = trim_commas(&core_tokens[1..attached_idx]).to_vec();
            while filter_tokens
                .last()
                .and_then(Token::as_word)
                .is_some_and(|word| matches!(word, "that" | "were" | "was" | "is" | "are"))
            {
                filter_tokens.pop();
            }
            let target_tokens = trim_commas(&core_tokens[attached_idx + 2..]);
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
            if !filter_tokens.is_empty()
                && !target_tokens.is_empty()
                && supported_target
                && !has_timing_tail
            {
                let filter = parse_object_filter(&filter_tokens, false)?;
                let target = parse_target_phrase(&target_tokens)?;
                return Ok(wrap_destroy_with_delayed_timing(
                    EffectAst::DestroyAllAttachedTo { filter, target },
                    delayed_timing,
                ));
            }
        }
        if let Some(except_for_idx) = core_tokens
            .windows(2)
            .position(|window| window[0].is_word("except") && window[1].is_word("for"))
            && except_for_idx > 1
        {
            let base_filter_tokens = trim_commas(&core_tokens[1..except_for_idx]);
            let exception_tokens = trim_commas(&core_tokens[except_for_idx + 2..]);
            if !base_filter_tokens.is_empty() && !exception_tokens.is_empty() {
                let mut filter = parse_object_filter(&base_filter_tokens, false)?;
                let exception_filter = parse_object_filter(&exception_tokens, false)?;
                apply_except_filter_exclusions(&mut filter, &exception_filter);
                return Ok(wrap_destroy_with_delayed_timing(
                    EffectAst::DestroyAll { filter },
                    delayed_timing,
                ));
            }
        }
        let filter_tokens = &core_tokens[1..];
        if let Some((choice_idx, consumed)) = find_color_choice_phrase(filter_tokens) {
            let base_filter_tokens = trim_commas(&filter_tokens[..choice_idx]);
            let trailing = trim_commas(&filter_tokens[choice_idx + consumed..]);
            if !trailing.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "unsupported trailing color-choice destroy-all clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            if base_filter_tokens.is_empty() {
                return Err(CardTextError::ParseError(format!(
                    "missing destroy-all filter before color-choice clause (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
            let filter = parse_object_filter(&base_filter_tokens, false)?;
            return Ok(wrap_destroy_with_delayed_timing(
                EffectAst::DestroyAllOfChosenColor { filter },
                delayed_timing,
            ));
        }
        let filter = parse_object_filter(filter_tokens, false)?;
        return Ok(wrap_destroy_with_delayed_timing(
            EffectAst::DestroyAll { filter },
            delayed_timing,
        ));
    }

    if clause_words.contains(&"unless") {
        return Err(CardTextError::ParseError(format!(
            "unsupported destroy-unless clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    if clause_words.contains(&"if") {
        return Err(CardTextError::ParseError(format!(
            "unsupported conditional destroy clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    if let Some(and_idx) = core_tokens.iter().position(|token| token.is_word("and")) {
        let tail_words = words(&core_tokens[and_idx + 1..]);
        let starts_multi_target = tail_words.first() == Some(&"target")
            || (tail_words.starts_with(&["up", "to"]) && tail_words.contains(&"target"));
        if starts_multi_target {
            return Err(CardTextError::ParseError(format!(
                "unsupported multi-target destroy clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    }

    if clause_words.starts_with(&["target", "blocked"]) {
        let mut target_tokens = core_tokens.to_vec();
        if let Some(blocked_idx) = target_tokens
            .iter()
            .position(|token| token.is_word("blocked"))
        {
            target_tokens.remove(blocked_idx);
        }
        let target = parse_target_phrase(&target_tokens)?;
        return Ok(wrap_destroy_with_delayed_timing(
            EffectAst::Conditional {
                predicate: PredicateAst::TargetIsBlocked,
                if_true: vec![EffectAst::Destroy { target }],
                if_false: Vec::new(),
            },
            delayed_timing,
        ));
    }

    let target = parse_target_phrase(&core_tokens)?;
    Ok(wrap_destroy_with_delayed_timing(
        EffectAst::Destroy { target },
        delayed_timing,
    ))
}

fn parse_destroy_combat_history_target(
    tokens: &[Token],
) -> Result<Option<TargetAst>, CardTextError> {
    let clause_words = words(tokens);
    if clause_words.as_slice()
        != [
            "target", "creature", "that", "was", "dealt", "damage", "this", "turn",
        ]
    {
        return Ok(None);
    }

    let target = parse_target_phrase(&tokens[..2])?;
    let TargetAst::Object(mut filter, target_span, it_span) = target else {
        return Ok(None);
    };
    filter.was_dealt_damage_this_turn = true;
    Ok(Some(TargetAst::Object(filter, target_span, it_span)))
}

fn apply_except_filter_exclusions(base: &mut ObjectFilter, exception: &ObjectFilter) {
    for card_type in exception
        .card_types
        .iter()
        .copied()
        .chain(exception.all_card_types.iter().copied())
    {
        if !base.excluded_card_types.contains(&card_type) {
            base.excluded_card_types.push(card_type);
        }
    }
    for subtype in exception.subtypes.iter().copied() {
        if !base.excluded_subtypes.contains(&subtype) {
            base.excluded_subtypes.push(subtype);
        }
    }
}

fn parse_exile(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let (tokens, until_source_leaves) = split_until_source_leaves_tail(tokens);
    let (tokens, face_down) = split_exile_face_down_suffix(tokens);
    let clause_words = words(tokens);
    let has_face_down_manifest_tail = (clause_words.contains(&"face-down")
        || clause_words.contains(&"facedown")
        || clause_words.contains(&"manifest")
        || clause_words.contains(&"pile"))
        && clause_words.contains(&"then");
    if has_face_down_manifest_tail {
        return Err(CardTextError::ParseError(format!(
            "unsupported face-down/manifest exile clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    if let Some(effect) = parse_same_name_exile_hand_and_graveyard_clause(
        tokens,
        subject,
        until_source_leaves,
        face_down,
    )? {
        return Ok(effect);
    }
    if matches!(clause_words.first().copied(), Some("all" | "each")) {
        let filter_tokens = &tokens[1..];
        let mut filter = parse_object_filter(filter_tokens, false)?;
        apply_exile_subject_owner_context(&mut filter, subject);
        return Ok(if until_source_leaves {
            EffectAst::ExileUntilSourceLeaves {
                target: TargetAst::Object(filter, None, None),
                face_down,
            }
        } else {
            EffectAst::ExileAll { filter, face_down }
        });
    }
    if let Some(filter) = parse_target_player_graveyard_filter(tokens) {
        return Ok(if until_source_leaves {
            EffectAst::ExileUntilSourceLeaves {
                target: TargetAst::Object(filter, None, None),
                face_down,
            }
        } else {
            EffectAst::ExileAll { filter, face_down }
        });
    }

    if clause_words.contains(&"dealt")
        && clause_words.contains(&"damage")
        && clause_words.contains(&"turn")
    {
        return Err(CardTextError::ParseError(format!(
            "unsupported combat-history exile clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let has_until_total_mana_value = clause_words.contains(&"until")
        && clause_words.contains(&"exiled")
        && clause_words.contains(&"total")
        && clause_words.contains(&"mana")
        && clause_words.contains(&"value");
    if has_until_total_mana_value {
        return Err(CardTextError::ParseError(format!(
            "unsupported iterative exile-total clause (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let has_attached_bundle = clause_words.contains(&"and")
        && clause_words.contains(&"all")
        && clause_words.contains(&"attached");
    if has_attached_bundle {
        return Err(CardTextError::ParseError(format!(
            "unsupported attached-object exile bundle (clause: '{}')",
            clause_words.join(" ")
        )));
    }
    let has_same_name_token_bundle = clause_words.contains(&"and")
        && clause_words.contains(&"tokens")
        && clause_words.contains(&"same")
        && clause_words.contains(&"name");
    if has_same_name_token_bundle {
        return Err(CardTextError::ParseError(format!(
            "unsupported same-name token exile bundle (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    if let Some(and_idx) = tokens.iter().position(|token| token.is_word("and"))
        && and_idx > 0
    {
        let tail_words = words(&tokens[and_idx + 1..]);
        let starts_multi_target = tail_words.first() == Some(&"target")
            || (tail_words.starts_with(&["up", "to"]) && tail_words.contains(&"target"));
        if starts_multi_target {
            return Err(CardTextError::ParseError(format!(
                "unsupported multi-target exile clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
    }

    if let Some(if_idx) = tokens.iter().position(|token| token.is_word("if"))
        && if_idx > 0
    {
        let target_tokens = trim_commas(&tokens[..if_idx]);
        let predicate_tokens = trim_commas(&tokens[if_idx + 1..]);
        if target_tokens.is_empty() || predicate_tokens.is_empty() {
            return Err(CardTextError::ParseError(format!(
                "unsupported conditional exile clause (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        let mut target = parse_target_phrase(&target_tokens)?;
        apply_exile_subject_hand_owner_context(&mut target, subject);
        let predicate = parse_predicate(&predicate_tokens)?;
        return Ok(EffectAst::Conditional {
            predicate,
            if_true: vec![if until_source_leaves {
                EffectAst::ExileUntilSourceLeaves { target, face_down }
            } else {
                EffectAst::Exile { target, face_down }
            }],
            if_false: Vec::new(),
        });
    }

    let mut target = parse_target_phrase(tokens)?;
    apply_exile_subject_hand_owner_context(&mut target, subject);
    Ok(if until_source_leaves {
        EffectAst::ExileUntilSourceLeaves { target, face_down }
    } else {
        EffectAst::Exile { target, face_down }
    })
}

fn parse_same_name_exile_hand_and_graveyard_clause(
    tokens: &[Token],
    subject: Option<SubjectAst>,
    until_source_leaves: bool,
    face_down: bool,
) -> Result<Option<EffectAst>, CardTextError> {
    let clause_words = words(tokens);
    if !(clause_words.starts_with(&["all", "cards"]) || clause_words.starts_with(&["all", "card"]))
        || !clause_words
            .windows(3)
            .any(|window| window == ["with", "that", "name"])
    {
        return Ok(None);
    }

    let Some(from_idx) = clause_words.iter().position(|word| *word == "from") else {
        return Ok(None);
    };
    let Some(first_zone_idx) = (from_idx + 1..clause_words.len()).find(|idx| {
        matches!(
            clause_words[*idx],
            "hand" | "hands" | "graveyard" | "graveyards"
        )
    }) else {
        return Ok(None);
    };

    let owner_words = &clause_words[from_idx + 1..first_zone_idx];
    let owner_from_subject = match subject {
        Some(SubjectAst::Player(player)) => controller_filter_for_token_player(player),
        _ => None,
    };
    let owner = match owner_words {
        ["target", "player"] | ["target", "players"] => Some(PlayerFilter::target_player()),
        ["target", "opponent"] | ["target", "opponents"] => Some(PlayerFilter::target_opponent()),
        ["that", "player"] | ["that", "players"] => Some(PlayerFilter::IteratedPlayer),
        ["your"] => Some(PlayerFilter::You),
        ["their"] | ["his", "or", "her"] => {
            owner_from_subject.or(Some(PlayerFilter::IteratedPlayer))
        }
        [] => owner_from_subject,
        _ => return Ok(None),
    };
    let Some(owner) = owner else {
        return Ok(None);
    };

    let mut zones = Vec::new();
    for word in &clause_words[first_zone_idx..] {
        let Some(zone) = parse_zone_word(word) else {
            continue;
        };
        if !matches!(zone, Zone::Hand | Zone::Graveyard) || zones.contains(&zone) {
            continue;
        }
        zones.push(zone);
    }
    if zones.len() != 2 || !zones.contains(&Zone::Hand) || !zones.contains(&Zone::Graveyard) {
        return Ok(None);
    }

    let mut filter = ObjectFilter::default();
    filter.owner = Some(owner);
    filter.tagged_constraints.push(TaggedObjectConstraint {
        tag: TagKey::from(IT_TAG),
        relation: TaggedOpbjectRelation::SameNameAsTagged,
    });
    filter.any_of = zones
        .into_iter()
        .map(|zone| ObjectFilter::default().in_zone(zone))
        .collect();

    Ok(Some(if until_source_leaves {
        EffectAst::ExileUntilSourceLeaves {
            target: TargetAst::Object(filter, None, None),
            face_down,
        }
    } else {
        EffectAst::ExileAll { filter, face_down }
    }))
}

fn split_until_source_leaves_tail(tokens: &[Token]) -> (&[Token], bool) {
    let Some(until_idx) = tokens.iter().rposition(|token| token.is_word("until")) else {
        return (tokens, false);
    };
    if until_idx == 0 {
        return (tokens, false);
    }
    let tail_words = words(&tokens[until_idx + 1..]);
    let has_source_leaves_tail = tail_words.len() >= 3
        && tail_words[tail_words.len() - 3] == "leaves"
        && tail_words[tail_words.len() - 2] == "the"
        && tail_words[tail_words.len() - 1] == "battlefield";
    if has_source_leaves_tail {
        (&tokens[..until_idx], true)
    } else {
        (tokens, false)
    }
}

fn split_exile_face_down_suffix(tokens: &[Token]) -> (&[Token], bool) {
    if tokens.is_empty() {
        return (tokens, false);
    }

    let mut end = tokens.len();
    if end > 0 && tokens[end - 1].is_word("instead") {
        end -= 1;
    }

    if end > 0 && (tokens[end - 1].is_word("face-down") || tokens[end - 1].is_word("facedown")) {
        return (&tokens[..end - 1], true);
    }

    if end >= 2 && tokens[end - 2].is_word("face") && tokens[end - 1].is_word("down") {
        return (&tokens[..end - 2], true);
    }

    (tokens, false)
}

fn parse_target_player_graveyard_filter(tokens: &[Token]) -> Option<ObjectFilter> {
    let words = words(tokens);
    if words.as_slice() == ["target", "player", "graveyard"]
        || words.as_slice() == ["target", "players", "graveyard"]
        || words.as_slice() == ["that", "player", "graveyard"]
        || words.as_slice() == ["that", "players", "graveyard"]
    {
        let mut filter = ObjectFilter::default().in_zone(Zone::Graveyard);
        filter.owner = Some(PlayerFilter::target_player());
        return Some(filter);
    }
    if words.as_slice() == ["target", "opponent", "graveyard"]
        || words.as_slice() == ["target", "opponents", "graveyard"]
    {
        let mut filter = ObjectFilter::default().in_zone(Zone::Graveyard);
        filter.owner = Some(PlayerFilter::Target(Box::new(PlayerFilter::Opponent)));
        return Some(filter);
    }
    if words.as_slice() == ["its", "controller", "graveyard"]
        || words.as_slice() == ["its", "controllers", "graveyard"]
    {
        let mut filter = ObjectFilter::default().in_zone(Zone::Graveyard);
        filter.owner = Some(PlayerFilter::ControllerOf(
            crate::filter::ObjectRef::tagged("triggering"),
        ));
        return Some(filter);
    }
    if words.as_slice() == ["its", "owner", "graveyard"]
        || words.as_slice() == ["its", "owners", "graveyard"]
    {
        let mut filter = ObjectFilter::default().in_zone(Zone::Graveyard);
        filter.owner = Some(PlayerFilter::OwnerOf(crate::filter::ObjectRef::tagged(
            "triggering",
        )));
        return Some(filter);
    }
    None
}

fn apply_exile_subject_hand_owner_context(target: &mut TargetAst, subject: Option<SubjectAst>) {
    let Some(filter) = target_object_filter_mut(target) else {
        return;
    };
    if filter.zone != Some(Zone::Hand) {
        return;
    }
    apply_exile_subject_owner_context(filter, subject);
}

fn apply_exile_subject_owner_context(filter: &mut ObjectFilter, subject: Option<SubjectAst>) {
    let Some(owner_filter) = exile_subject_owner_filter(subject) else {
        return;
    };
    let direct_zone_ok = matches!(
        filter.zone,
        Some(Zone::Hand) | Some(Zone::Graveyard) | Some(Zone::Library) | Some(Zone::Exile)
    );
    let any_of_zone_ok = filter.any_of.iter().any(|nested| {
        matches!(
            nested.zone,
            Some(Zone::Hand) | Some(Zone::Graveyard) | Some(Zone::Library) | Some(Zone::Exile)
        )
    });
    if !direct_zone_ok && !any_of_zone_ok {
        return;
    }
    match filter.owner {
        Some(PlayerFilter::Target(_)) | Some(PlayerFilter::IteratedPlayer) | None => {
            filter.owner = Some(owner_filter);
        }
        _ => {}
    }
}

fn apply_shuffle_subject_graveyard_owner_context(target: &mut TargetAst, subject: SubjectAst) {
    let Some(filter) = target_object_filter_mut(target) else {
        return;
    };
    if filter.zone != Some(Zone::Graveyard) {
        return;
    }

    let owner_filter = match subject {
        SubjectAst::Player(PlayerAst::Target) => Some(PlayerFilter::target_player()),
        SubjectAst::Player(PlayerAst::TargetOpponent) => Some(PlayerFilter::target_opponent()),
        SubjectAst::Player(PlayerAst::You) => Some(PlayerFilter::You),
        _ => None,
    };
    let Some(owner_filter) = owner_filter else {
        return;
    };

    match filter.owner {
        Some(PlayerFilter::IteratedPlayer) | Some(PlayerFilter::Target(_)) | None => {
            filter.owner = Some(owner_filter);
        }
        _ => {}
    }
}

fn exile_subject_owner_filter(subject: Option<SubjectAst>) -> Option<PlayerFilter> {
    match subject {
        Some(SubjectAst::Player(PlayerAst::Target)) => Some(PlayerFilter::target_player()),
        Some(SubjectAst::Player(PlayerAst::TargetOpponent)) => {
            Some(PlayerFilter::Target(Box::new(PlayerFilter::Opponent)))
        }
        Some(SubjectAst::Player(PlayerAst::That)) => Some(PlayerFilter::IteratedPlayer),
        Some(SubjectAst::Player(PlayerAst::You)) => Some(PlayerFilter::You),
        _ => None,
    }
}

fn target_object_filter_mut(target: &mut TargetAst) -> Option<&mut ObjectFilter> {
    match target {
        TargetAst::Object(filter, _, _) => Some(filter),
        TargetAst::WithCount(inner, _) => target_object_filter_mut(inner),
        _ => None,
    }
}

fn merge_it_match_filter_into_target(target: &mut TargetAst, it_filter: &ObjectFilter) -> bool {
    if let TargetAst::Tagged(tag, span) = target {
        let mut filter = ObjectFilter::default();
        filter.tagged_constraints.push(TaggedObjectConstraint {
            tag: tag.clone(),
            relation: TaggedOpbjectRelation::IsTaggedObject,
        });
        *target = TargetAst::Object(filter, span.clone(), None);
    }

    let Some(filter) = target_object_filter_mut(target) else {
        return false;
    };
    if !it_filter.card_types.is_empty() {
        filter.card_types = it_filter.card_types.clone();
    }
    if !it_filter.subtypes.is_empty() {
        filter.subtypes = it_filter.subtypes.clone();
    }
    if let Some(power) = &it_filter.power {
        filter.power = Some(power.clone());
        filter.power_reference = it_filter.power_reference;
    }
    if let Some(toughness) = &it_filter.toughness {
        filter.toughness = Some(toughness.clone());
        filter.toughness_reference = it_filter.toughness_reference;
    }
    if let Some(mana_value) = &it_filter.mana_value {
        filter.mana_value = Some(mana_value.clone());
    }
    true
}

fn parse_untap(tokens: &[Token]) -> Result<EffectAst, CardTextError> {
    if tokens.is_empty() {
        return Err(CardTextError::ParseError(
            "untap clause missing target".to_string(),
        ));
    }
    let words = words(tokens);
    if matches!(words.first().copied(), Some("all" | "each")) {
        let filter = parse_object_filter(&tokens[1..], false)?;
        return Ok(EffectAst::UntapAll { filter });
    }
    if words.as_slice() == ["them"] {
        let mut filter = ObjectFilter::default();
        filter.tagged_constraints.push(TaggedObjectConstraint {
            tag: IT_TAG.into(),
            relation: TaggedOpbjectRelation::IsTaggedObject,
        });
        return Ok(EffectAst::UntapAll { filter });
    }
    let target = parse_target_phrase(tokens)?;
    Ok(EffectAst::Untap { target })
}

fn parse_scry(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let (count, _) = parse_value(tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing scry count (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;

    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    Ok(EffectAst::Scry { count, player })
}

fn parse_surveil(
    tokens: &[Token],
    subject: Option<SubjectAst>,
) -> Result<EffectAst, CardTextError> {
    let (count, _) = parse_value(tokens).ok_or_else(|| {
        CardTextError::ParseError(format!(
            "missing surveil count (clause: '{}')",
            words(tokens).join(" ")
        ))
    })?;

    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    Ok(EffectAst::Surveil { count, player })
}

fn parse_pay(tokens: &[Token], subject: Option<SubjectAst>) -> Result<EffectAst, CardTextError> {
    let player = match subject {
        Some(SubjectAst::Player(player)) => player,
        _ => PlayerAst::Implicit,
    };

    if let Some((amount, used)) = parse_value(tokens)
        && tokens.get(used).is_some_and(|token| token.is_word("life"))
    {
        return Ok(EffectAst::LoseLife { amount, player });
    }
    if let Some((amount, used)) = parse_value(tokens)
        && tokens
            .get(used)
            .is_some_and(|token| token.is_word("energy"))
    {
        return Ok(EffectAst::PayEnergy { amount, player });
    }
    if tokens.iter().any(|token| token.is_word("e")) {
        let mut energy_count = 0u32;
        for token in tokens {
            let Some(word) = token.as_word() else {
                continue;
            };
            if is_article(word)
                || word == "and"
                || word == "or"
                || word == "energy"
                || word == "counter"
                || word == "counters"
            {
                continue;
            }
            if word == "e" {
                energy_count += 1;
                continue;
            }
            return Err(CardTextError::ParseError(format!(
                "unsupported pay clause token '{word}' (clause: '{}')",
                words(tokens).join(" ")
            )));
        }
        if energy_count > 0 {
            return Ok(EffectAst::PayEnergy {
                amount: Value::Fixed(energy_count as i32),
                player,
            });
        }
    }

    let mut pips = Vec::new();
    for token in tokens {
        let Some(word) = token.as_word() else {
            continue;
        };
        if is_article(word) || word == "mana" {
            continue;
        }
        if let Ok(symbols) = parse_mana_symbol_group(&word) {
            pips.push(symbols);
            continue;
        }
        return Err(CardTextError::ParseError(format!(
            "unsupported pay clause token '{word}' (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    if pips.is_empty() {
        return Err(CardTextError::ParseError(format!(
            "missing payment cost (clause: '{}')",
            words(tokens).join(" ")
        )));
    }

    Ok(EffectAst::PayMana {
        cost: ManaCost::from_pips(pips),
        player,
    })
}

fn parse_filter_comparison_tokens(
    axis: &str,
    tokens: &[&str],
    clause_words: &[&str],
) -> Result<Option<(crate::filter::Comparison, usize)>, CardTextError> {
    if tokens.is_empty() {
        return Ok(None);
    }

    let parse_operand = |operand: &str, extra_words: &[&str]| -> Result<i32, CardTextError> {
        let value = match operand.parse::<i32>() {
            Ok(value) => value,
            Err(_) => {
                return Err(CardTextError::ParseError(format!(
                    "unsupported dynamic {axis} comparison operand '{operand}' (clause: '{}')",
                    clause_words.join(" ")
                )));
            }
        };
        if extra_words
            .first()
            .is_some_and(|word| matches!(*word, "plus" | "minus"))
        {
            return Err(CardTextError::ParseError(format!(
                "unsupported arithmetic {axis} comparison (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        Ok(value)
    };

    let first = tokens[0];
    if let Ok(value) = first.parse::<i32>() {
        if tokens
            .get(1)
            .is_some_and(|word| matches!(*word, "plus" | "minus"))
        {
            return Err(CardTextError::ParseError(format!(
                "unsupported arithmetic {axis} comparison (clause: '{}')",
                clause_words.join(" ")
            )));
        }
        if tokens.get(1) == Some(&"or")
            && tokens
                .get(2)
                .is_some_and(|word| matches!(*word, "greater" | "more"))
        {
            return Ok(Some((
                crate::filter::Comparison::GreaterThanOrEqual(value),
                3,
            )));
        }
        if tokens.get(1) == Some(&"or")
            && tokens
                .get(2)
                .is_some_and(|word| matches!(*word, "less" | "fewer"))
        {
            return Ok(Some((crate::filter::Comparison::LessThanOrEqual(value), 3)));
        }
        let mut values = vec![value];
        let mut consumed = 1usize;
        while consumed < tokens.len() {
            let token = tokens[consumed];
            if matches!(token, "and" | "or" | "and/or") {
                consumed += 1;
                continue;
            }
            if let Ok(next_value) = token.parse::<i32>() {
                values.push(next_value);
                consumed += 1;
                continue;
            }
            break;
        }
        if values.len() > 1 {
            return Ok(Some((crate::filter::Comparison::OneOf(values), consumed)));
        }
        return Ok(Some((crate::filter::Comparison::Equal(value), 1)));
    }

    if first == "equal" && tokens.get(1) == Some(&"to") {
        let Some(operand) = tokens.get(2).copied() else {
            return Err(CardTextError::ParseError(format!(
                "missing {axis} comparison operand after 'equal to' (clause: '{}')",
                clause_words.join(" ")
            )));
        };
        let value = parse_operand(operand, &tokens[3..])?;
        return Ok(Some((crate::filter::Comparison::Equal(value), 3)));
    }

    if matches!(first, "less" | "greater") && tokens.get(1) == Some(&"than") {
        let mut operand_idx = 2usize;
        let mut inclusive = false;
        if tokens.get(operand_idx) == Some(&"or")
            && tokens.get(operand_idx + 1) == Some(&"equal")
            && tokens.get(operand_idx + 2) == Some(&"to")
        {
            inclusive = true;
            operand_idx += 3;
        }
        let Some(operand) = tokens.get(operand_idx).copied() else {
            return Err(CardTextError::ParseError(format!(
                "missing {axis} comparison operand after '{first} than' (clause: '{}')",
                clause_words.join(" ")
            )));
        };
        let value = parse_operand(operand, &tokens[operand_idx + 1..])?;
        let cmp = match (first, inclusive) {
            ("less", true) => crate::filter::Comparison::LessThanOrEqual(value),
            ("less", false) => crate::filter::Comparison::LessThan(value),
            ("greater", true) => crate::filter::Comparison::GreaterThanOrEqual(value),
            ("greater", false) => crate::filter::Comparison::GreaterThan(value),
            _ => unreachable!("first is constrained above"),
        };
        return Ok(Some((cmp, operand_idx + 1)));
    }

    if first == "x" {
        return Err(CardTextError::ParseError(format!(
            "unsupported dynamic {axis} comparison operand '{first}' (clause: '{}')",
            clause_words.join(" ")
        )));
    }

    Ok(None)
}

