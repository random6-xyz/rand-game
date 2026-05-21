mod generation;
mod validation;

pub const WORLD_SEED: u64 = 0x5241_4e44_4741_4d45;
pub const MAP_ID: u32 = 0;
pub const OBSERVATION_RADIUS: u32 = 8;
pub const MAX_MINE_AMOUNT: u32 = 1;

pub use generation::generated_tile;
pub use validation::{validate_action, validate_game_output};
