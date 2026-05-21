use crate::model::{ChunkCoord, MapKind, Position, ResourceKind, ResourceStack, Tile};

const RESOURCE_CLUSTER_SIZE: i32 = 12;
const RESOURCE_CLUSTER_GAP: i32 = 1;

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
    use crate::rules::{MAP_ID, WORLD_SEED};

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
}
