use std::path::PathBuf;

mod map_view;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
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

pub fn router(state: SharedState) -> Router {
    let max_bot_upload_bytes = state.inner().config.rules.max_bot_upload_bytes;
    Router::new()
        .route("/health", get(health))
        .route("/world", get(world_region))
        .route("/map-view", get(map_view))
        .route("/entities", get(entities))
        .route("/action-log", get(action_log))
        .route("/bot-stderr", get(bot_stderr))
        .route("/bots", post(upload_bot))
        .layer(DefaultBodyLimit::max(max_bot_upload_bytes))
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
        state.inner().config.rules.max_debug_map_view_radius
    } else {
        state.inner().config.rules.max_map_view_radius
    }
    .max(0);
    let radius = query
        .radius
        .unwrap_or(state.inner().config.rules.default_world_radius)
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
        .unwrap_or(state.inner().config.rules.default_world_radius)
        .clamp(0, state.inner().config.rules.max_world_radius.max(0));
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

async fn action_log(
    State(state): State<SharedState>,
    Query(query): Query<ActionLogQuery>,
) -> Json<ActionLogResponse> {
    let page_size = state.inner().config.rules.action_log_page_size.max(1);
    let limit = query.limit.unwrap_or(page_size).clamp(1, page_size * 10);
    let offset = query.offset.unwrap_or(0);
    let action_log = state.inner().action_log.lock().await;
    let entries = action_log.entries();
    let total = entries.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let slice = entries[start..end].to_vec();
    Json(ActionLogResponse {
        entries: slice,
        total,
        limit,
        offset,
    })
}

async fn bot_stderr(
    State(state): State<SharedState>,
    Query(query): Query<BotStderrQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| stream_bot_stderr(socket, state, query.player_id))
}

async fn stream_bot_stderr(mut socket: WebSocket, state: SharedState, player_id: Option<u64>) {
    let mut receiver = state.inner().bot_stderr.subscribe();

    loop {
        let event = match receiver.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                eprintln!("bot-stderr channel lagged: dropped {n} events");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        if player_id.is_some_and(|player_id| player_id != event.player_id) {
            continue;
        }

        let Ok(text) = serde_json::to_string(&event) else {
            continue;
        };
        if socket.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
}

async fn upload_bot(
    State(state): State<SharedState>,
    Query(query): Query<UploadQuery>,
    body: Bytes,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    if !state.inner().config.rules.enable_bot_upload {
        return Err((StatusCode::FORBIDDEN, "bot upload is disabled".into()));
    }
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty bot upload".into()));
    }
    let max_bot_upload_bytes = state.inner().config.rules.max_bot_upload_bytes;
    if body.len() > max_bot_upload_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("bot upload exceeds {max_bot_upload_bytes} bytes"),
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

    let world_snapshot = {
        let mut world = state.inner().world.lock().await;
        world
            .set_player_bot_path(player_id, bot_path.clone())
            .map_err(|err| (StatusCode::NOT_FOUND, err))?;
        world.clone()
    };
    storage::save_world(&world_snapshot).map_err(internal_error)?;

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

#[derive(Debug, Deserialize)]
struct BotStderrQuery {
    player_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ActionLogQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ActionLogResponse {
    entries: Vec<ActionLogEntry>,
    total: usize,
    limit: usize,
    offset: usize,
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

#[cfg(test)]
mod tests {
    use crate::rules::ServerRules;

    #[test]
    fn enable_bot_upload_defaults_to_false() {
        let rules = ServerRules::default();
        assert!(!rules.enable_bot_upload);
    }

    #[test]
    fn enable_bot_upload_from_toml_true() {
        let toml = "enable_bot_upload = true";
        let rules: ServerRules = toml::from_str(toml).expect("parse should succeed");
        assert!(rules.enable_bot_upload);
    }
}
