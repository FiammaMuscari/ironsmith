//! cf8 p05: "time travel" (CR 701.55) read as a clause anywhere, including
//! "time travel three times" and "time travel, then time travel".
//! Source-authored; deliberately unrun.
#[path = "cf8_p05_support/mod.rs"]
mod support;

#[test]
fn time_travel_cards_compile_with_counted_time_travel() {
    let rows = support::rows("time_travel_clause");
    assert_eq!(rows.len(), 3);
    for row in &rows {
        support::definitions(row);
    }
    // Two explicitly sequenced actions need no repeat wrapper. Verify both
    // time-counter operations instead of requiring one lowering strategy.
    for definition in support::definitions(&support::row("time_travel_clause", "The Parting of the Ways")) {
        let actions = support::all_effects(&definition);
        assert_eq!(actions.iter().filter(|effect| {
            effect.downcast_ref::<ironsmith::effects::ForEachCounterKindPutOrRemoveEffect>()
                .is_some_and(|effect| effect.fixed_counter_type == Some(ironsmith::object::CounterType::Time))
        }).count(), 2);
    }
    support::assert_markers("time_travel_clause", "The Tenth Doctor", &["Repeat", "Time"]);
}
