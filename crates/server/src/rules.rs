use rand_game_common::fb;

use crate::model::{
    ChunkCoord, MapKind, Position, ResourceKind, ResourceStack, Tile, ValidatedAction,
};
use crate::protocol;
use crate::world::WorldState;

pub const WORLD_SEED: u64 = 0x5241_4e44_4741_4d45;
pub const MAP_ID: u32 = 0;
pub const OBSERVATION_RADIUS: u32 = 8;
pub const MAX_MINE_AMOUNT: u32 = 1;

const RESOURCE_CLUSTER_SIZE: i32 = 12;
const RESOURCE_CLUSTER_GAP: i32 = 1;

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub actions: Vec<ValidatedAction>,
    pub rejected: Vec<String>,
    pub persistent_memory: Option<Vec<u8>>,
}

pub fn validate_game_output(
    world: &WorldState,
    player_id: u64,
    output_payload: &[u8],
    debug_max_actions: Option<u32>,
) -> Result<ValidationReport, Box<dyn std::error::Error>> {
    let output = fb::root_as_game_output(output_payload)?;
    let mut report = ValidationReport::default();

    if output.protocol_version() != fb::ProtocolVersion::V1 {
        report.rejected.push("unsupported protocol_version".into());
        return Ok(report);
    }

    let runtime_profile = world
        .player_runtime_profile(player_id)
        .ok_or("player has no runtime profile")?;
    let max_actions = debug_max_actions.unwrap_or(runtime_profile.max_actions);

    if let Some(memory) = output.persistent_memory() {
        if memory.len() > runtime_profile.max_persistent_memory_bytes as usize {
            report.rejected.push(format!(
                "persistent memory {} bytes exceeds max {}",
                memory.len(),
                runtime_profile.max_persistent_memory_bytes
            ));
        } else {
            report.persistent_memory = Some(memory.bytes().to_vec());
        }
    }

    let Some(actions) = output.actions() else {
        return Ok(report);
    };
    if actions.len() > max_actions as usize {
        report.rejected.push(format!(
            "action count {} exceeds max {}",
            actions.len(),
            max_actions
        ));
    }

    for index in 0..actions.len().min(max_actions as usize) {
        let action = actions.get(index);
        match validate_action(world, player_id, action) {
            Ok(action) => report.actions.push(action),
            Err(reason) => report.rejected.push(format!("action {index}: {reason}")),
        }
    }

    Ok(report)
}

pub fn validate_action(
    world: &WorldState,
    player_id: u64,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let actor = world
        .entities
        .get(&action.actor_entity_id())
        .ok_or("actor entity does not exist")?;
    if actor.owner_id != player_id {
        return Err("actor entity is not owned by player".into());
    }
    match action.kind() {
        fb::ActionKind::Move => validate_move(world, actor.position, action),
        fb::ActionKind::Mine => validate_mine(world, actor.position, action),
        fb::ActionKind::Build => validate_build(world, player_id, actor.position, action),
        fb::ActionKind::Lift => validate_lift(world, actor.position, action),
        fb::ActionKind::Put => validate_put(actor.cargo.as_slice(), action),
        other => Err(format!("unsupported action kind {other:?}")),
    }
}

fn validate_move(
    world: &WorldState,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let target = required_target_position(action, "Move")?;
    if actor_position.manhattan(target) != 1 {
        return Err("move target must be orthogonally adjacent".into());
    }
    if !world.is_passable(target) {
        return Err("move target is blocked".into());
    }

    Ok(ValidatedAction::Move {
        actor_entity_id: action.actor_entity_id(),
        target,
    })
}

fn validate_mine(
    world: &WorldState,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let target = required_target_position(action, "Mine")?;
    if actor_position.manhattan(target) != 1 {
        return Err("mine target must be orthogonally adjacent".into());
    }
    let tile = world.tile_at(target);
    let resource = tile.resource.ok_or("mine target has no resource")?;
    let requested = action.amount().clamp(1, MAX_MINE_AMOUNT);
    if requested > resource.amount {
        return Err("mine amount exceeds remaining resource".into());
    }

    Ok(ValidatedAction::Mine {
        actor_entity_id: action.actor_entity_id(),
        target,
        amount: requested,
    })
}

fn validate_build(
    world: &WorldState,
    player_id: u64,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let target = required_target_position(action, "Build")?;
    if actor_position.manhattan(target) != 1 {
        return Err("build target must be orthogonally adjacent".into());
    }
    if !world.is_passable(target) {
        return Err("build target is not empty and passable".into());
    }
    let near_owned_core = world.buildings.values().any(|building| {
        building.owner_id == player_id
            && building.kind == crate::model::BuildingKind::None
            && building.position.manhattan(target) <= 4
    });
    if !near_owned_core {
        return Err("build target must be near owned core".into());
    }
    let building_kind = protocol::to_model_building_kind(action.building_kind())
        .ok_or("build action has invalid building kind")?;
    if building_kind == crate::model::BuildingKind::None {
        return Err("building another core is not allowed in MVP".into());
    }

    Ok(ValidatedAction::Build {
        actor_entity_id: action.actor_entity_id(),
        target,
        building_kind,
    })
}

fn required_target_position(action: fb::Action<'_>, kind: &str) -> Result<Position, String> {
    action
        .target_position()
        .map(protocol::to_model_position)
        .ok_or_else(|| format!("{kind:?} action requires target_position"))
}

fn validate_lift(
    world: &WorldState,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let tile = world.tile_at(actor_position);
    let Some(resource) = tile.resource else {
        return Err("lift target tile has no resource".into());
    };
    let fb_resource = action
        .resource()
        .ok_or("lift action requires resource field")?;
    let kind = to_model_resource_kind(fb_resource.kind())
        .ok_or("lift action has invalid resource kind")?;
    if resource.kind != kind {
        return Err("lift resource kind does not match tile resource kind".into());
    }
    let amount = action.amount().clamp(1, MAX_MINE_AMOUNT);
    if amount > resource.amount {
        return Err("lift amount exceeds remaining resource".into());
    }

    Ok(ValidatedAction::Lift {
        actor_entity_id: action.actor_entity_id(),
        kind,
        amount,
    })
}

fn validate_put(
    actor_cargo: &[ResourceStack],
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let fb_resource = action
        .resource()
        .ok_or("put action requires resource field")?;
    let kind =
        to_model_resource_kind(fb_resource.kind()).ok_or("put action has invalid resource kind")?;
    let available = actor_cargo
        .iter()
        .filter(|stack| stack.kind == kind)
        .map(|stack| stack.amount)
        .sum::<u32>();
    if available == 0 {
        return Err("put resource kind not in actor cargo".into());
    }
    let amount = action.amount().clamp(1, available);

    Ok(ValidatedAction::Put {
        actor_entity_id: action.actor_entity_id(),
        kind,
        amount,
    })
}

fn to_model_resource_kind(kind: fb::ResourceKind) -> Option<ResourceKind> {
    match kind {
        fb::ResourceKind::Iron => Some(ResourceKind::Iron),
        fb::ResourceKind::Copper => Some(ResourceKind::Copper),
        fb::ResourceKind::Energy => Some(ResourceKind::Energy),
        fb::ResourceKind::Stone => Some(ResourceKind::Stone),
        fb::ResourceKind::Tree => Some(ResourceKind::Tree),
        fb::ResourceKind::Water => Some(ResourceKind::Water),
        _ => None,
    }
}

pub fn generated_tile(world_seed: u64, map_id: u32, map_kind: MapKind, position: Position) -> Tile {
    let sample = hash_position(world_seed, map_id, position);
    let resource = generated_resource(world_seed, map_id, position, sample, map_kind);

    Tile {
        position,
        resource,
        building_id: None,
        owner_id: None,
    }
}

fn generated_resource(
    world_seed: u64,
    map_id: u32,
    position: Position,
    sample: u64,
    map_kind: MapKind,
) -> Option<ResourceStack> {
    let cell_x = position.x.div_euclid(RESOURCE_CLUSTER_SIZE);
    let cell_y = position.y.div_euclid(RESOURCE_CLUSTER_SIZE);
    let mut best_cluster: Option<ResourceCluster> = None;
    let mut best_distance = i32::MAX;

    for y in (cell_y - 1)..=(cell_y + 1) {
        for x in (cell_x - 1)..=(cell_x + 1) {
            let cluster = resource_cluster_at(world_seed, map_id, map_kind, x, y);
            let distance = squared_distance(position, cluster.center);
            if distance <= cluster.radius * cluster.radius && distance < best_distance {
                best_cluster = Some(cluster);
                best_distance = distance;
            }
        }
    }

    let cluster = best_cluster?;
    for y in (cell_y - 1)..=(cell_y + 1) {
        for x in (cell_x - 1)..=(cell_x + 1) {
            let other = resource_cluster_at(world_seed, map_id, map_kind, x, y);
            if other.kind == cluster.kind {
                continue;
            }

            let distance = squared_distance(position, other.center);
            let gap_radius = other.radius + RESOURCE_CLUSTER_GAP;
            if distance <= gap_radius * gap_radius {
                return None;
            }
        }
    }

    let amount = 5000 + ((sample >> 16) % 1000) as u32;

    Some(ResourceStack {
        kind: cluster.kind,
        amount,
    })
}

#[derive(Debug, Clone, Copy)]
struct ResourceCluster {
    center: Position,
    radius: i32,
    kind: ResourceKind,
}

fn resource_cluster_at(
    world_seed: u64,
    map_id: u32,
    map_kind: MapKind,
    cell_x: i32,
    cell_y: i32,
) -> ResourceCluster {
    let sample = hash_chunk(
        world_seed,
        map_id,
        ChunkCoord {
            x: cell_x,
            y: cell_y,
        },
        0x7265_736f_7572_6365,
    );
    let padding = 2;
    let spread = (RESOURCE_CLUSTER_SIZE - padding * 2) as u64;
    let center = Position::new(
        cell_x * RESOURCE_CLUSTER_SIZE + padding + ((sample >> 8) % spread) as i32,
        cell_y * RESOURCE_CLUSTER_SIZE + padding + ((sample >> 16) % spread) as i32,
    );
    let radius_span = if matches!(map_kind, MapKind::Resource) {
        2
    } else {
        1
    };
    let min_radius = if matches!(map_kind, MapKind::Resource) {
        4
    } else {
        2
    };
    let base_radius = min_radius + ((sample >> 24) % radius_span) as i32;
    let base_kind = match sample % 6 {
        0 => ResourceKind::Iron,
        1 => ResourceKind::Copper,
        2 => ResourceKind::Energy,
        3 => ResourceKind::Stone,
        4 => ResourceKind::Tree,
        _ => ResourceKind::Water,
    };

    let (kind, radius) = match base_kind {
        ResourceKind::Water => {
            if (sample >> 32).is_multiple_of(5) {
                (ResourceKind::Water, base_radius)
            } else {
                (ResourceKind::Energy, base_radius)
            }
        }
        ResourceKind::Tree => {
            if (sample >> 32).is_multiple_of(5) {
                (ResourceKind::Tree, base_radius)
            } else {
                (ResourceKind::Copper, base_radius)
            }
        }
        ResourceKind::Stone => {
            if (sample >> 32).is_multiple_of(3) {
                (ResourceKind::Stone, (base_radius * 2 / 3).max(1))
            } else {
                (ResourceKind::Iron, base_radius)
            }
        }
        other => (other, base_radius),
    };

    ResourceCluster {
        center,
        radius,
        kind,
    }
}

fn squared_distance(a: Position, b: Position) -> i32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

pub fn hash_chunk(world_seed: u64, map_id: u32, chunk: ChunkCoord, window: u64) -> u64 {
    let mut value =
        world_seed ^ ((map_id as u64) << 32) ^ window.wrapping_mul(0x517c_c1b7_2722_0a95);
    value ^= (chunk.x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= (chunk.y as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    splitmix64(value)
}

fn hash_position(world_seed: u64, map_id: u32, position: Position) -> u64 {
    let mut value = world_seed ^ ((map_id as u64) << 32);
    value ^= (position.x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= (position.y as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    splitmix64(value)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::WorldState;

    #[test]
    fn generated_tiles_are_deterministic() {
        let position = Position::new(12, -5);

        assert_eq!(
            generated_tile(WORLD_SEED, MAP_ID, MapKind::Resource, position),
            generated_tile(WORLD_SEED, MAP_ID, MapKind::Resource, position)
        );
    }

    #[test]
    fn generated_resources_cluster_by_kind() {
        for y in -24..=24 {
            for x in -24..=24 {
                let position = Position::new(x, y);
                let Some(resource) = generated_resource(
                    WORLD_SEED,
                    MAP_ID,
                    position,
                    hash_position(WORLD_SEED, MAP_ID, position),
                    MapKind::Resource,
                ) else {
                    continue;
                };

                let neighbor = Position::new(position.x + 1, position.y);
                let Some(neighbor_resource) = generated_resource(
                    WORLD_SEED,
                    MAP_ID,
                    neighbor,
                    hash_position(WORLD_SEED, MAP_ID, neighbor),
                    MapKind::Resource,
                ) else {
                    continue;
                };

                if resource.kind == neighbor_resource.kind {
                    return;
                }
            }
        }

        panic!("expected at least one adjacent same-kind resource tile");
    }

    #[test]
    fn generated_resources_leave_gaps_between_different_kinds() {
        for y in -24..=24 {
            for x in -24..=24 {
                let position = Position::new(x, y);
                let Some(resource) = generated_resource(
                    WORLD_SEED,
                    MAP_ID,
                    position,
                    hash_position(WORLD_SEED, MAP_ID, position),
                    MapKind::Resource,
                ) else {
                    continue;
                };

                let cell_x = position.x.div_euclid(RESOURCE_CLUSTER_SIZE);
                let cell_y = position.y.div_euclid(RESOURCE_CLUSTER_SIZE);
                for other_y in (cell_y - 1)..=(cell_y + 1) {
                    for other_x in (cell_x - 1)..=(cell_x + 1) {
                        let other = resource_cluster_at(
                            WORLD_SEED,
                            MAP_ID,
                            MapKind::Resource,
                            other_x,
                            other_y,
                        );
                        if resource.kind == other.kind {
                            continue;
                        }

                        let distance = squared_distance(position, other.center);
                        let gap_radius = other.radius + RESOURCE_CLUSTER_GAP;
                        assert!(distance > gap_radius * gap_radius);
                    }
                }
            }
        }
    }

    #[test]
    fn debug_action_validation_ignores_actor_cooldown() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let actor_id = player.worker_entity_id;

        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let target = fb::Vec2I::new(1, 0);
        let action = fb::Action::create(
            &mut fbb,
            &fb::ActionArgs {
                kind: fb::ActionKind::Lift,
                actor_entity_id: actor_id,
                target_position: Some(&target),
                ..Default::default()
            },
        );
        fbb.finish_minimal(action);
        let action = flatbuffers::root::<fb::Action<'_>>(fbb.finished_data()).expect("action");

        assert_eq!(
            validate_action(&world, 1, action),
            Err("lift action requires resource field".into())
        );
    }
}
