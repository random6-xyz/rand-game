use std::path::PathBuf;

mod map_view;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::action_log::ActionLogEntry;
use crate::model::{Entity, Position, Tile};
use crate::state::SharedState;
use crate::storage;

use self::map_view::render_ascii_map;

const MAX_WORLD_RADIUS: i32 = 16;
const MAX_DEBUG_MAP_VIEW_RADIUS: i32 = 128;
const DEFAULT_WORLD_RADIUS: i32 = 4;
const MAX_BOT_UPLOAD_BYTES: usize = 16 * 1024 * 1024;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/world", get(world_region))
        .route("/map-view", get(map_view))
        .route("/entities", get(entities))
        .route("/action-log", get(action_log))
        .route("/bots", post(upload_bot))
        .layer(DefaultBodyLimit::max(MAX_BOT_UPLOAD_BYTES))
        .with_state(state)
}

async fn health(State(state): State<SharedState>) -> Json<HealthResponse> {
    let world = state.inner().world.lock().await;
    let action_log = state.inner().action_log.lock().await;

    Json(HealthResponse {
        ok: true,
        tick: world.tick,
        players: world.players.len(),
        entities: world.entities.len(),
        action_log_entries: action_log.len(),
    })
}

async fn map_view(
    State(state): State<SharedState>,
    Query(query): Query<MapViewQuery>,
) -> Result<Response, (StatusCode, String)> {
    let player_id = query.player_id.unwrap_or(1);
    let x = query.x.unwrap_or_default();
    let y = query.y.unwrap_or_default();
    let debug_map_view = state.inner().config.debug_max_actions.is_some();
    let max_radius = if debug_map_view {
        MAX_DEBUG_MAP_VIEW_RADIUS
    } else {
        MAX_WORLD_RADIUS
    };
    let radius = query
        .radius
        .unwrap_or(DEFAULT_WORLD_RADIUS)
        .clamp(0, max_radius);
    let center = Position::new(x, y);

    let world = state.inner().world.lock().await;
    if !world.players.contains_key(&player_id) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("player {player_id} has no map view permission"),
        ));
    }
    if let Some(map_id) = query.map_id
        && map_id != world.map_id
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "map_id {map_id} is not available; current map_id is {}",
                world.map_id
            ),
        ));
    }

    let body = render_ascii_map(&world, player_id, center, radius, debug_map_view);
    Ok(([(CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response())
}

async fn world_region(
    State(state): State<SharedState>,
    Query(query): Query<WorldQuery>,
) -> Json<WorldResponse> {
    let x = query.x.unwrap_or_default();
    let y = query.y.unwrap_or_default();
    let radius = query
        .radius
        .unwrap_or(DEFAULT_WORLD_RADIUS)
        .clamp(0, MAX_WORLD_RADIUS);
    let center = Position::new(x, y);
    let world = state.inner().world.lock().await;
    let mut tiles = Vec::new();

    for ty in y - radius..=y + radius {
        for tx in x - radius..=x + radius {
            let position = Position::new(tx, ty);
            if center.manhattan(position) <= radius as u32 {
                tiles.push(world.tile_at(position));
            }
        }
    }

    Json(WorldResponse {
        tick: world.tick,
        center,
        radius,
        tiles,
    })
}

async fn entities(State(state): State<SharedState>) -> Json<Vec<Entity>> {
    let world = state.inner().world.lock().await;
    let mut entities = world.entities.values().cloned().collect::<Vec<_>>();
    entities.sort_by_key(|entity| entity.id);
    Json(entities)
}

async fn action_log(State(state): State<SharedState>) -> Json<Vec<ActionLogEntry>> {
    let action_log = state.inner().action_log.lock().await;
    Json(action_log.entries().to_vec())
}

async fn upload_bot(
    State(state): State<SharedState>,
    Query(query): Query<UploadQuery>,
    body: Bytes,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty bot upload".into()));
    }
    if body.len() > MAX_BOT_UPLOAD_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("bot upload exceeds {MAX_BOT_UPLOAD_BYTES} bytes"),
        ));
    }

    let player_id = query.player_id.unwrap_or(1);
    let bot_path = PathBuf::from(format!("var/bots/{player_id}/bot"));
    let parent = bot_path
        .parent()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "invalid bot path".into()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(internal_error)?;
    tokio::fs::write(&bot_path, &body)
        .await
        .map_err(internal_error)?;
    set_executable(&bot_path).map_err(internal_error)?;

    {
        let mut world = state.inner().world.lock().await;
        world
            .set_player_bot_path(player_id, bot_path.clone())
            .map_err(|err| (StatusCode::NOT_FOUND, err))?;
        storage::save_world(&world).map_err(internal_error)?;
    }

    Ok(Json(UploadResponse {
        player_id,
        path: bot_path.display().to_string(),
        bytes: body.len(),
    }))
}

#[cfg(unix)]
fn set_executable(path: &PathBuf) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn set_executable(_path: &PathBuf) -> std::io::Result<()> {
    Ok(())
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

#[derive(Debug, Deserialize)]
struct WorldQuery {
    x: Option<i32>,
    y: Option<i32>,
    radius: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct MapViewQuery {
    player_id: Option<u64>,
    map_id: Option<u32>,
    x: Option<i32>,
    y: Option<i32>,
    radius: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    player_id: Option<u64>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    tick: u64,
    players: usize,
    entities: usize,
    action_log_entries: usize,
}

#[derive(Debug, Serialize)]
struct WorldResponse {
    tick: u64,
    center: Position,
    radius: i32,
    tiles: Vec<Tile>,
}

#[derive(Debug, Serialize)]
struct UploadResponse {
    player_id: u64,
    path: String,
    bytes: usize,
}
