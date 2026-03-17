# Deferred

- Distinct-color mana output remains deferred for Bloom Tender and Faeburrow Elder. Exact support for "For each color among permanents you control, add one mana of that color" needs new sentence-level color iteration or a dedicated mana effect, which is outside this parser/lowering-only pass.
- Spent-to-cast provenance work remains deferred, including "colors of mana spent to cast" style tracking.
- Keyword-cost work remains deferred, including new lowering for keyword-specific payment machinery.
- Generic X-expression work remains deferred where existing value parsing cannot lower the expression without broader support.
- Any new mechanic or effect work remains deferred in this pass.
