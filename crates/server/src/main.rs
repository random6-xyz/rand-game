mod action_log;
mod api;
mod model;
mod protocol;
mod rules;
mod runner;
mod state;
mod storage;
mod tick;
mod world;

use state::{ServerConfig, SharedState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_config()?;
    let world = storage::load_world_or_default();
    let action_log = storage::load_action_log_or_default();
    println!(
        "server world initialized: seed={}, map_id={}, tick={}, radius={}",
        world.world_seed, world.map_id, world.tick, world.observation_radius
    );
    println!(
        "state: players={}, entities={}, buildings={}, stored_tile_changes={}",
        world.players.len(),
        world.entities.len(),
        world.buildings.len(),
        world.stored_tile_change_count()
    );
    if let Some(max_actions) = config.debug_max_actions {
        println!("debug: overriding max_actions per bot run to {max_actions}");
    }

    let state = SharedState::new(world, action_log, config);
    tokio::spawn(tick::run_tick_loop(state.clone()));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("HTTP API listening on http://127.0.0.1:3000");
    axum::serve(listener, api::router(state)).await?;

    Ok(())
}

fn parse_config() -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let mut config = ServerConfig {
        debug_max_actions: parse_env_u32("RAND_GAME_DEBUG_MAX_ACTIONS")?,
        log_bot_stderr: parse_env_bool("RAND_GAME_LOG_BOT_STDERR")?,
    };
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--debug-max-actions" => {
                let value = args.next().ok_or("missing value for --debug-max-actions")?;
                config.debug_max_actions = Some(value.parse()?);
            }
            "--log-bot-stderr" => {
                config.log_bot_stderr = true;
            }
            "--help" | "-h" => {
                println!(
                    "rand-game-server\n\nUsage:\n  rand-game-server [--debug-max-actions N] [--log-bot-stderr]\n\nEnvironment:\n  RAND_GAME_DEBUG_MAX_ACTIONS=N\n  RAND_GAME_LOG_BOT_STDERR=1"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown option `{other}`").into()),
        }
    }

    Ok(config)
}

fn parse_env_u32(name: &str) -> Result<Option<u32>, Box<dyn std::error::Error>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value.parse()?)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn parse_env_bool(name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match std::env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON" => Ok(true),
            "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF" => Ok(false),
            _ => Err(format!("{name} must be a boolean value").into()),
        },
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(err) => Err(err.into()),
    }
}
