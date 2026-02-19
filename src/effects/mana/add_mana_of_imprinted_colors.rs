//! Add mana of imprinted card's colors effect implementation.
//!
//! Used by Chrome Mox to produce mana based on the colors of the exiled card.

use super::choice_helpers::{choose_mana_colors, credit_mana_symbols};
use crate::color::Color;
use crate::effect::EffectOutcome;
use crate::effects::EffectExecutor;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::mana::ManaSymbol;

/// Effect that adds one mana of any of the imprinted card's colors.
///
/// If no card is imprinted, or the imprinted card is colorless, this does nothing.
/// If the imprinted card has multiple colors, the player chooses which color.
///
/// # Example
///
/// ```ignore
/// // Chrome Mox's mana ability
/// let effect = AddManaOfImprintedColorsEffect::new();
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct AddManaOfImprintedColorsEffect;

impl AddManaOfImprintedColorsEffect {
    /// Create a new add mana of imprinted colors effect.
    pub fn new() -> Self {
        Self
    }
}

impl Default for AddManaOfImprintedColorsEffect {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectExecutor for AddManaOfImprintedColorsEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let source_id = ctx.source;
        let controller = ctx.controller;

        // Get the imprinted cards
        let imprinted = game.get_imprinted_cards(source_id).to_vec();

        if imprinted.is_empty() {
            // No imprinted card - can't produce mana
            return Ok(EffectOutcome::count(0));
        }

        // Get colors from the first imprinted card (Chrome Mox only imprints one)
        let imprinted_id = imprinted[0];
        let colors: Vec<Color> = game
            .object(imprinted_id)
            .map(|obj| {
                let color_set = obj.colors();
                let mut colors = Vec::new();
                if color_set.contains(Color::White) {
                    colors.push(Color::White);
                }
                if color_set.contains(Color::Blue) {
                    colors.push(Color::Blue);
                }
                if color_set.contains(Color::Black) {
                    colors.push(Color::Black);
                }
                if color_set.contains(Color::Red) {
                    colors.push(Color::Red);
                }
                if color_set.contains(Color::Green) {
                    colors.push(Color::Green);
                }
                colors
            })
            .unwrap_or_default();

        if colors.is_empty() {
            // Imprinted card is colorless - can't produce mana
            return Ok(EffectOutcome::count(0));
        }

        let chosen_color =
            choose_mana_colors(game, ctx, controller, 1, true, Some(&colors), colors[0])
                .into_iter()
                .next()
                .unwrap_or(colors[0]);
        credit_mana_symbols(game, controller, [ManaSymbol::from_color(chosen_color)]);

        Ok(EffectOutcome::count(1))
    }

    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn producible_mana_symbols(
        &self,
        game: &GameState,
        source: crate::ids::ObjectId,
        _controller: crate::ids::PlayerId,
    ) -> Option<Vec<ManaSymbol>> {
        let imprinted_id = *game.get_imprinted_cards(source).first()?;
        let color_set = game.object(imprinted_id)?.colors();

        let mut symbols = Vec::new();
        if color_set.contains(Color::White) {
            symbols.push(ManaSymbol::White);
        }
        if color_set.contains(Color::Blue) {
            symbols.push(ManaSymbol::Blue);
        }
        if color_set.contains(Color::Black) {
            symbols.push(ManaSymbol::Black);
        }
        if color_set.contains(Color::Red) {
            symbols.push(ManaSymbol::Red);
        }
        if color_set.contains(Color::Green) {
            symbols.push(ManaSymbol::Green);
        }
        if symbols.is_empty() {
            return None;
        }
        Some(symbols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::CardBuilder;
    use crate::effect::EffectResult;
    use crate::ids::{CardId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::types::CardType;
    use crate::zone::Zone;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn create_chrome_mox(game: &mut GameState, owner: PlayerId) -> crate::ids::ObjectId {
        let card = CardBuilder::new(CardId::new(), "Chrome Mox")
            .mana_cost(ManaCost::new())
            .card_types(vec![CardType::Artifact])
            .build();
        game.create_object_from_card(&card, owner, Zone::Battlefield)
    }

    fn create_red_card(game: &mut GameState, owner: PlayerId) -> crate::ids::ObjectId {
        let card = CardBuilder::new(CardId::new(), "Lightning Bolt")
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Red]]))
            .card_types(vec![CardType::Instant])
            .build();
        game.create_object_from_card(&card, owner, Zone::Exile)
    }

    fn create_colorless_card(game: &mut GameState, owner: PlayerId) -> crate::ids::ObjectId {
        let card = CardBuilder::new(CardId::new(), "Sol Ring")
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::Generic(1)]]))
            .card_types(vec![CardType::Artifact])
            .build();
        game.create_object_from_card(&card, owner, Zone::Exile)
    }

    fn create_multicolor_card(game: &mut GameState, owner: PlayerId) -> crate::ids::ObjectId {
        let card = CardBuilder::new(CardId::new(), "Rakdos Charm")
            .mana_cost(ManaCost::from_pips(vec![
                vec![ManaSymbol::Black],
                vec![ManaSymbol::Red],
            ]))
            .card_types(vec![CardType::Instant])
            .build();
        game.create_object_from_card(&card, owner, Zone::Exile)
    }

    #[test]
    fn test_no_imprinted_card() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let mox = create_chrome_mox(&mut game, alice);

        let mut ctx = ExecutionContext::new_default(mox, alice);
        let effect = AddManaOfImprintedColorsEffect::new();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        // No imprinted card - no mana
        assert_eq!(result.result, EffectResult::Count(0));
        assert_eq!(game.player(alice).unwrap().mana_pool.total(), 0);
    }

    #[test]
    fn test_imprinted_colorless_card() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let mox = create_chrome_mox(&mut game, alice);
        let sol_ring = create_colorless_card(&mut game, alice);

        // Imprint the colorless card
        game.imprint_card(mox, sol_ring);

        let mut ctx = ExecutionContext::new_default(mox, alice);
        let effect = AddManaOfImprintedColorsEffect::new();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        // Colorless card - no mana
        assert_eq!(result.result, EffectResult::Count(0));
        assert_eq!(game.player(alice).unwrap().mana_pool.total(), 0);
    }

    #[test]
    fn test_imprinted_red_card() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let mox = create_chrome_mox(&mut game, alice);
        let bolt = create_red_card(&mut game, alice);

        // Imprint the red card
        game.imprint_card(mox, bolt);

        let mut ctx = ExecutionContext::new_default(mox, alice);
        let effect = AddManaOfImprintedColorsEffect::new();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        // Red card - adds red mana
        assert_eq!(result.result, EffectResult::Count(1));
        assert_eq!(game.player(alice).unwrap().mana_pool.red, 1);
    }

    #[test]
    fn test_imprinted_multicolor_card_defaults_to_first() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let mox = create_chrome_mox(&mut game, alice);
        let rakdos_charm = create_multicolor_card(&mut game, alice);

        // Imprint the multicolor card
        game.imprint_card(mox, rakdos_charm);

        let mut ctx = ExecutionContext::new_default(mox, alice);
        let effect = AddManaOfImprintedColorsEffect::new();
        let result = effect.execute(&mut game, &mut ctx).unwrap();

        // Multicolor - defaults to first color (black comes before red)
        assert_eq!(result.result, EffectResult::Count(1));
        // Should have one mana of either black or red
        let pool = &game.player(alice).unwrap().mana_pool;
        assert_eq!(pool.black + pool.red, 1);
    }

    #[test]
    fn test_clone_box() {
        let effect = AddManaOfImprintedColorsEffect::new();
        let cloned = effect.clone_box();
        assert!(format!("{:?}", cloned).contains("AddManaOfImprintedColorsEffect"));
    }
}
