# Deferred

- Generic `where X is ...` follow-up work that would need broader value-grammar expansion beyond the parser-only tails handled here.
- Exact non-target single-opponent chooser support for clauses like `look at an opponent's hand` in multiplayer, which would need dedicated non-target player-choice lowering instead of the existing broad opponent filter semantics.
- New runtime/value mechanics for exact "cards you've drawn this turn" counts, including `Fists of Flame` style `gets +1/+0 for each card you've drawn this turn`.
- New runtime/value mechanics for counting distinct mana values among cards in graveyards, including `All-Seeing Arbiter` style `where X is the number of different mana values among cards in your graveyard`.
