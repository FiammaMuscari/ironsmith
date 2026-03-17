# Deferred

This worktree only widened parser/lowering paths that already map onto existing engine mechanics.

## Still Deferred: New Execution Semantics

- Per-player untap caps such as `Players can't untap more than one creature during their untap steps.` These need a new restriction/tracker model instead of the existing binary `can't untap` restriction.
- Mana-retention clauses such as `You don't lose this mana as steps and phases end.` These need explicit mana-pool lifetime semantics, not just parsing.
- Cast restrictions on a specific named card or card subset gated by a condition, such as `You can't cast Rakdos unless an opponent lost life this turn.` These need a way to restrict casting a specific object/card identity, not just broad player/spell filters.
- Clauses that stop players from both casting spells and activating abilities when the ability stop must include mana abilities, such as `That player can't cast spells or activate abilities.` Existing player-based restriction support only covers non-mana activated abilities.

## Deferred In This Ownership Slice

- Modal headers like `Choose any number —` on cards such as `Rankle, Master of Pranks`. Existing choose-mode execution is already present, but the remaining parser work for that header shape lives outside the owned files for this task.
