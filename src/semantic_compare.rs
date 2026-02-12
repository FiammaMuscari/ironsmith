use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy)]
pub struct EmbeddingConfig {
    pub dims: usize,
    pub mismatch_threshold: f32,
}

fn strip_parenthetical(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for ch in text.chars() {
        if ch == '(' {
            depth += 1;
            continue;
        }
        if ch == ')' {
            depth = depth.saturating_sub(1);
            continue;
        }
        if depth == 0 {
            out.push(ch);
        }
    }
    out
}

fn semantic_clauses(text: &str) -> Vec<String> {
    let mut clauses = Vec::new();
    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line = if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = trimmed.trim_start_matches('(').trim_end_matches(')').trim();
            // Keep parenthetical lines only when they carry executable semantics
            // (most notably mana abilities like "({T}: Add {G}.)").
            if inner.contains(':') {
                inner.to_string()
            } else {
                continue;
            }
        } else {
            strip_parenthetical(raw_line)
        };
        let mut current = String::new();
        for ch in line.chars() {
            if matches!(ch, '.' | ';' | '\n') {
                let trimmed = current.trim();
                if !trimmed.is_empty() && trimmed.chars().any(|ch| ch.is_ascii_alphanumeric()) {
                    clauses.push(trimmed.to_string());
                }
                current.clear();
            } else {
                current.push(ch);
            }
        }
        let trimmed = current.trim();
        if !trimmed.is_empty() && trimmed.chars().any(|ch| ch.is_ascii_alphanumeric()) {
            clauses.push(trimmed.to_string());
        }
    }
    clauses
}

fn tokenize_text(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_braces = false;

    for ch in lower.chars() {
        if in_braces {
            current.push(ch);
            if ch == '}' {
                tokens.push(current.clone());
                current.clear();
                in_braces = false;
            }
            continue;
        }

        if ch == '{' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(ch);
            in_braces = true;
            continue;
        }

        if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '+' | '-' | '\'') {
            current.push(ch);
            continue;
        }

        if !current.is_empty() {
            tokens.push(current.clone());
            current.clear();
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn is_number_token(token: &str) -> bool {
    token == "x" || token.parse::<i64>().is_ok()
}

fn is_pt_component(value: &str) -> bool {
    let stripped = value.trim_matches(|c| matches!(c, '+' | '-'));
    stripped == "x" || stripped == "*" || stripped.parse::<i32>().is_ok()
}

fn is_pt_token(token: &str) -> bool {
    let Some((left, right)) = token.split_once('/') else {
        return false;
    };
    is_pt_component(left) && is_pt_component(right)
}

fn normalize_word(token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    if matches!(
        token,
        "zero"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
    ) {
        return Some("<num>".to_string());
    }
    if token == "plusoneplusone" || token == "minusoneminusone" {
        return Some("<pt>".to_string());
    }
    if token.starts_with('{') && token.ends_with('}') {
        return Some("<mana>".to_string());
    }
    if is_pt_token(token) {
        return Some("<pt>".to_string());
    }
    if is_number_token(token) {
        return Some("<num>".to_string());
    }

    let mut base = token.trim_matches('\'').to_string();
    if base.ends_with("'s") {
        base.truncate(base.len().saturating_sub(2));
    }
    if base.len() > 4 && base.ends_with('s') {
        base.pop();
    }
    if base.is_empty() {
        None
    } else {
        Some(base)
    }
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "the"
            | "to"
            | "of"
            | "and"
            | "or"
            | "for"
            | "from"
            | "in"
            | "on"
            | "at"
            | "with"
            | "into"
            | "onto"
            | "up"
            | "down"
    )
}

fn comparison_tokens(clause: &str) -> Vec<String> {
    tokenize_text(clause)
        .into_iter()
        .filter_map(|token| normalize_word(&token))
        .filter(|token| !is_stopword(token))
        .collect()
}

fn embedding_tokens(clause: &str) -> Vec<String> {
    tokenize_text(clause)
        .into_iter()
        .filter_map(|token| normalize_word(&token))
        .collect()
}

fn hash_index(feature: &str, dims: usize) -> usize {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    feature.hash(&mut hasher);
    (hasher.finish() as usize) % dims.max(1)
}

fn hash_sign(feature: &str) -> f32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("sign", feature).hash(&mut hasher);
    if hasher.finish() & 1 == 0 {
        1.0
    } else {
        -1.0
    }
}

fn add_feature(vec: &mut [f32], feature: &str, weight: f32) {
    let idx = hash_index(feature, vec.len());
    vec[idx] += hash_sign(feature) * weight;
}

fn l2_normalize(vec: &mut [f32]) {
    let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vec {
            *v /= norm;
        }
    }
}

fn embed_clause(clause: &str, dims: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; dims.max(1)];
    let tokens = embedding_tokens(clause);

    for token in &tokens {
        add_feature(&mut vec, &format!("u:{token}"), 1.0);
    }
    for window in tokens.windows(2) {
        add_feature(&mut vec, &format!("b:{}|{}", window[0], window[1]), 1.35);
    }
    for window in tokens.windows(3) {
        add_feature(
            &mut vec,
            &format!("t:{}|{}|{}", window[0], window[1], window[2]),
            1.65,
        );
    }

    // Structural anchors for common semantic clauses.
    let lower = clause.to_ascii_lowercase();
    for marker in ["where", "plus", "minus", "for each", "as long as", "unless"] {
        if lower.contains(marker) {
            add_feature(&mut vec, &format!("m:{marker}"), 1.8);
        }
    }

    // Lightweight character n-grams help when token sets are similar but syntax differs.
    let compact = lower
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == ' ')
        .collect::<String>();
    let chars: Vec<char> = compact.chars().collect();
    for ngram in chars.windows(4).take(200) {
        let key = ngram.iter().collect::<String>();
        add_feature(&mut vec, &format!("c:{key}"), 0.35);
    }

    l2_normalize(&mut vec);
    vec
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0f32;
    for i in 0..len {
        dot += a[i] * b[i];
    }
    dot.clamp(-1.0, 1.0)
}

fn directional_embedding_coverage(from: &[Vec<f32>], to: &[Vec<f32>]) -> f32 {
    if from.is_empty() {
        return if to.is_empty() { 1.0 } else { 0.0 };
    }

    let mut total = 0.0f32;
    for source in from {
        let mut best = -1.0f32;
        for target in to {
            let score = cosine_similarity(source, target);
            if score > best {
                best = score;
            }
        }
        total += best.max(0.0);
    }
    total / from.len() as f32
}

fn jaccard_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a_set: std::collections::HashSet<&str> = a.iter().map(String::as_str).collect();
    let b_set: std::collections::HashSet<&str> = b.iter().map(String::as_str).collect();
    let inter = a_set.intersection(&b_set).count() as f32;
    let union = a_set.union(&b_set).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn directional_coverage(from: &[Vec<String>], to: &[Vec<String>]) -> f32 {
    if from.is_empty() {
        return if to.is_empty() { 1.0 } else { 0.0 };
    }

    let mut total = 0.0f32;
    for source in from {
        let mut best = 0.0f32;
        for target in to {
            let score = jaccard_similarity(source, target);
            if score > best {
                best = score;
            }
        }
        total += best;
    }
    total / from.len() as f32
}

fn is_compiled_heading_prefix(prefix: &str) -> bool {
    let prefix = prefix.trim().to_ascii_lowercase();
    prefix == "spell effects"
        || prefix.starts_with("activated ability ")
        || prefix.starts_with("triggered ability ")
        || prefix.starts_with("static ability ")
        || prefix.starts_with("keyword ability ")
        || prefix.starts_with("mana ability ")
        || prefix.starts_with("ability ")
        || prefix.starts_with("alternative cast ")
}

fn strip_compiled_prefix(line: &str) -> &str {
    let Some((prefix, rest)) = line.split_once(':') else {
        return line;
    };
    if is_compiled_heading_prefix(prefix) {
        rest.trim()
    } else {
        line
    }
}

pub fn compare_semantics_scored(
    oracle_text: &str,
    compiled_lines: &[String],
    embedding: Option<EmbeddingConfig>,
) -> (f32, f32, f32, isize, bool) {
    let oracle_clauses = semantic_clauses(oracle_text);
    let compiled_clauses = compiled_lines
        .iter()
        .flat_map(|line| semantic_clauses(strip_compiled_prefix(line)))
        .collect::<Vec<_>>();

    let oracle_tokens: Vec<Vec<String>> = oracle_clauses
        .iter()
        .map(|clause| comparison_tokens(clause))
        .filter(|tokens| !tokens.is_empty())
        .collect();
    let compiled_tokens: Vec<Vec<String>> = compiled_clauses
        .iter()
        .map(|clause| comparison_tokens(clause))
        .filter(|tokens| !tokens.is_empty())
        .collect();

    // Parenthetical-only oracle text (typically reminder text) carries no
    // semantic clauses after normalization, so don't flag as mismatch.
    if oracle_tokens.is_empty() {
        return (1.0, 1.0, 1.0, 0, false);
    }

    let oracle_coverage = directional_coverage(&oracle_tokens, &compiled_tokens);
    let compiled_coverage = directional_coverage(&compiled_tokens, &oracle_tokens);
    let line_delta = compiled_clauses.len() as isize - oracle_clauses.len() as isize;

    let min_coverage = oracle_coverage.min(compiled_coverage);
    let semantic_gap = min_coverage < 0.25;
    let line_gap = line_delta.abs() >= 3 && min_coverage < 0.50;
    let empty_gap = !oracle_tokens.is_empty() && compiled_tokens.is_empty();

    let mut similarity_score = min_coverage;
    let mut mismatch = semantic_gap || line_gap || empty_gap;

    if let Some(cfg) = embedding {
        let oracle_emb = oracle_clauses
            .iter()
            .map(|clause| embed_clause(clause, cfg.dims))
            .collect::<Vec<_>>();
        let compiled_emb = compiled_clauses
            .iter()
            .map(|clause| embed_clause(clause, cfg.dims))
            .collect::<Vec<_>>();
        let emb_oracle = directional_embedding_coverage(&oracle_emb, &compiled_emb);
        let emb_compiled = directional_embedding_coverage(&compiled_emb, &oracle_emb);
        let emb_min = emb_oracle.min(emb_compiled);
        similarity_score = emb_min;
        if emb_min < cfg.mismatch_threshold {
            mismatch = true;
        }
    }

    (
        oracle_coverage,
        compiled_coverage,
        similarity_score,
        line_delta,
        mismatch,
    )
}

pub fn compare_semantics(
    oracle_text: &str,
    compiled_lines: &[String],
    embedding: Option<EmbeddingConfig>,
) -> (f32, f32, isize, bool) {
    let (oracle_coverage, compiled_coverage, _similarity_score, line_delta, mismatch) =
        compare_semantics_scored(oracle_text, compiled_lines, embedding);
    (oracle_coverage, compiled_coverage, line_delta, mismatch)
}
