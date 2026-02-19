//! ChooseMode effect implementation.

use crate::effect::{EffectMode, EffectOutcome, Value};
use crate::effects::EffectExecutor;
use crate::effects::executor_trait::ModalSpec;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;

/// Effect that presents modal choices to the player.
///
/// For modal spells like "Choose one —" or "Choose one or more —".
///
/// # Fields
///
/// * `modes` - Available mode choices
/// * `choose_count` - Maximum number of modes to choose
/// * `min_choose_count` - Minimum modes to choose (defaults to choose_count if None)
///
/// # Example
///
/// ```ignore
/// // "Choose one —"
/// let effect = ChooseModeEffect::choose_one(vec![
///     EffectMode::new("Deal 3 damage to any target", vec![Effect::deal_damage(3, ...)]),
///     EffectMode::new("Gain 3 life", vec![Effect::gain_life(3)]),
/// ]);
///
/// // "Choose one or both —"
/// let effect = ChooseModeEffect::choose_up_to(2, 1, vec![...]);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ChooseModeEffect {
    /// Available mode choices.
    pub modes: Vec<EffectMode>,
    /// Maximum number of modes to choose.
    pub choose_count: Value,
    /// Minimum modes to choose. If None, defaults to choose_count (exact choice).
    pub min_choose_count: Option<Value>,
    /// Whether the same mode can be chosen more than once.
    pub allow_repeated_modes: bool,
    /// Whether chosen modes are disallowed for future activations of the same ability.
    pub disallow_previously_chosen_modes: bool,
    /// Whether "previously chosen" tracking resets each turn.
    pub disallow_previously_chosen_modes_this_turn: bool,
}

impl ChooseModeEffect {
    /// Create a new ChooseMode effect.
    pub fn new(
        modes: Vec<EffectMode>,
        choose_count: Value,
        min_choose_count: Option<Value>,
    ) -> Self {
        Self {
            modes,
            choose_count,
            min_choose_count,
            allow_repeated_modes: false,
            disallow_previously_chosen_modes: false,
            disallow_previously_chosen_modes_this_turn: false,
        }
    }

    /// Create a "choose one" modal effect.
    pub fn choose_one(modes: Vec<EffectMode>) -> Self {
        Self::new(modes, Value::Fixed(1), None)
    }

    /// Create a "choose X" modal effect with exact count required.
    pub fn choose_exactly(count: impl Into<Value>, modes: Vec<EffectMode>) -> Self {
        Self::new(modes, count.into(), None)
    }

    /// Create a "choose up to X" or "choose one or more" modal effect.
    pub fn choose_up_to(
        max: impl Into<Value>,
        min: impl Into<Value>,
        modes: Vec<EffectMode>,
    ) -> Self {
        Self::new(modes, max.into(), Some(min.into()))
    }

    /// Allow selecting the same mode more than once.
    pub fn with_repeated_modes(mut self) -> Self {
        self.allow_repeated_modes = true;
        self
    }

    /// Require each activation to choose a mode that has not been chosen before.
    pub fn with_previously_unchosen_modes_only(mut self) -> Self {
        self.disallow_previously_chosen_modes = true;
        self.disallow_previously_chosen_modes_this_turn = false;
        self
    }

    /// Require each activation this turn to choose a mode not chosen earlier this turn.
    pub fn with_previously_unchosen_modes_only_this_turn(mut self) -> Self {
        self.disallow_previously_chosen_modes = true;
        self.disallow_previously_chosen_modes_this_turn = true;
        self
    }
}

impl EffectExecutor for ChooseModeEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        super::choose_mode_runtime::run_choose_mode(self, game, ctx)
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn get_modal_spec(&self) -> Option<ModalSpec> {
        Some(ModalSpec {
            mode_descriptions: self.modes.iter().map(|m| m.description.clone()).collect(),
            max_modes: self.choose_count.clone(),
            min_modes: self
                .min_choose_count
                .clone()
                .unwrap_or_else(|| self.choose_count.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::Ability;
    use crate::card::CardBuilder;
    use crate::cost::TotalCost;
    use crate::effect::Effect;
    use crate::effect::EffectResult;
    use crate::ids::CardId;
    use crate::ids::PlayerId;
    use crate::types::CardType;
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn make_mode(description: &str, effects: Vec<Effect>) -> EffectMode {
        EffectMode {
            description: description.to_string(),
            effects,
        }
    }

    #[test]
    fn test_choose_one_auto_first() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let initial_life = game.player(alice).unwrap().life;

        let effect = ChooseModeEffect::choose_one(vec![
            make_mode("Gain 5 life", vec![Effect::gain_life(5)]),
            make_mode("Gain 1 life", vec![Effect::gain_life(1)]),
        ]);
        effect.execute(&mut game, &mut ctx).unwrap();

        // Without decision maker, auto-selects first mode
        assert_eq!(game.player(alice).unwrap().life, initial_life + 5);
    }

    #[test]
    fn test_choose_mode_empty() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let effect = ChooseModeEffect::choose_one(vec![]);
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Resolved);
    }

    #[test]
    fn test_choose_mode_zero_count() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let initial_life = game.player(alice).unwrap().life;

        let effect = ChooseModeEffect::new(
            vec![make_mode("Gain 5 life", vec![Effect::gain_life(5)])],
            Value::Fixed(0),
            None,
        );
        effect.execute(&mut game, &mut ctx).unwrap();

        // No modes should execute
        assert_eq!(game.player(alice).unwrap().life, initial_life);
    }

    #[test]
    fn test_choose_up_to_auto_min() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        // Use AutoPassDecisionMaker to auto-select minimum count
        let mut dm = crate::decision::AutoPassDecisionMaker;
        let mut ctx = ExecutionContext::new_default(source, alice).with_decision_maker(&mut dm);

        let initial_life = game.player(alice).unwrap().life;

        // Choose one or both (min 1, max 2)
        let effect = ChooseModeEffect::choose_up_to(
            2,
            1,
            vec![
                make_mode("Gain 3 life", vec![Effect::gain_life(3)]),
                make_mode("Gain 2 life", vec![Effect::gain_life(2)]),
            ],
        );
        effect.execute(&mut game, &mut ctx).unwrap();

        // With AutoPassDecisionMaker, auto-selects first min (1) modes
        assert_eq!(game.player(alice).unwrap().life, initial_life + 3);
    }

    #[test]
    fn test_choose_mode_clone_box() {
        let effect =
            ChooseModeEffect::choose_one(vec![make_mode("Test", vec![Effect::gain_life(1)])]);
        let cloned = effect.clone_box();
        assert!(format!("{:?}", cloned).contains("ChooseModeEffect"));
    }

    #[test]
    fn test_choose_mode_disallow_previously_chosen_modes_tracks_per_ability() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source_card = CardBuilder::new(CardId::from_raw(1), "Modal Relic")
            .card_types(vec![CardType::Artifact])
            .build();
        let source = game.create_object_from_card(&source_card, alice, Zone::Battlefield);

        let choose_mode = ChooseModeEffect::choose_one(vec![
            make_mode("Gain 5 life.", vec![Effect::gain_life(5)]),
            make_mode("Gain 1 life.", vec![Effect::gain_life(1)]),
        ])
        .with_previously_unchosen_modes_only();
        game.object_mut(source).unwrap().abilities = vec![Ability::activated(
            TotalCost::default(),
            vec![Effect::new(choose_mode.clone())],
        )];

        let initial_life = game.player(alice).unwrap().life;

        // First use chooses mode 0.
        let mut ctx1 = ExecutionContext::new_default(source, alice);
        choose_mode.execute(&mut game, &mut ctx1).unwrap();
        assert_eq!(game.player(alice).unwrap().life, initial_life + 5);
        assert!(game.ability_mode_was_chosen(source, 0, 0, false));

        // Second use can no longer choose mode 0, so it chooses mode 1.
        let mut ctx2 = ExecutionContext::new_default(source, alice);
        choose_mode.execute(&mut game, &mut ctx2).unwrap();
        assert_eq!(game.player(alice).unwrap().life, initial_life + 6);
        assert!(game.ability_mode_was_chosen(source, 0, 1, false));

        // Third use has no legal modes left.
        let mut ctx3 = ExecutionContext::new_default(source, alice);
        let err = choose_mode.execute(&mut game, &mut ctx3).unwrap_err();
        assert!(matches!(err, ExecutionError::Impossible(_)));
    }
}
