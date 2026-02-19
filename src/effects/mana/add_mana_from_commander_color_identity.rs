//! Add mana from commander color identity effect implementation.

use super::choice_helpers::{choose_mana_colors, credit_repeated_mana_symbol};
use crate::color::Color;
use crate::effect::{EffectOutcome, Value};
use crate::effects::EffectExecutor;
use crate::effects::helpers::{resolve_player_filter, resolve_value};
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::mana::ManaSymbol;
use crate::target::PlayerFilter;

/// Effect that adds mana of any color in the player's commander's color identity.
///
/// Used by cards like Arcane Signet and Command Tower. If the commander's color
/// identity is colorless (or there is no commander), adds colorless mana instead.
///
/// # Fields
///
/// * `amount` - Number of mana to add
/// * `player` - Which player receives the mana
///
/// # Example
///
/// ```ignore
/// // Arcane Signet: Tap to add one mana of any color in your commander's identity
/// let effect = AddManaFromCommanderColorIdentityEffect::you(1);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct AddManaFromCommanderColorIdentityEffect {
    /// Number of mana to add.
    pub amount: Value,
    /// Which player receives the mana.
    pub player: PlayerFilter,
}

impl AddManaFromCommanderColorIdentityEffect {
    /// Create a new add mana from commander color identity effect.
    pub fn new(amount: impl Into<Value>, player: PlayerFilter) -> Self {
        Self {
            amount: amount.into(),
            player,
        }
    }

    /// Create an effect where you add mana from your commander's color identity.
    pub fn you(amount: impl Into<Value>) -> Self {
        Self::new(amount, PlayerFilter::You)
    }
}

impl EffectExecutor for AddManaFromCommanderColorIdentityEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let player_id = resolve_player_filter(game, &self.player, ctx)?;
        let amount = resolve_value(game, &self.amount, ctx)?.max(0) as u32;

        if amount == 0 {
            return Ok(EffectOutcome::count(0));
        }

        // Get the commander's color identity
        let color_identity = game.get_commander_color_identity(player_id);

        // If colorless identity, add colorless mana
        if color_identity.is_empty() {
            credit_repeated_mana_symbol(game, player_id, ManaSymbol::Colorless, amount);
            return Ok(EffectOutcome::count(amount as i32));
        }

        // Build list of available colors from identity
        let mut available_colors = Vec::new();
        if color_identity.contains(Color::White) {
            available_colors.push(Color::White);
        }
        if color_identity.contains(Color::Blue) {
            available_colors.push(Color::Blue);
        }
        if color_identity.contains(Color::Black) {
            available_colors.push(Color::Black);
        }
        if color_identity.contains(Color::Red) {
            available_colors.push(Color::Red);
        }
        if color_identity.contains(Color::Green) {
            available_colors.push(Color::Green);
        }

        let color = choose_mana_colors(
            game,
            ctx,
            player_id,
            1,
            true,
            Some(&available_colors),
            available_colors[0],
        )
        .into_iter()
        .next()
        .unwrap_or(available_colors[0]);

        credit_repeated_mana_symbol(game, player_id, ManaSymbol::from_color(color), amount);

        Ok(EffectOutcome::count(amount as i32))
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn producible_mana_symbols(
        &self,
        game: &GameState,
        _source: crate::ids::ObjectId,
        controller: crate::ids::PlayerId,
    ) -> Option<Vec<ManaSymbol>> {
        let identity = game.get_commander_color_identity(controller);
        if identity.is_empty() {
            return Some(vec![ManaSymbol::Colorless]);
        }

        let mut symbols = Vec::new();
        if identity.contains(Color::White) {
            symbols.push(ManaSymbol::White);
        }
        if identity.contains(Color::Blue) {
            symbols.push(ManaSymbol::Blue);
        }
        if identity.contains(Color::Black) {
            symbols.push(ManaSymbol::Black);
        }
        if identity.contains(Color::Red) {
            symbols.push(ManaSymbol::Red);
        }
        if identity.contains(Color::Green) {
            symbols.push(ManaSymbol::Green);
        }
        Some(symbols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::effect::EffectResult;
    use crate::ids::{CardId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::object::Object;
    use crate::types::CardType;
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn setup_commander(game: &mut GameState, player: PlayerId, colors: Vec<ManaSymbol>) {
        // Create a commander with the given colors
        let commander_card = CardBuilder::new(CardId::new(), "Test Commander")
            .mana_cost(ManaCost::from_pips(
                colors.into_iter().map(|s| vec![s]).collect(),
            ))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(3, 3))
            .build();

        let id = game.new_object_id();
        let obj = Object::from_card(id, &commander_card, player, Zone::Command);
        game.add_object(obj);

        if let Some(p) = game.player_mut(player) {
            p.add_commander(id);
        }
    }

    #[test]
    fn test_add_mana_from_commander_no_commander() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        // No commander set, should add colorless mana
        let effect = AddManaFromCommanderColorIdentityEffect::you(2);
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(2));
        assert_eq!(game.player(alice).unwrap().mana_pool.colorless, 2);
    }

    #[test]
    fn test_add_mana_from_commander_single_color() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        // Create a mono-white commander
        setup_commander(&mut game, alice, vec![ManaSymbol::White]);

        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let effect = AddManaFromCommanderColorIdentityEffect::you(1);
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(1));
        // Should add white (only color in identity)
        assert_eq!(game.player(alice).unwrap().mana_pool.white, 1);
    }

    #[test]
    fn test_add_mana_from_commander_two_colors() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);

        // Create a white/black commander
        setup_commander(&mut game, alice, vec![ManaSymbol::White, ManaSymbol::Black]);

        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let effect = AddManaFromCommanderColorIdentityEffect::you(2);
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(2));
        // Should add first available color (white) without decision maker
        assert_eq!(game.player(alice).unwrap().mana_pool.white, 2);
    }

    #[test]
    fn test_add_mana_from_commander_zero() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = game.new_object_id();
        let mut ctx = ExecutionContext::new_default(source, alice);

        let effect = AddManaFromCommanderColorIdentityEffect::you(0);
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(result.result, EffectResult::Count(0));
    }

    #[test]
    fn test_add_mana_from_commander_clone_box() {
        let effect = AddManaFromCommanderColorIdentityEffect::you(1);
        let cloned = effect.clone_box();
        assert!(format!("{:?}", cloned).contains("AddManaFromCommanderColorIdentityEffect"));
    }
}
