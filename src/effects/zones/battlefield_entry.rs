use crate::executor::ExecutionContext;
use crate::game_state::GameState;
use crate::ids::{ObjectId, PlayerId};
use crate::zone::Zone;

/// Controller policy when an object enters the battlefield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BattlefieldEntryController {
    Preserve,
    Owner,
    Specific(PlayerId),
}

/// Config for moving an object to the battlefield through ETB processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BattlefieldEntryOptions {
    pub controller: BattlefieldEntryController,
    pub tapped: bool,
}

impl BattlefieldEntryOptions {
    pub(crate) fn preserve(tapped: bool) -> Self {
        Self {
            controller: BattlefieldEntryController::Preserve,
            tapped,
        }
    }

    pub(crate) fn owner(tapped: bool) -> Self {
        Self {
            controller: BattlefieldEntryController::Owner,
            tapped,
        }
    }

    pub(crate) fn specific(controller: PlayerId, tapped: bool) -> Self {
        Self {
            controller: BattlefieldEntryController::Specific(controller),
            tapped,
        }
    }
}

/// Result for a move-to-battlefield attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BattlefieldEntryOutcome {
    Moved(ObjectId),
    Prevented,
}

/// Move an object to the battlefield with ETB replacement processing and policy hooks.
pub(crate) fn move_to_battlefield_with_options(
    game: &mut GameState,
    ctx: &mut ExecutionContext,
    object_id: ObjectId,
    options: BattlefieldEntryOptions,
) -> BattlefieldEntryOutcome {
    let Some(result) = game.move_object_with_etb_processing_with_dm(
        object_id,
        Zone::Battlefield,
        &mut ctx.decision_maker,
    ) else {
        return BattlefieldEntryOutcome::Prevented;
    };

    let new_id = result.new_id;

    if let Some(obj) = game.object_mut(new_id) {
        match options.controller {
            BattlefieldEntryController::Preserve => {}
            BattlefieldEntryController::Owner => {
                obj.controller = obj.owner;
            }
            BattlefieldEntryController::Specific(controller) => {
                obj.controller = controller;
            }
        }
    }

    if options.tapped && !result.enters_tapped {
        game.tap(new_id);
    }

    BattlefieldEntryOutcome::Moved(new_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::ids::{CardId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::object::Object;
    use crate::types::CardType;

    fn setup_game() -> GameState {
        GameState::new(vec!["Alice".to_string(), "Bob".to_string()], 20)
    }

    fn make_creature_card(card_id: u32, name: &str) -> crate::card::Card {
        CardBuilder::new(CardId::from_raw(card_id), name)
            .mana_cost(ManaCost::from_pips(vec![
                vec![ManaSymbol::Generic(1)],
                vec![ManaSymbol::Green],
            ]))
            .card_types(vec![CardType::Creature])
            .power_toughness(PowerToughness::fixed(2, 2))
            .build()
    }

    fn create_creature_in_graveyard(game: &mut GameState, owner: PlayerId) -> ObjectId {
        let id = game.new_object_id();
        let card = make_creature_card(id.0 as u32, "Grizzly Bears");
        let obj = Object::from_card(id, &card, owner, Zone::Graveyard);
        game.add_object(obj);
        id
    }

    #[test]
    fn test_move_to_battlefield_with_options_sets_specific_controller_and_tapped() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);
        let source = game.new_object_id();
        let target = create_creature_in_graveyard(&mut game, bob);
        let mut ctx = ExecutionContext::new_default(source, alice);

        let outcome = move_to_battlefield_with_options(
            &mut game,
            &mut ctx,
            target,
            BattlefieldEntryOptions::specific(alice, true),
        );

        let BattlefieldEntryOutcome::Moved(new_id) = outcome else {
            panic!("expected successful move");
        };

        let obj = game.object(new_id).expect("moved object should exist");
        assert_eq!(obj.controller, alice);
        assert!(game.is_tapped(new_id));
    }

    #[test]
    fn test_move_to_battlefield_with_options_owner_control() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let bob = PlayerId::from_index(1);
        let source = game.new_object_id();
        let target = create_creature_in_graveyard(&mut game, bob);
        let mut ctx = ExecutionContext::new_default(source, alice);

        let outcome = move_to_battlefield_with_options(
            &mut game,
            &mut ctx,
            target,
            BattlefieldEntryOptions::owner(false),
        );

        let BattlefieldEntryOutcome::Moved(new_id) = outcome else {
            panic!("expected successful move");
        };

        let obj = game.object(new_id).expect("moved object should exist");
        assert_eq!(obj.controller, bob);
        assert!(!game.is_tapped(new_id));
    }
}
