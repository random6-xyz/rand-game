use std::path::Path;

use crate::action_log::ActionLog;
use crate::state::ServerConfig;
use crate::world::WorldState;

const WORLD_STATE_PATH: &str = "var/server/world.bin";
const ACTION_LOG_PATH: &str = "var/server/action-log.bin";

pub fn load_world_or_default(config: &ServerConfig) -> WorldState {
    match std::fs::read(WORLD_STATE_PATH) {
        Ok(bytes) => match bincode::deserialize::<WorldState>(&bytes) {
            Ok(world) => {
                warn_if_world_config_differs(&world, config);
                world
            }
            Err(err) => {
                eprintln!("failed to restore world state: {err}");
                WorldState::new_with_config(&config.env, &config.rules)
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            WorldState::new_with_config(&config.env, &config.rules)
        }
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
    match std::fs::read(ACTION_LOG_PATH) {
        Ok(bytes) => match bincode::deserialize(&bytes) {
            Ok(action_log) => action_log,
            Err(err) => {
                eprintln!("failed to restore action log: {err}");
                ActionLog::default()
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ActionLog::default(),
        Err(err) => {
            eprintln!("failed to read action log: {err}");
            ActionLog::default()
        }
    }
}

pub fn save_world(world: &WorldState) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(WORLD_STATE_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = bincode::serialize(world)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn save_action_log(action_log: &ActionLog) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(ACTION_LOG_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = bincode::serialize(action_log)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
