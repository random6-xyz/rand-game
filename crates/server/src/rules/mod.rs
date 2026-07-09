mod generation;
mod validation;

use crate::model::{CoreTier, RuntimeProfile};

pub const WORLD_SEED: u64 = 0x5241_4e44_4741_4d45;
pub const MAP_ID: u32 = 0;
pub const OBSERVATION_RADIUS: u32 = 8;
pub const MAX_MINE_AMOUNT: u32 = 1;
pub const RULESET_VERSION: u32 = 1;
pub const TICK_INTERVAL_MS: u64 = 1000;
pub const MAX_WORLD_RADIUS: i32 = 16;
pub const DEFAULT_WORLD_RADIUS: i32 = 4;
pub const MAX_DEBUG_MAP_VIEW_RADIUS: i32 = 128;
pub const MAX_BOT_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const BUILD_CORE_RADIUS: u32 = 4;
pub const BOT_STDERR_CHANNEL_CAPACITY: usize = 128;
pub const MAX_ACTION_LOG_ENTRIES: usize = 10000;
pub const ACTION_LOG_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ServerEnv {
    pub world_seed: u64,
    pub map_id: u32,
}

impl Default for ServerEnv {
    fn default() -> Self {
        Self {
            world_seed: WORLD_SEED,
            map_id: MAP_ID,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ServerRules {
    pub ruleset_version: u32,
    pub tick_interval_ms: u64,
    pub observation_radius: u32,
    pub max_mine_amount: u32,
    pub max_world_radius: i32,
    pub max_map_view_radius: i32,
    pub max_debug_map_view_radius: i32,
    pub default_world_radius: i32,
    pub max_bot_upload_bytes: usize,
    pub build_core_radius: u32,
    pub bot_stderr_channel_capacity: usize,
    pub max_action_log_entries: usize,
    pub action_log_page_size: usize,
    pub basic_core: RuntimeProfileConfig,
    pub standard_core: RuntimeProfileConfig,
    pub advanced_core: RuntimeProfileConfig,
}

impl Default for ServerRules {
    fn default() -> Self {
        Self {
            ruleset_version: RULESET_VERSION,
            tick_interval_ms: TICK_INTERVAL_MS,
            observation_radius: OBSERVATION_RADIUS,
            max_mine_amount: MAX_MINE_AMOUNT,
            max_world_radius: MAX_WORLD_RADIUS,
            max_map_view_radius: MAX_WORLD_RADIUS,
            max_debug_map_view_radius: MAX_DEBUG_MAP_VIEW_RADIUS,
            default_world_radius: DEFAULT_WORLD_RADIUS,
            max_bot_upload_bytes: MAX_BOT_UPLOAD_BYTES,
            build_core_radius: BUILD_CORE_RADIUS,
            bot_stderr_channel_capacity: BOT_STDERR_CHANNEL_CAPACITY,
            max_action_log_entries: MAX_ACTION_LOG_ENTRIES,
            action_log_page_size: ACTION_LOG_PAGE_SIZE,
            basic_core: RuntimeProfileConfig::basic(),
            standard_core: RuntimeProfileConfig::standard(),
            advanced_core: RuntimeProfileConfig::advanced(),
        }
    }
}

impl ServerRules {
    pub fn runtime_profile(&self, tier: CoreTier) -> RuntimeProfile {
        match tier {
            CoreTier::Basic => self.basic_core.into(),
            CoreTier::Standard => self.standard_core.into(),
            CoreTier::Advanced => self.advanced_core.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RuntimeProfileConfig {
    pub run_interval_ticks: u64,
    pub cpu_time_ms: u32,
    pub wall_time_ms: u32,
    pub memory_bytes: u32,
    pub stdout_bytes: u32,
    pub stderr_bytes: u32,
    pub max_actions: u32,
    pub max_persistent_memory_bytes: u32,
}

impl RuntimeProfileConfig {
    pub const fn basic() -> Self {
        Self {
            run_interval_ticks: 5,
            cpu_time_ms: 50,
            wall_time_ms: 250,
            memory_bytes: 64 * 1024 * 1024,
            stdout_bytes: 64 * 1024,
            stderr_bytes: 64 * 1024,
            max_actions: 8,
            max_persistent_memory_bytes: 4096,
        }
    }

    pub const fn standard() -> Self {
        Self {
            run_interval_ticks: 3,
            cpu_time_ms: 100,
            wall_time_ms: 400,
            memory_bytes: 96 * 1024 * 1024,
            stdout_bytes: 96 * 1024,
            stderr_bytes: 96 * 1024,
            max_actions: 16,
            max_persistent_memory_bytes: 8192,
        }
    }

    pub const fn advanced() -> Self {
        Self {
            run_interval_ticks: 1,
            cpu_time_ms: 200,
            wall_time_ms: 750,
            memory_bytes: 128 * 1024 * 1024,
            stdout_bytes: 128 * 1024,
            stderr_bytes: 128 * 1024,
            max_actions: 32,
            max_persistent_memory_bytes: 16384,
        }
    }
}

impl Default for RuntimeProfileConfig {
    fn default() -> Self {
        Self::basic()
    }
}

impl From<RuntimeProfileConfig> for RuntimeProfile {
    fn from(profile: RuntimeProfileConfig) -> Self {
        Self {
            run_interval_ticks: profile.run_interval_ticks,
            cpu_time_ms: profile.cpu_time_ms,
            wall_time_ms: profile.wall_time_ms,
            memory_bytes: profile.memory_bytes,
            stdout_bytes: profile.stdout_bytes,
            stderr_bytes: profile.stderr_bytes,
            max_actions: profile.max_actions,
            max_persistent_memory_bytes: profile.max_persistent_memory_bytes,
        }
    }
}

pub use generation::generated_tile;
pub use validation::{validate_action, validate_game_output};
