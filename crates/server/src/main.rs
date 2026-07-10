mod action_log;
mod api;
mod config;
mod model;
mod protocol;
mod rules;
mod runner;
mod state;
mod storage;
mod tick;
mod world;

use state::SharedState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let config = config::parse_config()?;
    let world = storage::load_world_or_default(&config);
    let action_log = storage::load_action_log_or_default();
    tracing::info!(
        "server world initialized: seed={}, map_id={}, tick={}, radius={}",
        world.world_seed,
        world.map_id,
        world.tick,
        world.observation_radius
    );
    tracing::info!(
        "state: players={}, entities={}, buildings={}, stored_tile_changes={}",
        world.players.len(),
        world.entities.len(),
        world.buildings.len(),
        world.stored_tile_change_count()
    );
    if let Some(max_actions) = config.debug_max_actions {
        tracing::info!("debug: overriding max_actions per bot run to {max_actions}");
    }

    let state = SharedState::new(world, action_log, config);
    tokio::spawn(tick::run_tick_loop(state.clone()));

    let addr = state.inner().config.addr.clone();
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("HTTP API listening on http://{addr}");
    axum::serve(listener, api::router(state)).await?;

    Ok(())
}
