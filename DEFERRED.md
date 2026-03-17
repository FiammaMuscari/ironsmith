# Deferred In This Worktree

This worktree only touched target/object parsing in:

- `src/cards/builders/parse_parsing/targets.rs`
- parser regressions in tests

The following cases are intentionally deferred because they either need parser work outside the owned files for this task or require new mechanic/effect support:

- Fireball-style divided damage and other distributed-target damage phrases.
- Coin-flip or random-choice pseudo-targets such as `coin`.
- `"the rest"` / pile-splitting / Balance-style restructuring follow-ups.
- Player-iteration subject phrases like `each other player` that currently route through non-owned player/for-each parsing paths.
- Player-subject restriction clauses with leading condition/duration prefixes, including representative failures like `Island Sanctuary`, `Teferi's Protection`, and `Grand Abolisher`.
- Any new mechanic/effect work beyond existing `ObjectFilter` / `TargetAst` / restriction lowering support.
