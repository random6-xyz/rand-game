use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

mod cargo;

use crate::model::{
    Building, BuildingKind, CoreTier, Entity, ItemStack, MapKind, Player, Position, ResourceKind,
    ResourceStack, Tile, TileOverride, ValidatedAction,
};
use crate::rules::{self, ServerEnv, ServerRules};

use self::cargo::{add_cargo, remove_cargo};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct WorldState {
    pub world_seed: u64,
    pub map_id: u32,
    pub tick: u64,
    pub observation_radius: u32,
    pub players: HashMap<u64, Player>,
    pub entities: HashMap<u64, Entity>,
    pub buildings: HashMap<u64, Building>,
    tile_overrides: HashMap<Position, TileOverride>,
    next_id: u64,
}

pub(crate) struct WorldStateParts {
    pub world_seed: u64,
    pub map_id: u32,
    pub tick: u64,
    pub observation_radius: u32,
    pub players: HashMap<u64, Player>,
    pub entities: HashMap<u64, Entity>,
    pub buildings: HashMap<u64, Building>,
    pub tile_overrides: HashMap<Position, TileOverride>,
    pub next_id: u64,
}

impl WorldState {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::new_with_config(&ServerEnv::default(), &ServerRules::default())
    }

    pub fn new_with_config(env: &ServerEnv, rules: &ServerRules) -> Self {
        let mut world = Self {
            world_seed: env.world_seed,
            map_id: env.map_id,
            tick: 0,
            observation_radius: rules.observation_radius,
            players: HashMap::new(),
            entities: HashMap::new(),
            buildings: HashMap::new(),
            tile_overrides: HashMap::new(),
            next_id: 1,
        };

        world.spawn_initial_player();
        world
    }

    pub fn tile_at(&self, position: Position) -> Tile {
        let mut tile =
            rules::generated_tile(self.world_seed, self.map_id, self.map_kind(), position);

        if let Some(change) = self.tile_overrides.get(&position) {
            if let Some(resource) = change.resource {
                tile.resource = resource;
            }
            if let Some(owner_id) = change.owner_id {
                tile.owner_id = owner_id;
            }
        }

        if let Some(building) = self.building_at(position) {
            tile.building_id = Some(building.id);
            tile.owner_id = Some(building.owner_id);
        }

        tile
    }

    pub fn stored_tile_change_count(&self) -> usize {
        self.tile_overrides.len()
    }

    pub(crate) fn from_parts(parts: WorldStateParts) -> Self {
        Self {
            world_seed: parts.world_seed,
            map_id: parts.map_id,
            tick: parts.tick,
            observation_radius: parts.observation_radius,
            players: parts.players,
            entities: parts.entities,
            buildings: parts.buildings,
            tile_overrides: parts.tile_overrides,
            next_id: parts.next_id,
        }
    }

    pub(crate) fn tile_overrides(&self) -> &HashMap<Position, TileOverride> {
        &self.tile_overrides
    }

    pub(crate) fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn map_kind(&self) -> MapKind {
        match self.map_id % 3 {
            0 => MapKind::Resource,
            1 => MapKind::Hazard,
            _ => MapKind::Monster,
        }
    }

    pub fn owned_entities(&self, player_id: u64) -> Vec<&Entity> {
        let mut entities = self
            .entities
            .values()
            .filter(|entity| entity.owner_id == player_id)
            .collect::<Vec<_>>();
        entities.sort_by_key(|entity| entity.id);
        entities
    }

    pub fn player_runtime_profile_with_rules(
        &self,
        player_id: u64,
        rules: &ServerRules,
    ) -> Option<crate::model::RuntimeProfile> {
        self.players
            .get(&player_id)
            .map(|player| rules.runtime_profile(player.core_tier))
    }

    pub fn visible_tiles_for(&self, player_id: u64) -> Vec<Tile> {
        let mut positions = Vec::new();
        let radius = self.observation_radius as i32;

        for entity in self.owned_entities(player_id) {
            for y in entity.position.y - radius..=entity.position.y + radius {
                for x in entity.position.x - radius..=entity.position.x + radius {
                    let position = Position::new(x, y);
                    if entity.position.manhattan(position) <= self.observation_radius {
                        positions.push(position);
                    }
                }
            }
        }

        positions.sort_by_key(|position| (position.x, position.y));
        positions.dedup();
        positions
            .into_iter()
            .map(|position| self.tile_at(position))
            .collect()
    }

    pub fn is_passable(&self, position: Position) -> bool {
        self.tile_at(position).building_id.is_none()
    }

    pub fn player_bot_path(&self, player_id: u64) -> Option<PathBuf> {
        self.players
            .get(&player_id)
            .map(|player| player.bot_path.clone())
            .filter(|path| !path.as_os_str().is_empty())
            .filter(|path| path != std::path::Path::new(crate::runner::DEFAULT_BOT_PATH))
    }

    pub fn set_player_bot_path(&mut self, player_id: u64, bot_path: PathBuf) -> Result<(), String> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or_else(|| format!("player {player_id} does not exist"))?;
        player.bot_path = bot_path;
        Ok(())
    }

    pub fn player_persistent_memory(&self, player_id: u64) -> &[u8] {
        self.players
            .get(&player_id)
            .map(|player| player.persistent_memory.as_slice())
            .unwrap_or_default()
    }

    pub fn set_player_persistent_memory(
        &mut self,
        player_id: u64,
        memory: Vec<u8>,
    ) -> Result<(), String> {
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or_else(|| format!("player {player_id} does not exist"))?;
        player.persistent_memory = memory;
        Ok(())
    }

    pub fn primary_player_id(&self) -> Option<u64> {
        self.players.keys().copied().min()
    }

    pub fn apply_action(&mut self, player_id: u64, action: &ValidatedAction) -> String {
        match *action {
            ValidatedAction::Move {
                actor_entity_id,
                target,
            } => self.apply_move(actor_entity_id, target),
            ValidatedAction::Mine {
                actor_entity_id,
                target,
                amount,
            } => self.apply_mine(actor_entity_id, target, amount),
            ValidatedAction::Build {
                actor_entity_id,
                target,
                building_kind,
            } => self.apply_build(player_id, actor_entity_id, target, building_kind),
            ValidatedAction::Lift {
                actor_entity_id,
                kind,
                amount,
            } => self.apply_lift(actor_entity_id, kind, amount),
            ValidatedAction::Put {
                actor_entity_id,
                kind,
                amount,
            } => self.apply_put(actor_entity_id, kind, amount),
            ValidatedAction::Craft {
                actor_entity_id,
                ref recipe_id,
                ref inputs,
                ref outputs,
                ..
            } => self.apply_craft(actor_entity_id, recipe_id, inputs, outputs),
            ValidatedAction::Research {
                actor_entity_id,
                ref research_id,
                ref inputs,
            } => self.apply_research(player_id, actor_entity_id, research_id, inputs),
        }
    }

    pub fn advance_tick(&mut self) {
        self.tick += 1;
    }

    fn apply_move(&mut self, actor_entity_id: u64, target: Position) -> String {
        let entity = self
            .entities
            .get_mut(&actor_entity_id)
            .expect("validated move actor must exist");
        let from = entity.position;
        entity.position = target;
        format!(
            "entity {actor_entity_id} moved from ({}, {}) to ({}, {})",
            from.x, from.y, target.x, target.y
        )
    }

    fn apply_mine(&mut self, actor_entity_id: u64, target: Position, amount: u32) -> String {
        let tile = self.tile_at(target);
        let Some(resource) = tile.resource else {
            return format!(
                "entity {actor_entity_id} mined 0 at ({}, {})",
                target.x, target.y
            );
        };
        let mined = amount.min(resource.amount);
        let remaining = resource.amount - mined;
        self.set_tile_resource(
            target,
            (remaining > 0).then_some(ResourceStack {
                kind: resource.kind,
                amount: remaining,
            }),
        );

        if let Some(entity) = self.entities.get_mut(&actor_entity_id) {
            add_cargo(
                entity,
                ItemStack {
                    kind: resource.kind.item_id().into(),
                    amount: mined,
                },
            );
        }

        format!(
            "entity {actor_entity_id} mined {mined} {:?} at ({}, {})",
            resource.kind, target.x, target.y
        )
    }

    fn apply_build(
        &mut self,
        player_id: u64,
        actor_entity_id: u64,
        target: Position,
        building_kind: BuildingKind,
    ) -> String {
        let building_id = self.alloc_id();
        self.buildings.insert(
            building_id,
            Building {
                id: building_id,
                kind: building_kind,
                owner_id: player_id,
                position: target,
                power: 0,
            },
        );

        format!(
            "entity {actor_entity_id} built {:?} {} at ({}, {})",
            building_kind, building_id, target.x, target.y
        )
    }

    fn apply_lift(&mut self, actor_entity_id: u64, kind: ResourceKind, amount: u32) -> String {
        let position = self
            .entities
            .get(&actor_entity_id)
            .expect("validated lift actor must exist")
            .position;
        let tile = self.tile_at(position);
        let lifted = tile
            .resource
            .filter(|r| r.kind == kind)
            .map(|r| amount.min(r.amount))
            .unwrap_or(0);
        if lifted > 0 {
            let remaining = tile.resource.map(|r| r.amount).unwrap_or(0) - lifted;
            self.set_tile_resource(
                position,
                (remaining > 0).then_some(ResourceStack {
                    kind,
                    amount: remaining,
                }),
            );
        }
        if lifted > 0 {
            let entity = self
                .entities
                .get_mut(&actor_entity_id)
                .expect("validated lift actor must exist");
            add_cargo(
                entity,
                ItemStack {
                    kind: kind.item_id().into(),
                    amount: lifted,
                },
            );
        }

        format!(
            "entity {actor_entity_id} lifted {lifted} {:?} at ({}, {})",
            kind, position.x, position.y
        )
    }

    fn apply_put(&mut self, actor_entity_id: u64, kind: ResourceKind, amount: u32) -> String {
        let position = self
            .entities
            .get(&actor_entity_id)
            .expect("validated put actor must exist")
            .position;
        let put = {
            let entity = self
                .entities
                .get_mut(&actor_entity_id)
                .expect("validated put actor must exist");
            remove_cargo(entity, kind.item_id(), amount)
        };
        if put > 0 {
            let tile = self.tile_at(position);
            let new_amount = tile
                .resource
                .filter(|r| r.kind == kind)
                .map(|r| r.amount + put)
                .unwrap_or(put);
            self.set_tile_resource(
                position,
                Some(ResourceStack {
                    kind,
                    amount: new_amount,
                }),
            );
        }

        format!(
            "entity {actor_entity_id} put {put} {:?} at ({}, {})",
            kind, position.x, position.y
        )
    }

    fn apply_craft(
        &mut self,
        actor_entity_id: u64,
        recipe_id: &str,
        inputs: &[ItemStack],
        outputs: &[ItemStack],
    ) -> String {
        let entity = self
            .entities
            .get_mut(&actor_entity_id)
            .expect("validated craft actor must exist");
        for input in inputs {
            remove_cargo(entity, &input.kind, input.amount);
        }
        for output in outputs {
            add_cargo(entity, output.clone());
        }

        format!("entity {actor_entity_id} crafted {recipe_id}")
    }

    fn apply_research(
        &mut self,
        player_id: u64,
        actor_entity_id: u64,
        research_id: &str,
        inputs: &[ItemStack],
    ) -> String {
        let entity = self
            .entities
            .get_mut(&actor_entity_id)
            .expect("validated research actor must exist");
        for input in inputs {
            remove_cargo(entity, &input.kind, input.amount);
        }
        let player = self
            .players
            .get_mut(&player_id)
            .expect("validated research player must exist");
        player.researched_ids.insert(research_id.to_string());

        format!("entity {actor_entity_id} researched {research_id}")
    }

    fn set_tile_resource(&mut self, position: Position, resource: Option<ResourceStack>) {
        self.tile_overrides.entry(position).or_default().resource = Some(resource);
    }

    fn spawn_initial_player(&mut self) {
        let player_id = self.alloc_id();
        let core_entity_id = self.alloc_id();
        let worker_entity_id = self.alloc_id();
        let core_building_id = self.alloc_id();
        let core_position = Position::new(0, 0);
        let worker_position = Position::new(1, 0);

        self.entities.insert(
            core_entity_id,
            Entity {
                id: core_entity_id,
                owner_id: player_id,
                position: core_position,
                cargo: Vec::new(),
            },
        );
        self.entities.insert(
            worker_entity_id,
            Entity {
                id: worker_entity_id,
                owner_id: player_id,
                position: worker_position,
                cargo: Vec::new(),
            },
        );
        self.buildings.insert(
            core_building_id,
            Building {
                id: core_building_id,
                kind: BuildingKind::None,
                owner_id: player_id,
                position: core_position,
                power: 25,
            },
        );
        self.players.insert(
            player_id,
            Player {
                id: player_id,
                core_entity_id,
                worker_entity_id,
                core_building_id,
                core_tier: CoreTier::Basic,
                bot_path: PathBuf::new(),
                persistent_memory: Vec::new(),
                researched_ids: HashSet::new(),
            },
        );
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn building_at(&self, position: Position) -> Option<&Building> {
        self.buildings
            .values()
            .find(|building| building.position == position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{ServerEnv, ServerRules};

    #[test]
    fn creates_initial_player_entities_and_core_building() {
        let world = WorldState::new();

        assert_eq!(world.players.len(), 1);
        assert_eq!(world.entities.len(), 2);
        assert_eq!(world.buildings.len(), 1);
        assert_eq!(world.stored_tile_change_count(), 0);
        assert_eq!(world.player_bot_path(1), None);
    }

    #[test]
    fn tile_lookup_overlays_core_building() {
        let world = WorldState::new();
        let tile = world.tile_at(Position::new(0, 0));

        assert!(tile.building_id.is_some());
        assert_eq!(tile.owner_id, Some(1));
    }

    #[test]
    fn visible_tiles_include_owned_entity_radius() {
        let world = WorldState::new();
        let tiles = world.visible_tiles_for(1);

        assert!(
            tiles
                .iter()
                .any(|tile| tile.position == Position::new(0, 0))
        );
        assert!(
            tiles
                .iter()
                .any(|tile| tile.position == Position::new(1, 0))
        );
    }

    #[test]
    fn creates_world_from_server_env_and_rules() {
        let env = ServerEnv {
            world_seed: 42,
            map_id: 2,
        };
        let rules = ServerRules {
            observation_radius: 3,
            ..ServerRules::default()
        };

        let world = WorldState::new_with_config(&env, &rules);

        assert_eq!(world.world_seed, 42);
        assert_eq!(world.map_id, 2);
        assert_eq!(world.observation_radius, 3);
    }

    #[test]
    fn craft_action_consumes_inputs_and_adds_outputs() {
        let mut world = WorldState::new();
        let actor_id = world.players.get(&1).expect("player").worker_entity_id;
        world.entities.get_mut(&actor_id).expect("actor").cargo = vec![ItemStack {
            kind: "iron-ore".into(),
            amount: 1,
        }];

        let result = world.apply_action(
            1,
            &ValidatedAction::Craft {
                actor_entity_id: actor_id,
                recipe_id: "iron-plate".into(),
                target_building_id: None,
                inputs: vec![ItemStack {
                    kind: "iron-ore".into(),
                    amount: 1,
                }],
                outputs: vec![ItemStack {
                    kind: "iron-plate".into(),
                    amount: 1,
                }],
            },
        );

        let cargo = &world.entities.get(&actor_id).expect("actor").cargo;
        assert_eq!(result, format!("entity {actor_id} crafted iron-plate"));
        assert!(!cargo.iter().any(|stack| stack.kind == "iron-ore"));
        assert!(
            cargo
                .iter()
                .any(|stack| stack.kind == "iron-plate" && stack.amount == 1)
        );
    }

    #[test]
    fn research_action_consumes_inputs_and_unlocks_recipes() {
        let mut world = WorldState::new();
        let player_id = 1;
        let actor_id = world
            .players
            .get(&player_id)
            .expect("player")
            .worker_entity_id;
        world.entities.get_mut(&actor_id).expect("actor").cargo = vec![ItemStack {
            kind: "iron-ore".into(),
            amount: 10,
        }];

        let result = world.apply_action(
            player_id,
            &ValidatedAction::Research {
                actor_entity_id: actor_id,
                research_id: "basic-smelting".into(),
                inputs: vec![ItemStack {
                    kind: "iron-ore".into(),
                    amount: 10,
                }],
            },
        );

        let cargo = &world.entities.get(&actor_id).expect("actor").cargo;
        let player = world.players.get(&player_id).expect("player");
        assert_eq!(
            result,
            format!("entity {actor_id} researched basic-smelting")
        );
        assert!(!cargo.iter().any(|stack| stack.kind == "iron-ore"));
        assert!(player.researched_ids.contains("basic-smelting"));
    }
}
