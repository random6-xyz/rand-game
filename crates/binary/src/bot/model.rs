use rand_game_common::fb::{BuildingKind, ResourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Position {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResourceTile {
    pub(crate) position: Position,
    pub(crate) resource: ResourceKind,
    pub(crate) amount: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct Actor {
    pub(crate) id: u64,
    pub(crate) position: Position,
    #[allow(dead_code)]
    pub(crate) cargo: SmallCargo,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SmallCargo {
    pub(crate) iron: u32,
    pub(crate) copper: u32,
    pub(crate) energy: u32,
    pub(crate) stone: u32,
    pub(crate) tree: u32,
    pub(crate) water: u32,
}

pub(crate) enum ActionPlan {
    Mine {
        target: Position,
        resource: ResourceKind,
        amount: u32,
    },
    Move {
        target: Position,
    },
    #[allow(dead_code)]
    Build {
        target: Position,
        building_kind: BuildingKind,
    },
    #[allow(dead_code)]
    Lift {
        resource: ResourceKind,
        amount: u32,
    },
    #[allow(dead_code)]
    Put {
        resource: ResourceKind,
        amount: u32,
    },
    #[allow(dead_code)]
    Craft {
        recipe_id: String,
        target_building_id: u64,
    },
}

pub(crate) struct PlannedAction {
    pub(crate) actor_id: u64,
    pub(crate) plan: ActionPlan,
}
