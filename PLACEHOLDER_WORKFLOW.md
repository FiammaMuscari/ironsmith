# Placeholder Elimination Workflow

This runbook defines the repeatable migration loop for eliminating placeholder mechanics:

- `StaticAbilityId::KeywordMarker`
- `StaticAbilityId::RuleTextPlaceholder`
- `StaticAbilityId::UnsupportedParserLine`

Use this for one mechanic family or fallback-reason category at a time, and keep parser, lowering/runtime, and tests in the same PR.

---

## 1. Freeze a Baseline First

Before code changes, record a measurable slice baseline:

```bash
cargo run --quiet --bin audit_parsed_mechanics -- \
  --cards /Users/chiplis/ironsmith/cards.json \
  --slice-mechanic "<family-prefix>" \
  --slice-fallback-reason "<reason-prefix>" \
  --json-out /tmp/parsed_mechanics_<slice>.json
```

Track these three numbers under `.slice`:

- `placeholder_count`
- `unsupported_reason_count`
- `affected_content_count`

Slice done criteria:

1. Zero hits for the slice in `audit_parsed_mechanics`.
2. No placeholder ability IDs emitted for migrated cards in compile-shape tests.
3. Relevant parser and simulation tests pass.

---

## 2. Migration Loop (One Slice Per PR)

### 2.1 `KeywordMarker` -> typed mechanic

1. Inventory marker strings with audit output and group into a mechanic family.
2. Add explicit parser-side AST/enum variants for that family.
3. Route phrase matching to typed variants before marker fallback branches.
4. Lower typed AST into concrete triggered/activated/static behavior.
5. Add runtime executor/trigger wiring only when existing primitives cannot express semantics.
6. Remove family-specific marker matching in classification/filtering/matching consumers.
7. Keep marker fallback only for still-unimplemented families.
8. Add tests:
   - parse mapping test
   - compile-shape test (no marker IDs)
   - simulation behavior test

Exit criterion: migrated family no longer appears in unimplemented marker audit output.

### 2.2 `RuleTextPlaceholder` -> typed static/restriction/replacement logic

1. Classify placeholder lines by semantics:
   - restrictions
   - replacement effects
   - continuous effects
   - permissions
   - timing constraints
2. Add dedicated parse functions per bucket and run them before placeholder fallback.
3. Introduce concrete static ability/restriction/replacement structures.
4. Wire runtime checks into the right phase hooks (untap/combat/casting/zone-change/etc.).
5. Ensure rendering outputs canonical text from typed mechanics, not raw placeholder text.
6. Remove migrated bucket from placeholder fallback branches.
7. Add tests:
   - parse acceptance
   - runtime simulation
   - interaction/regression coverage

Exit criterion: migrated bucket no longer emits `RuleTextPlaceholder` in definitions or audit output.

### 2.3 `UnsupportedParserLine` -> real parse + compile coverage

1. Prioritize by fallback-reason taxonomy (highest-volume reasons first).
2. Expand grammar/helpers so covered patterns produce typed AST instead of parse errors.
3. Extend AST only for genuinely missing constructs.
4. Extend lowering so new AST compiles into executable effects/triggers.
5. Resolve delayed/anaphoric references (`that`, `it`, `this`) via tagging/LKI infrastructure.
6. Keep fallback behavior in `allow-unsupported` mode only for still-unknown reasons.
7. Add tests:
   - strict parse succeeds for covered pattern
   - delayed/runtime execution works
   - no fallback marker emission for covered category

Exit criterion: covered reason disappears from fallback-reason audit output and no longer emits `UnsupportedParserLine`.

---

## 3. Detailed Execution Playbook (Myriad + Undying/Persist Pattern)

Use this playbook for each family, not as a one-off checklist:

1. Freeze baseline counts (placeholder/reason/affected-content) before parser edits.
2. Make parser output typed AST for the family before fallback branches.
3. Lower typed AST via shared primitives first; add new primitive only when semantics are missing.
4. Preserve parse-level keyword identity for readability, but execute through composed runtime primitives.
5. Integrate runtime hooks only where primitives require new engine support.
6. Keep rendering deterministic and canonical; remove placeholder-render branches for migrated paths.
7. Tighten downstream consumers to rely on typed IDs/fields, not marker strings.
8. Validate with parser + compile-shape + simulation layers on each slice.
9. Gate each PR with bounded audits so migrated slices cannot regress.

Myriad-specific lowering shape reference:

1. Attack trigger on source.
2. Iterate eligible opponents.
3. Exclude current defending player.
4. Create token copies tapped and attacking.
5. Resolve attacker choice for "that player or a planeswalker they control".
6. Schedule end-of-combat exile for created tokens.

Undying/Persist template shape reference:

1. Trigger on dies using last-known identity.
2. Guard on counter predicate (no relevant counter).
3. Return to battlefield under owner control.
4. Apply ETB counter (`+1/+1` for Undying, `-1/-1` for Persist).
5. Use tagging/LKI infrastructure, never text-string assumptions.

---

## 4. Cross-Cutting Quality Gates

Apply to every placeholder-class migration:

1. Keep migrations incremental: one family/category per PR.
2. Include parser + lowering/runtime + tests in the same PR.
3. Run:
   - `cargo test -q`
   - bounded slice audit
4. Add CI checks failing on regressions for migrated slices:

```bash
cargo run --quiet --bin audit_parsed_mechanics -- \
  --cards /Users/chiplis/ironsmith/cards.json \
  --slice-mechanic "<migrated-family-prefix>" \
  --slice-fallback-reason "<migrated-reason-prefix>" \
  --fail-on-slice-hits
```

5. Run periodic full audit to catch cross-slice regressions.

---

## 5. Program Completion and Final Cleanup

Do not remove global placeholder types early. Final cleanup starts only when placeholder inventory is effectively zero.

Final cleanup scope:

1. Remove placeholder IDs/types.
2. Remove placeholder-only branches in:
   - support classification gates
   - marker-based filters
   - compiled text placeholder rendering
3. Keep periodic full audits after cleanup to prevent reintroduction.

---

## 6. PR Template (Suggested)

Use this short checklist in each migration PR:

1. Baseline recorded (`placeholder_count`, `unsupported_reason_count`, `affected_content_count`).
2. Parser emits typed AST for migrated slice before fallback.
3. Lowering/runtime wired with primitives (or justified new primitive).
4. Consumer cleanup completed (no slice-specific marker string checks).
5. Tests added:
   - parser
   - compile-shape (no placeholders)
   - simulation
6. `cargo test -q` passes.
7. Bounded slice audit passes with `--fail-on-slice-hits`.
8. Slice no longer present in audit outputs.
