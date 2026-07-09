use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{Mutex, broadcast};

use crate::action_log::ActionLog;
use crate::rules::{ServerEnv, ServerRules};
use crate::world::WorldState;

#[derive(Clone)]
pub struct SharedState {
    inner: Arc<ServerState>,
}

pub struct ServerState {
    pub world: Mutex<WorldState>,
    pub action_log: Mutex<ActionLog>,
    pub bot_stderr: broadcast::Sender<BotStderrEvent>,
    pub config: ServerConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct BotStderrEvent {
    pub tick: u64,
    pub player_id: u64,
    pub bot_path: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub addr: String,
    pub debug_max_actions: Option<u32>,
    pub log_bot_stderr: bool,
    pub env: ServerEnv,
    pub rules: ServerRules,
    pub rule_catalog: rand_game_common::rules::RuleCatalog,
}

impl SharedState {
    pub fn new(world: WorldState, action_log: ActionLog, config: ServerConfig) -> Self {
        let (bot_stderr, _) = broadcast::channel(config.rules.bot_stderr_channel_capacity.max(1));
        Self {
            inner: Arc::new(ServerState {
                world: Mutex::new(world),
                action_log: Mutex::new(action_log),
                bot_stderr,
                config,
            }),
        }
    }

    pub fn inner(&self) -> &ServerState {
        &self.inner
    }
}
