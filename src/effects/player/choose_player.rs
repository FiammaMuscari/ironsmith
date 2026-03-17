use crate::effect::EffectOutcome;
use crate::effects::EffectExecutor;
use crate::effects::helpers::{resolve_player_filter, resolve_player_filter_to_list};
use crate::executor::{ExecutionContext, ExecutionError};
use crate::game_state::GameState;
use crate::ids::PlayerId;
use crate::target::{FilterContext, PlayerFilter};

#[derive(Debug, Clone, PartialEq)]
pub struct ChoosePlayerEffect {
    pub chooser: PlayerFilter,
    pub filter: PlayerFilter,
    pub tag: crate::tag::TagKey,
    pub random: bool,
    pub excluded_tags: Vec<crate::tag::TagKey>,
}

impl ChoosePlayerEffect {
    pub fn new(
        chooser: PlayerFilter,
        filter: PlayerFilter,
        tag: impl Into<crate::tag::TagKey>,
    ) -> Self {
        Self {
            chooser,
            filter,
            tag: tag.into(),
            random: false,
            excluded_tags: Vec::new(),
        }
    }

    pub fn at_random(mut self) -> Self {
        self.random = true;
        self
    }

    pub fn excluding_tags(mut self, excluded_tags: Vec<crate::tag::TagKey>) -> Self {
        self.excluded_tags = excluded_tags;
        self
    }

    fn candidate_players(
        &self,
        game: &GameState,
        ctx: &ExecutionContext,
    ) -> Result<Vec<PlayerId>, ExecutionError> {
        let mut filter_ctx: FilterContext = ctx.filter_context(game);
        for excluded_tag in &self.excluded_tags {
            if let Some(players) = ctx.get_tagged_players(excluded_tag.as_str()) {
                let filtered = filter_ctx
                    .tagged_players
                    .remove(excluded_tag)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|player| !players.contains(player))
                    .collect::<Vec<_>>();
                if !filtered.is_empty() {
                    filter_ctx
                        .tagged_players
                        .insert(excluded_tag.clone(), filtered);
                }
            }
        }
        let mut players = resolve_player_filter_to_list(game, &self.filter, &filter_ctx, ctx)?;
        for excluded_tag in &self.excluded_tags {
            if let Some(excluded_players) = ctx.get_tagged_players(excluded_tag.as_str()) {
                players.retain(|player| !excluded_players.contains(player));
            }
        }
        players.sort_by_key(|player| player.0);
        players.dedup();
        Ok(players)
    }
}

impl EffectExecutor for ChoosePlayerEffect {
    fn clone_box(&self) -> Box<dyn EffectExecutor> {
        Box::new(self.clone())
    }

    fn execute(
        &self,
        game: &mut GameState,
        ctx: &mut ExecutionContext,
    ) -> Result<EffectOutcome, ExecutionError> {
        let chooser = resolve_player_filter(game, &self.chooser, ctx)?;
        let candidates = self.candidate_players(game, ctx)?;
        let Some(chosen) = (if self.random {
            let mut shuffled = candidates.clone();
            game.shuffle_slice(&mut shuffled);
            shuffled.first().copied()
        } else {
            let options = candidates
                .iter()
                .filter_map(|player_id| {
                    game.player(*player_id)
                        .map(|player| (player.name.clone(), *player_id))
                })
                .collect::<Vec<_>>();
            (!options.is_empty()).then_some(crate::decisions::ask_choose_one(
                game,
                &mut ctx.decision_maker,
                chooser,
                ctx.source,
                &options,
            ))
        }) else {
            return Ok(EffectOutcome::resolved());
        };

        ctx.set_tagged_players(self.tag.clone(), vec![chosen]);
        if self.tag.as_str() != "__it__" {
            // Mirror the most recent chosen player onto the conventional follow-up
            // tag so clauses like "that player ..." resolve against the new choice.
            ctx.set_tagged_players(crate::tag::TagKey::from("__it__"), vec![chosen]);
        }
        Ok(EffectOutcome::count(1))
    }
}
