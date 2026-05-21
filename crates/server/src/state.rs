use std::sync::Arc;

use tokio::sync::Mutex;

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
    pub config: ServerConfig,
}

#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
    pub addr: String,
    pub debug_max_actions: Option<u32>,
    pub log_bot_stderr: bool,
    pub env: ServerEnv,
    pub rules: ServerRules,
}

impl SharedState {
    pub fn new(world: WorldState, action_log: ActionLog, config: ServerConfig) -> Self {
        Self {
            inner: Arc::new(ServerState {
                world: Mutex::new(world),
                action_log: Mutex::new(action_log),
                config,
            }),
        }
    }

    pub fn inner(&self) -> &ServerState {
        &self.inner
    }
}
