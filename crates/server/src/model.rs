use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn manhattan(self, other: Self) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapKind {
    Resource,
    Hazard,
    Monster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceKind {
    Iron,
    Copper,
    Energy,
    Stone,
    Tree,
    Water,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceStack {
    pub kind: ResourceKind,
    pub amount: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum BuildingKind {
    None,
    Miner,
    Storage,
    Solar,
    Assembler,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidatedAction {
    Move {
        actor_entity_id: u64,
        target: Position,
    },
    Mine {
        actor_entity_id: u64,
        target: Position,
        amount: u32,
    },
    Build {
        actor_entity_id: u64,
        target: Position,
        building_kind: BuildingKind,
    },
    Lift {
        actor_entity_id: u64,
        kind: ResourceKind,
        amount: u32,
    },
    Put {
        actor_entity_id: u64,
        kind: ResourceKind,
        amount: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Building {
    pub id: u64,
    pub kind: BuildingKind,
    pub owner_id: u64,
    pub position: Position,
    pub power: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub id: u64,
    pub owner_id: u64,
    pub position: Position,
    pub cargo: Vec<ResourceStack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreTier {
    Basic,
    Standard,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProfile {
    pub run_interval_ticks: u64,
    pub cpu_time_ms: u32,
    pub wall_time_ms: u32,
    pub memory_bytes: u32,
    pub stdout_bytes: u32,
    pub stderr_bytes: u32,
    pub max_actions: u32,
    pub max_persistent_memory_bytes: u32,
}

impl CoreTier {
    pub const fn runtime_profile(self) -> RuntimeProfile {
        match self {
            Self::Basic => RuntimeProfile {
                run_interval_ticks: 5,
                cpu_time_ms: 50,
                wall_time_ms: 250,
                memory_bytes: 64 * 1024 * 1024,
                stdout_bytes: 64 * 1024,
                stderr_bytes: 64 * 1024,
                max_actions: 8,
                max_persistent_memory_bytes: 4096,
            },
            Self::Standard => RuntimeProfile {
                run_interval_ticks: 3,
                cpu_time_ms: 100,
                wall_time_ms: 400,
                memory_bytes: 96 * 1024 * 1024,
                stdout_bytes: 96 * 1024,
                stderr_bytes: 96 * 1024,
                max_actions: 16,
                max_persistent_memory_bytes: 8192,
            },
            Self::Advanced => RuntimeProfile {
                run_interval_ticks: 1,
                cpu_time_ms: 200,
                wall_time_ms: 750,
                memory_bytes: 128 * 1024 * 1024,
                stdout_bytes: 128 * 1024,
                stderr_bytes: 128 * 1024,
                max_actions: 32,
                max_persistent_memory_bytes: 16384,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: u64,
    pub core_entity_id: u64,
    pub worker_entity_id: u64,
    pub core_building_id: u64,
    pub core_tier: CoreTier,
    pub bot_path: PathBuf,
    pub persistent_memory: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub position: Position,
    pub resource: Option<ResourceStack>,
    pub building_id: Option<u64>,
    pub owner_id: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileOverride {
    pub resource: Option<Option<ResourceStack>>,
    pub owner_id: Option<Option<u64>>,
}
