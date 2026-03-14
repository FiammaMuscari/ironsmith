//! Attach to effect implementation.

use crate::effect::EffectOutcome;
use crate::effects::EffectExecutor;
use crate::effects::helpers::resolve_single_object_for_effect;
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::target::ChooseSpec;
use crate::zone::Zone;

/// Effect that attaches the source permanent to a target permanent.
///
/// Used primarily by Auras that grant control or Equipment that auto-attach.
/// The source becomes attached to the target, and the target gains
/// the source in its attachments list.
///
/// # Fields
///
/// * `target` - The target specification for what to attach to
///
/// # Example
///
/// ```ignore
/// // Create an attach effect for an aura
/// let effect = AttachToEffect::new(ChooseSpec::target_creature());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct AttachToEffect {
    /// The target to attach to.
    pub target: ChooseSpec,
}

impl AttachToEffect {
    /// Create a new attach to effect.
    pub fn new(target: ChooseSpec) -> Self {
        Self { target }
    }
}

impl EffectExecutor for AttachToEffect {
    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let target_id = resolve_single_object_for_effect(game, ctx, &self.target)?;

        // If this is a spell on the stack (Aura resolving), defer attachment
        if let Some(source) = game.object(ctx.source)
            && source.zone == Zone::Stack
        {
            return Ok(EffectOutcome::resolved());
        }

        // Detach from previous parent if needed.
        let previous_parent = game
            .object(ctx.source)
            .and_then(|source| source.attached_to);
        if let Some(previous_parent) = previous_parent
            && previous_parent != target_id
            && let Some(parent) = game.object_mut(previous_parent)
        {
            parent.attachments.retain(|id| *id != ctx.source);
        }

        // Attach the source to the target
        if let Some(source) = game.object_mut(ctx.source) {
            source.attached_to = Some(target_id);
        }

        if let Some(target) = game.object_mut(target_id)
            && !target.attachments.contains(&ctx.source)
        {
            target.attachments.push(ctx.source);
        }
        game.continuous_effects.record_attachment(ctx.source);

        Ok(EffectOutcome::resolved())
    }

    fn get_target_spec(&self) -> Option<&ChooseSpec> {
        Some(&self.target)
    }

    fn target_description(&self) -> &'static str {
        "target to attach to"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardBuilder, PowerToughness};
    use crate::executor::ResolvedTarget;
    use crate::ids::{CardId, ObjectId, PlayerId};
    use crate::mana::{ManaCost, ManaSymbol};
    use crate::object::Object;
    use crate::snapshot::ObjectSnapshot;
    use crate::types::{CardType, Subtype};

    fn setup_game() -> GameState {
        crate::tests::test_helpers::setup_two_player_game()
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

    fn make_aura_card(card_id: u32, name: &str) -> crate::card::Card {
        CardBuilder::new(CardId::from_raw(card_id), name)
            .mana_cost(ManaCost::from_pips(vec![vec![ManaSymbol::White]]))
            .card_types(vec![CardType::Enchantment])
            .subtypes(vec![Subtype::Aura])
            .build()
    }

    fn create_creature(game: &mut GameState, name: &str, controller: PlayerId) -> ObjectId {
        let id = game.new_object_id();
        let card = make_creature_card(id.0 as u32, name);
        let obj = Object::from_card(id, &card, controller, Zone::Battlefield);
        game.add_object(obj);
        id
    }

    fn create_aura(game: &mut GameState, name: &str, controller: PlayerId) -> ObjectId {
        let id = game.new_object_id();
        let card = make_aura_card(id.0 as u32, name);
        let obj = Object::from_card(id, &card, controller, Zone::Battlefield);
        game.add_object(obj);
        id
    }

    #[test]
    fn test_attach_to_target_from_ctx_targets() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = create_aura(&mut game, "Pacifism", alice);
        let target = create_creature(&mut game, "Bear", alice);
        let mut ctx = ExecutionContext::new_default(source, alice)
            .with_targets(vec![ResolvedTarget::Object(target)]);

        let effect = AttachToEffect::new(ChooseSpec::target_creature());
        effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(game.object(source).unwrap().attached_to, Some(target));
        assert!(game.object(target).unwrap().attachments.contains(&source));
    }

    #[test]
    fn test_attach_to_tagged_target_without_ctx_targets() {
        let mut game = setup_game();
        let alice = PlayerId::from_index(0);
        let source = create_aura(&mut game, "Pacifism", alice);
        let target = create_creature(&mut game, "Bear", alice);
        let mut ctx = ExecutionContext::new_default(source, alice);
        let snapshot = ObjectSnapshot::from_object(game.object(target).unwrap(), &game);
        ctx.tag_object("attach_target", snapshot);

        let effect = AttachToEffect::new(ChooseSpec::Tagged("attach_target".into()));
        effect.execute(&mut game, &mut ctx).unwrap();

        assert_eq!(game.object(source).unwrap().attached_to, Some(target));
        assert!(game.object(target).unwrap().attachments.contains(&source));
    }
}
