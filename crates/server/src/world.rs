use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::{
    Building, BuildingKind, ChunkCoord, CoreTier, Entity, EntityKind, EnvironmentEvent,
    EnvironmentEventKind, MapKind, Monster, MonsterKind, Player, Position, ResourceStack,
    TerrainKind, Tile, TileOverride, ValidatedAction,
};
use crate::rules;

pub const CHUNK_SIZE: i32 = 32;

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

impl WorldState {
    pub fn new() -> Self {
        let mut world = Self {
            world_seed: rules::WORLD_SEED,
            map_id: rules::MAP_ID,
            tick: 0,
            observation_radius: rules::OBSERVATION_RADIUS,
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
            if let Some(terrain) = change.terrain {
                tile.terrain = terrain;
            }
            if let Some(resource) = change.resource {
                tile.resource = resource;
            }
            if let Some(owner_id) = change.owner_id {
                tile.owner_id = owner_id;
            }
            if let Some(danger_level) = change.danger_level {
                tile.danger_level = danger_level;
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

    pub fn map_kind(&self) -> MapKind {
        match self.map_id % 5 {
            0 => MapKind::Resource,
            1 => MapKind::Hazard,
            2 => MapKind::Monster,
            3 => MapKind::Event,
            _ => MapKind::War,
        }
    }

    pub fn chunk_coord(position: Position) -> ChunkCoord {
        ChunkCoord {
            x: position.x.div_euclid(CHUNK_SIZE),
            y: position.y.div_euclid(CHUNK_SIZE),
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

    pub fn player_runtime_profile(&self, player_id: u64) -> Option<crate::model::RuntimeProfile> {
        self.players
            .get(&player_id)
            .map(|player| player.core_tier.runtime_profile())
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

    pub fn visible_monsters_for(&self, player_id: u64) -> Vec<Monster> {
        let mut monsters = Vec::new();
        for entity in self.owned_entities(player_id) {
            let radius = self.observation_radius as i32;
            let min_chunk = Self::chunk_coord(Position::new(
                entity.position.x - radius,
                entity.position.y - radius,
            ));
            let max_chunk = Self::chunk_coord(Position::new(
                entity.position.x + radius,
                entity.position.y + radius,
            ));
            for chunk_y in min_chunk.y..=max_chunk.y {
                for chunk_x in min_chunk.x..=max_chunk.x {
                    if let Some(monster) = self.generated_monster(ChunkCoord {
                        x: chunk_x,
                        y: chunk_y,
                    }) && entity.position.manhattan(monster.position) <= self.observation_radius
                    {
                        monsters.push(monster);
                    }
                }
            }
        }
        monsters.sort_by_key(|monster| monster.id);
        monsters.dedup_by_key(|monster| monster.id);
        monsters
    }

    pub fn environment_events_for(&self, player_id: u64) -> Vec<EnvironmentEvent> {
        let mut events = Vec::new();
        for entity in self.owned_entities(player_id) {
            let radius = self.observation_radius as i32;
            let min_chunk = Self::chunk_coord(Position::new(
                entity.position.x - radius,
                entity.position.y - radius,
            ));
            let max_chunk = Self::chunk_coord(Position::new(
                entity.position.x + radius,
                entity.position.y + radius,
            ));
            for chunk_y in min_chunk.y..=max_chunk.y {
                for chunk_x in min_chunk.x..=max_chunk.x {
                    if let Some(event) = self.generated_environment_event(ChunkCoord {
                        x: chunk_x,
                        y: chunk_y,
                    }) && entity.position.manhattan(event.center)
                        <= self.observation_radius + event.radius
                    {
                        events.push(event);
                    }
                }
            }
        }
        events.sort_by_key(|event| event.id);
        events.dedup_by_key(|event| event.id);
        events
    }

    pub fn is_passable(&self, position: Position) -> bool {
        let tile = self.tile_at(position);
        !matches!(tile.terrain, TerrainKind::Water | TerrainKind::Mountain)
            && tile.building_id.is_none()
    }

    pub fn building_at_id(&self, building_id: u64) -> Option<&Building> {
        self.buildings.get(&building_id)
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
            ValidatedAction::Transfer {
                actor_entity_id,
                target_entity_id,
                target_building_id,
                resource,
                amount,
            } => self.apply_transfer(
                actor_entity_id,
                target_entity_id,
                target_building_id,
                resource,
                amount,
            ),
            ValidatedAction::Scan {
                actor_entity_id,
                target,
                radius,
            } => format!(
                "entity {actor_entity_id} scanned ({}, {}) radius {radius}",
                target.x, target.y
            ),
        }
    }

    pub fn advance_tick(&mut self) {
        self.tick += 1;
    }

    fn generated_monster(&self, chunk: ChunkCoord) -> Option<Monster> {
        if !matches!(self.map_kind(), MapKind::Monster | MapKind::War) {
            return None;
        }
        let sample = rules::hash_chunk(self.world_seed, self.map_id, chunk, self.tick / 10);
        if sample % 100 >= 35 {
            return None;
        }
        let position = Position::new(
            chunk.x * CHUNK_SIZE + ((sample >> 8) % CHUNK_SIZE as u64) as i32,
            chunk.y * CHUNK_SIZE + ((sample >> 16) % CHUNK_SIZE as u64) as i32,
        );
        let kind = match (sample >> 24) % 3 {
            0 => MonsterKind::Drone,
            1 => MonsterKind::Swarm,
            _ => MonsterKind::Guardian,
        };
        let max_hp = match kind {
            MonsterKind::Drone => 40,
            MonsterKind::Swarm => 75,
            MonsterKind::Guardian => 150,
        };
        Some(Monster {
            id: sample,
            kind,
            position,
            hp: max_hp,
            max_hp,
            target_entity_id: 0,
        })
    }

    fn generated_environment_event(&self, chunk: ChunkCoord) -> Option<EnvironmentEvent> {
        if !matches!(
            self.map_kind(),
            MapKind::Hazard | MapKind::Event | MapKind::War
        ) {
            return None;
        }
        let window = self.tick / 30;
        let sample = rules::hash_chunk(
            self.world_seed ^ 0x4556_454e_5453,
            self.map_id,
            chunk,
            window,
        );
        if sample % 100 >= 45 {
            return None;
        }
        let kind = match (sample >> 8) % 4 {
            0 => EnvironmentEventKind::Storm,
            1 => EnvironmentEventKind::Radiation,
            2 => EnvironmentEventKind::Meteor,
            _ => EnvironmentEventKind::ResourceSurge,
        };
        Some(EnvironmentEvent {
            id: sample,
            kind,
            center: Position::new(chunk.x * CHUNK_SIZE + 16, chunk.y * CHUNK_SIZE + 16),
            radius: 4 + ((sample >> 16) % 9) as u32,
            starts_at_tick: window * 30,
            ends_at_tick: window * 30 + 30,
            intensity: 1 + ((sample >> 24) % 100) as u16,
        })
    }

    fn apply_move(&mut self, actor_entity_id: u64, target: Position) -> String {
        let entity = self
            .entities
            .get_mut(&actor_entity_id)
            .expect("validated move actor must exist");
        let from = entity.position;
        entity.position = target;
        entity.cooldown_until_tick = self.tick + 1;
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
                ResourceStack {
                    kind: resource.kind,
                    amount: mined,
                },
            );
            entity.cooldown_until_tick = self.tick + 1;
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
                hp: 100,
                max_hp: 100,
                power: 0,
            },
        );

        if let Some(entity) = self.entities.get_mut(&actor_entity_id) {
            entity.cooldown_until_tick = self.tick + 2;
        }

        format!(
            "entity {actor_entity_id} built {:?} {} at ({}, {})",
            building_kind, building_id, target.x, target.y
        )
    }

    fn apply_transfer(
        &mut self,
        actor_entity_id: u64,
        target_entity_id: Option<u64>,
        target_building_id: Option<u64>,
        resource: ResourceStack,
        amount: u32,
    ) -> String {
        let moved = amount.min(resource.amount);
        if let Some(entity) = self.entities.get_mut(&actor_entity_id) {
            entity.cooldown_until_tick = self.tick + 1;
        }
        format!(
            "entity {actor_entity_id} transferred {moved} {:?} to entity {:?} building {:?}",
            resource.kind, target_entity_id, target_building_id
        )
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
                kind: EntityKind::Core,
                owner_id: player_id,
                position: core_position,
                hp: 500,
                max_hp: 500,
                energy: 100,
                cargo: Vec::new(),
                cooldown_until_tick: 0,
            },
        );
        self.entities.insert(
            worker_entity_id,
            Entity {
                id: worker_entity_id,
                kind: EntityKind::Worker,
                owner_id: player_id,
                position: worker_position,
                hp: 100,
                max_hp: 100,
                energy: 50,
                cargo: Vec::new(),
                cooldown_until_tick: 0,
            },
        );
        self.buildings.insert(
            core_building_id,
            Building {
                id: core_building_id,
                kind: BuildingKind::Core,
                owner_id: player_id,
                position: core_position,
                hp: 1000,
                max_hp: 1000,
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

fn add_cargo(entity: &mut Entity, resource: ResourceStack) {
    if let Some(existing) = entity
        .cargo
        .iter_mut()
        .find(|existing| existing.kind == resource.kind)
    {
        existing.amount += resource.amount;
    } else {
        entity.cargo.push(resource);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
