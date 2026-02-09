//! "Whenever an ability of [filter] is activated" trigger.

use crate::events::EventKind;
use crate::events::spells::AbilityActivatedEvent;
use crate::target::ObjectFilter;
use crate::triggers::TriggerEvent;
use crate::triggers::matcher_trait::{TriggerContext, TriggerMatcher};

#[derive(Debug, Clone, PartialEq)]
pub struct AbilityActivatedTrigger {
    pub filter: ObjectFilter,
}

impl AbilityActivatedTrigger {
    pub fn new(filter: ObjectFilter) -> Self {
        Self { filter }
    }
}

impl TriggerMatcher for AbilityActivatedTrigger {
    fn matches(&self, event: &TriggerEvent, ctx: &TriggerContext) -> bool {
        if event.kind() != EventKind::AbilityActivated {
            return false;
        }
        let Some(e) = event.downcast::<AbilityActivatedEvent>() else {
            return false;
        };

        if let Some(obj) = ctx.game.object(e.source) {
            self.filter.matches(obj, &ctx.filter_ctx, ctx.game)
        } else if let Some(snapshot) = e.snapshot.as_ref() {
            self.filter
                .matches_snapshot(snapshot, &ctx.filter_ctx, ctx.game)
        } else {
            false
        }
    }

    fn display(&self) -> String {
        format!(
            "Whenever an ability of {} is activated",
            self.filter.description()
        )
    }

    fn clone_box(&self) -> Box<dyn TriggerMatcher> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display() {
        let trigger = AbilityActivatedTrigger::new(ObjectFilter::default());
        assert!(trigger.display().contains("activated"));
    }
}
