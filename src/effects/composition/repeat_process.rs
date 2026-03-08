use crate::effect::{Effect, EffectId, EffectOutcome, EffectPredicate};
use crate::effects::{EffectExecutor, SequenceEffect};
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;

#[derive(Debug, Clone, PartialEq)]
pub struct RepeatProcessEffect {
    pub effects: Vec<Effect>,
    pub condition: EffectId,
    pub predicate: EffectPredicate,
}

impl RepeatProcessEffect {
    pub fn new(effects: Vec<Effect>, condition: EffectId, predicate: EffectPredicate) -> Self {
        Self {
            effects,
            condition,
            predicate,
        }
    }
}

impl EffectExecutor for RepeatProcessEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let sequence = SequenceEffect::new(self.effects.clone());
        let mut all_events = Vec::new();
        let result = loop {
            let outcome = sequence.execute(game, ctx)?;
            all_events.extend(outcome.events.clone());

            if outcome.result.is_failure() {
                break outcome.result;
            }

            let should_continue = ctx
                .effect_results
                .get(&self.condition)
                .is_some_and(|result| self.predicate.evaluate(result));
            if !should_continue {
                break outcome.result;
            }
        };

        Ok(EffectOutcome {
            result,
            events: all_events,
        })
    }
}
