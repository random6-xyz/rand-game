use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::action_log::{ActionLog, ActionLogEntry};
use crate::model::{
    Building, BuildingKind, CoreTier, Entity, ItemStack, Player, Position, ResourceKind,
    ResourceStack, TileOverride, ValidatedAction,
};
use crate::state::ServerConfig;
use crate::world::{WorldState, WorldStateParts};

const DB_PATH: &str = "var/server/state.sqlite3";

pub fn load_world_or_default(config: &ServerConfig) -> WorldState {
    match load_world() {
        Ok(Some(world)) => {
            warn_if_world_config_differs(&world, config);
            world
        }
        Ok(None) => WorldState::new_with_config(&config.env, &config.rules),
        Err(err) => {
            eprintln!("failed to read world state: {err}");
            WorldState::new_with_config(&config.env, &config.rules)
        }
    }
}

fn warn_if_world_config_differs(world: &WorldState, config: &ServerConfig) {
    if world.world_seed != config.env.world_seed {
        eprintln!(
            "stored world seed {} differs from configured seed {}; run `cargo xtask clean-state` to start a new world",
            world.world_seed, config.env.world_seed
        );
    }
    if world.map_id != config.env.map_id {
        eprintln!(
            "stored map_id {} differs from configured map_id {}; run `cargo xtask clean-state` to start a new world",
            world.map_id, config.env.map_id
        );
    }
    if world.observation_radius != config.rules.observation_radius {
        eprintln!(
            "stored observation_radius {} differs from configured observation_radius {}; run `cargo xtask clean-state` to start a new world",
            world.observation_radius, config.rules.observation_radius
        );
    }
}

pub fn load_action_log_or_default() -> ActionLog {
    match load_action_log() {
        Ok(action_log) => action_log,
        Err(err) => {
            eprintln!("failed to read action log: {err}");
            ActionLog::default()
        }
    }
}

pub fn save_world(world: &WorldState) -> Result<(), Box<dyn std::error::Error>> {
    save_world_to_path(world, Path::new(DB_PATH))
}

fn save_world_to_path(world: &WorldState, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = open_connection_at(path)?;
    let tx = conn.transaction()?;

    tx.execute("DELETE FROM world_meta", [])?;
    tx.execute("DELETE FROM players", [])?;
    tx.execute("DELETE FROM entities", [])?;
    tx.execute("DELETE FROM entity_cargo", [])?;
    tx.execute("DELETE FROM buildings", [])?;
    tx.execute("DELETE FROM tile_overrides", [])?;

    tx.execute(
        "INSERT INTO world_meta (id, world_seed, map_id, tick, observation_radius, next_id) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![
            to_i64(world.world_seed)?,
            i64::from(world.map_id),
            to_i64(world.tick)?,
            i64::from(world.observation_radius),
            to_i64(world.next_id())?,
        ],
    )?;

    for player in world.players.values() {
        tx.execute(
            "INSERT INTO players (id, core_entity_id, worker_entity_id, core_building_id, core_tier, bot_path, persistent_memory, researched_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                to_i64(player.id)?,
                to_i64(player.core_entity_id)?,
                to_i64(player.worker_entity_id)?,
                to_i64(player.core_building_id)?,
                to_json(&player.core_tier)?,
                player.bot_path.to_string_lossy().as_ref(),
                &player.persistent_memory,
                to_json(&player.researched_ids)?,
            ],
        )?;
    }

    for entity in world.entities.values() {
        tx.execute(
            "INSERT INTO entities (id, owner_id, x, y) VALUES (?1, ?2, ?3, ?4)",
            params![
                to_i64(entity.id)?,
                to_i64(entity.owner_id)?,
                entity.position.x,
                entity.position.y,
            ],
        )?;
        for (idx, stack) in entity.cargo.iter().enumerate() {
            tx.execute(
                "INSERT INTO entity_cargo (entity_id, idx, kind, amount) VALUES (?1, ?2, ?3, ?4)",
                params![
                    to_i64(entity.id)?,
                    to_i64(idx as u64)?,
                    &stack.kind,
                    i64::from(stack.amount)
                ],
            )?;
        }
    }

    for building in world.buildings.values() {
        tx.execute(
            "INSERT INTO buildings (id, kind, owner_id, x, y, power) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                to_i64(building.id)?,
                to_json(&building.kind)?,
                to_i64(building.owner_id)?,
                building.position.x,
                building.position.y,
                building.power,
            ],
        )?;
    }

    for (position, override_tile) in world.tile_overrides() {
        let (resource_is_set, resource_kind, resource_amount) = match override_tile.resource {
            Some(Some(resource)) => (
                1_i64,
                Some(to_json(&resource.kind)?),
                Some(i64::from(resource.amount)),
            ),
            Some(None) => (1_i64, None, None),
            None => (0_i64, None, None),
        };
        let (owner_is_set, owner_id) = match override_tile.owner_id {
            Some(Some(owner_id)) => (1_i64, Some(to_i64(owner_id)?)),
            Some(None) => (1_i64, None),
            None => (0_i64, None),
        };
        tx.execute(
            "INSERT INTO tile_overrides (x, y, resource_is_set, resource_kind, resource_amount, owner_is_set, owner_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                position.x,
                position.y,
                resource_is_set,
                resource_kind,
                resource_amount,
                owner_is_set,
                owner_id,
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

pub fn save_action_log(action_log: &ActionLog) -> Result<(), Box<dyn std::error::Error>> {
    save_action_log_to_path(action_log, Path::new(DB_PATH))
}

fn save_action_log_to_path(
    action_log: &ActionLog,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = open_connection_at(path)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM action_log", [])?;
    for (idx, entry) in action_log.entries().iter().enumerate() {
        tx.execute(
            "INSERT INTO action_log (seq, tick, player_id, action_json, result, count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                to_i64(idx as u64)?,
                to_i64(entry.tick)?,
                to_i64(entry.player_id)?,
                to_json(&entry.action)?,
                &entry.result,
                to_i64(entry.count)?,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn load_world() -> Result<Option<WorldState>, Box<dyn std::error::Error>> {
    load_world_from_path(Path::new(DB_PATH))
}

fn load_world_from_path(path: &Path) -> Result<Option<WorldState>, Box<dyn std::error::Error>> {
    let conn = open_connection_at(path)?;
    let Some(meta) = conn
        .query_row(
            "SELECT world_seed, map_id, tick, observation_radius, next_id FROM world_meta WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?
    else {
        return Ok(None);
    };

    let mut players = HashMap::new();
    let mut player_rows = conn.prepare(
        "SELECT id, core_entity_id, worker_entity_id, core_building_id, core_tier, bot_path, persistent_memory, researched_ids FROM players",
    )?;
    let player_iter = player_rows.query_map([], |row| {
        let core_tier_json: String = row.get(4)?;
        let bot_path: String = row.get(5)?;
        let researched_ids_json: String = row.get::<_, String>(7).unwrap_or_else(|_| "[]".into());
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            core_tier_json,
            bot_path,
            row.get::<_, Vec<u8>>(6)?,
            researched_ids_json,
        ))
    })?;
    for player in player_iter {
        let (
            id,
            core_entity_id,
            worker_entity_id,
            core_building_id,
            core_tier_json,
            bot_path,
            persistent_memory,
            researched_ids_json,
        ) = player?;
        let id = from_i64(id, "player id")?;
        players.insert(
            id,
            Player {
                id,
                core_entity_id: from_i64(core_entity_id, "core entity id")?,
                worker_entity_id: from_i64(worker_entity_id, "worker entity id")?,
                core_building_id: from_i64(core_building_id, "core building id")?,
                core_tier: from_json::<CoreTier>(&core_tier_json)?,
                bot_path: PathBuf::from(bot_path),
                persistent_memory,
                researched_ids: serde_json::from_str(&researched_ids_json).unwrap_or_default(),
            },
        );
    }

    let mut cargo_by_entity: HashMap<u64, Vec<ItemStack>> = HashMap::new();
    {
        let mut cargo_rows = conn.prepare(
            "SELECT entity_id, kind, amount FROM entity_cargo ORDER BY entity_id, idx ASC",
        )?;
        let cargo_iter = cargo_rows.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for cargo in cargo_iter {
            let (entity_id, kind, amount) = cargo?;
            let entity_id = from_i64(entity_id, "cargo entity id")?;
            cargo_by_entity
                .entry(entity_id)
                .or_default()
                .push(ItemStack {
                    kind,
                    amount: from_i64_to_u32(amount, "cargo amount")?,
                });
        }
    }

    let mut entities = HashMap::new();
    let mut entity_rows = conn.prepare("SELECT id, owner_id, x, y FROM entities")?;
    let entity_iter = entity_rows.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, i32>(3)?,
        ))
    })?;
    for entity in entity_iter {
        let (id, owner_id, x, y) = entity?;
        let id = from_i64(id, "entity id")?;
        entities.insert(
            id,
            Entity {
                id,
                owner_id: from_i64(owner_id, "entity owner id")?,
                position: Position::new(x, y),
                cargo: cargo_by_entity.remove(&id).unwrap_or_default(),
            },
        );
    }

    let mut buildings = HashMap::new();
    let mut building_rows =
        conn.prepare("SELECT id, kind, owner_id, x, y, power FROM buildings")?;
    let building_iter = building_rows.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i32>(3)?,
            row.get::<_, i32>(4)?,
            row.get::<_, i32>(5)?,
        ))
    })?;
    for building in building_iter {
        let (id, kind_json, owner_id, x, y, power) = building?;
        let id = from_i64(id, "building id")?;
        buildings.insert(
            id,
            Building {
                id,
                kind: from_json::<BuildingKind>(&kind_json)?,
                owner_id: from_i64(owner_id, "building owner id")?,
                position: Position::new(x, y),
                power,
            },
        );
    }

    let mut tile_overrides = HashMap::new();
    let mut tile_rows = conn.prepare(
        "SELECT x, y, resource_is_set, resource_kind, resource_amount, owner_is_set, owner_id FROM tile_overrides",
    )?;
    let tile_iter = tile_rows.query_map([], |row| {
        Ok((
            row.get::<_, i32>(0)?,
            row.get::<_, i32>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    })?;
    for tile in tile_iter {
        let (x, y, resource_is_set, resource_kind, resource_amount, owner_is_set, owner_id) = tile?;
        let resource = if resource_is_set == 0 {
            None
        } else {
            Some(match (resource_kind, resource_amount) {
                (Some(kind), Some(amount)) => Some(ResourceStack {
                    kind: from_json::<ResourceKind>(&kind)?,
                    amount: from_i64_to_u32(amount, "resource amount")?,
                }),
                _ => None,
            })
        };
        let owner_id = if owner_is_set == 0 {
            None
        } else {
            Some(
                owner_id
                    .map(|id| from_i64(id, "tile owner id"))
                    .transpose()?,
            )
        };
        tile_overrides.insert(Position::new(x, y), TileOverride { resource, owner_id });
    }

    Ok(Some(WorldState::from_parts(WorldStateParts {
        world_seed: from_i64(meta.0, "world seed")?,
        map_id: from_i64_to_u32(meta.1, "map id")?,
        tick: from_i64(meta.2, "tick")?,
        observation_radius: from_i64_to_u32(meta.3, "observation radius")?,
        players,
        entities,
        buildings,
        tile_overrides,
        next_id: from_i64(meta.4, "next id")?,
    })))
}

fn load_action_log() -> Result<ActionLog, Box<dyn std::error::Error>> {
    load_action_log_from_path(Path::new(DB_PATH))
}

fn load_action_log_from_path(path: &Path) -> Result<ActionLog, Box<dyn std::error::Error>> {
    let conn = open_connection_at(path)?;
    let mut rows = conn.prepare(
        "SELECT tick, player_id, action_json, result, count FROM action_log ORDER BY seq ASC",
    )?;
    let iter = rows.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    let mut entries = Vec::new();
    for entry in iter {
        let (tick, player_id, action_json, result, count) = entry?;
        entries.push(ActionLogEntry {
            tick: from_i64(tick, "action log tick")?,
            player_id: from_i64(player_id, "action log player id")?,
            action: from_json::<ValidatedAction>(&action_json)?,
            result,
            count: from_i64(count, "action log count")?,
        });
    }
    Ok(ActionLog::from_entries(entries))
}

fn open_connection_at(path: &Path) -> Result<Connection, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    ensure_schema(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 5000;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS world_meta (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            world_seed INTEGER NOT NULL,
            map_id INTEGER NOT NULL,
            tick INTEGER NOT NULL,
            observation_radius INTEGER NOT NULL,
            next_id INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS players (
            id INTEGER PRIMARY KEY,
            core_entity_id INTEGER NOT NULL,
            worker_entity_id INTEGER NOT NULL,
            core_building_id INTEGER NOT NULL,
            core_tier TEXT NOT NULL,
            bot_path TEXT NOT NULL,
            persistent_memory BLOB NOT NULL,
            researched_ids TEXT NOT NULL DEFAULT '[]'
        );

        CREATE TABLE IF NOT EXISTS entities (
            id INTEGER PRIMARY KEY,
            owner_id INTEGER NOT NULL,
            x INTEGER NOT NULL,
            y INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS entity_cargo (
            entity_id INTEGER NOT NULL,
            idx INTEGER NOT NULL,
            kind TEXT NOT NULL,
            amount INTEGER NOT NULL,
            PRIMARY KEY (entity_id, idx)
        );

        CREATE TABLE IF NOT EXISTS buildings (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            owner_id INTEGER NOT NULL,
            x INTEGER NOT NULL,
            y INTEGER NOT NULL,
            power INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tile_overrides (
            x INTEGER NOT NULL,
            y INTEGER NOT NULL,
            resource_is_set INTEGER NOT NULL,
            resource_kind TEXT,
            resource_amount INTEGER,
            owner_is_set INTEGER NOT NULL,
            owner_id INTEGER,
            PRIMARY KEY (x, y)
        );

        CREATE TABLE IF NOT EXISTS action_log (
            seq INTEGER PRIMARY KEY,
            tick INTEGER NOT NULL,
            player_id INTEGER NOT NULL,
            action_json TEXT NOT NULL,
            result TEXT NOT NULL,
            count INTEGER NOT NULL
        );
        ",
    )
}

fn to_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}

fn from_json<T: DeserializeOwned>(value: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(value)
}

fn to_i64(value: u64) -> Result<i64, Box<dyn std::error::Error>> {
    i64::try_from(value).map_err(|_| format!("value {value} exceeds SQLite INTEGER range").into())
}

fn from_i64(value: i64, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    u64::try_from(value).map_err(|_| format!("{name} must be non-negative, got {value}").into())
}

fn from_i64_to_u32(value: i64, name: &str) -> Result<u32, Box<dyn std::error::Error>> {
    u32::try_from(value).map_err(|_| format!("{name} is out of u32 range: {value}").into())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::action_log::{ActionLog, ActionLogEntry};
    use crate::model::{ItemStack, Position, ResourceKind, ValidatedAction};
    use crate::world::WorldState;

    use super::{
        load_action_log_from_path, load_world_from_path, save_action_log_to_path,
        save_world_to_path,
    };

    #[test]
    fn saves_and_loads_world_from_sqlite() {
        let path = test_db_path("world");
        let mut world = WorldState::new();
        let worker_id = world.players.get(&1).expect("player").worker_entity_id;
        world.entities.get_mut(&worker_id).expect("worker").cargo = vec![ItemStack {
            kind: "energy".into(),
            amount: 3,
        }];
        world
            .set_player_bot_path(1, "var/bots/1/bot".into())
            .expect("set bot path");
        world.advance_tick();
        world.apply_action(
            1,
            &ValidatedAction::Put {
                actor_entity_id: worker_id,
                kind: ResourceKind::Energy,
                amount: 2,
            },
        );

        save_world_to_path(&world, &path).expect("save world");
        let loaded = load_world_from_path(&path)
            .expect("load world")
            .expect("stored world");

        assert_eq!(loaded.tick, 1);
        assert_eq!(
            loaded.player_bot_path(1).expect("bot path"),
            pathbuf("var/bots/1/bot")
        );
        assert_eq!(loaded.next_id(), world.next_id());
        assert_eq!(loaded.stored_tile_change_count(), 1);
        assert_eq!(
            loaded
                .entities
                .get(&worker_id)
                .expect("worker")
                .cargo
                .first()
                .expect("cargo")
                .amount,
            1
        );
        let expected_resource = world.tile_at(Position::new(1, 0)).resource;
        let resource = loaded.tile_at(Position::new(1, 0)).resource;
        assert_eq!(resource, expected_resource);
        let resource = resource.expect("stored resource");
        assert_eq!(resource.kind, ResourceKind::Energy);

        cleanup(&path);
    }

    #[test]
    fn saves_and_loads_player_research_state() {
        let path = test_db_path("research");
        let mut world = WorldState::new();
        let player_id = 1;
        let player = world.players.get_mut(&player_id).expect("player");
        player.researched_ids.insert("basic-smelting".to_string());
        player
            .researched_ids
            .insert("advanced-manufacturing".to_string());

        save_world_to_path(&world, &path).expect("save world");
        let loaded = load_world_from_path(&path)
            .expect("load world")
            .expect("stored world");
        let loaded_player = loaded.players.get(&player_id).expect("player");

        assert_eq!(loaded_player.researched_ids.len(), 2);
        assert!(loaded_player.researched_ids.contains("basic-smelting"));
        assert!(
            loaded_player
                .researched_ids
                .contains("advanced-manufacturing")
        );

        cleanup(&path);
    }

    #[test]
    fn saves_and_loads_action_log_from_sqlite() {
        let path = test_db_path("action-log");
        let action = ValidatedAction::Move {
            actor_entity_id: 2,
            target: Position::new(2, 0),
        };
        let mut log = ActionLog::default();
        log.push(ActionLogEntry::new(7, 1, action, "moved".into()));

        save_action_log_to_path(&log, &path).expect("save action log");
        let loaded = load_action_log_from_path(&path).expect("load action log");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.entries()[0].tick, 7);
        assert_eq!(loaded.entries()[0].result, "moved");

        cleanup(&path);
    }

    fn test_db_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::path::PathBuf::from(format!(
            "target/storage-tests/{name}-{}-{nanos}/state.sqlite3",
            std::process::id()
        ))
    }

    fn pathbuf(path: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(path)
    }

    fn cleanup(path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
