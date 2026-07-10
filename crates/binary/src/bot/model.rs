use rand_game_common::fb::ResourceKind;

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

impl SmallCargo {
    pub(crate) fn to_map(&self) -> std::collections::HashMap<String, u32> {
        let mut map = std::collections::HashMap::new();
        if self.iron > 0 {
            map.insert("iron-ore".to_string(), self.iron);
        }
        if self.copper > 0 {
            map.insert("copper-ore".to_string(), self.copper);
        }
        if self.energy > 0 {
            map.insert("energy".to_string(), self.energy);
        }
        if self.stone > 0 {
            map.insert("stone".to_string(), self.stone);
        }
        if self.tree > 0 {
            map.insert("tree".to_string(), self.tree);
        }
        if self.water > 0 {
            map.insert("water".to_string(), self.water);
        }
        map
    }

    pub(crate) fn total_items(&self) -> u32 {
        self.iron + self.copper + self.energy + self.stone + self.tree + self.water
    }
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
        building_spec_id: String,
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
    Craft {
        recipe_id: String,
        target_building_id: u64,
    },
    #[allow(dead_code)]
    Research {
        research_id: String,
    },
}

pub(crate) struct PlannedAction {
    pub(crate) actor_id: u64,
    pub(crate) plan: ActionPlan,
}
