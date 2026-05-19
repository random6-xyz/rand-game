use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::action_log::ActionLogEntry;
use crate::model::{Entity, EntityKind, Position, ResourceKind, TerrainKind, Tile};
use crate::state::SharedState;
use crate::storage;

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

fn render_ascii_map(
    world: &crate::world::WorldState,
    player_id: u64,
    center: Position,
    radius: i32,
    reveal_all: bool,
) -> String {
    let visible_positions = world
        .visible_tiles_for(player_id)
        .into_iter()
        .map(|tile| tile.position)
        .collect::<HashSet<_>>();
    let entities_by_position = world
        .entities
        .values()
        .map(|entity| (entity.position, entity.kind))
        .collect::<HashMap<_, _>>();

    let mut output = String::new();
    output.push_str(&format!(
        "tick={} map_id={} player_id={} center=({}, {}) radius={} reveal_all={}\n",
        world.tick, world.map_id, player_id, center.x, center.y, radius, reveal_all
    ));
    output.push_str(
        "legend: C core, W worker, B building, i iron, c copper, e energy, . plain, r rock, ~ water, ^ mountain, x ruin, ! danger, ? unseen\n",
    );

    for ty in (center.y - radius..=center.y + radius).rev() {
        output.push_str(&format!("{ty:>5} "));
        for tx in center.x - radius..=center.x + radius {
            let position = Position::new(tx, ty);
            output.push(tile_glyph(
                world,
                &visible_positions,
                &entities_by_position,
                position,
                reveal_all,
            ));
        }
        output.push('\n');
    }
    output.push_str("      ");
    for tx in center.x - radius..=center.x + radius {
        output.push(if tx == center.x { '+' } else { '-' });
    }
    output.push('\n');
    output
}

fn tile_glyph(
    world: &crate::world::WorldState,
    visible_positions: &HashSet<Position>,
    entities_by_position: &HashMap<Position, EntityKind>,
    position: Position,
    reveal_all: bool,
) -> char {
    if !reveal_all && !visible_positions.contains(&position) {
        return '?';
    }
    if let Some(entity_kind) = entities_by_position.get(&position) {
        return match entity_kind {
            EntityKind::Core => 'C',
            EntityKind::Worker => 'W',
        };
    }

    let tile = world.tile_at(position);
    if tile.building_id.is_some() {
        return 'B';
    }
    if tile.danger_level >= 20 {
        return '!';
    }
    if let Some(resource) = tile.resource {
        return match resource.kind {
            ResourceKind::Iron => 'i',
            ResourceKind::Copper => 'c',
            ResourceKind::Energy => 'e',
        };
    }
    match tile.terrain {
        TerrainKind::Plain => '.',
        TerrainKind::Rock => 'r',
        TerrainKind::Water => '~',
        TerrainKind::Mountain => '^',
        TerrainKind::Ruin => 'x',
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::WorldState;

    #[test]
    fn ascii_map_renders_visible_owned_entities() {
        let world = WorldState::new();

        let map = render_ascii_map(&world, 1, Position::new(0, 0), 2, false);

        assert!(map.contains("player_id=1"));
        assert!(map.contains('C'));
        assert!(map.contains('W'));
    }

    #[test]
    fn ascii_map_debug_mode_reveals_tiles_outside_visibility() {
        let world = WorldState::new();

        let hidden_map = render_ascii_map(&world, 1, Position::new(100, 100), 1, false);
        let debug_map = render_ascii_map(&world, 1, Position::new(100, 100), 1, true);

        assert!(hidden_map.contains('?'));
        assert!(!debug_map.lines().skip(2).any(|line| line.contains('?')));
        assert!(debug_map.contains("reveal_all=true"));
    }
}
