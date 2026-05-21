use std::collections::{HashSet, VecDeque};

use super::model::{Position, ResourceTile};

pub(crate) fn adjacent_resource_index(
    actor_pos: Position,
    resources: &[ResourceTile],
) -> Option<usize> {
    resources
        .iter()
        .enumerate()
        .filter(|(_, resource)| resource.amount > 0)
        .filter(|(_, resource)| manhattan(actor_pos, resource.position) == 1)
        .min_by_key(|(_, resource)| (resource.position.x, resource.position.y))
        .map(|(index, _)| index)
}

pub(crate) fn next_step_toward_resource(
    actor_pos: Position,
    resources: &[ResourceTile],
    passable_positions: &HashSet<Position>,
) -> Option<(usize, Position)> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back((actor_pos, None));
    visited.insert(actor_pos);

    while let Some((position, first_step)) = queue.pop_front() {
        if let Some(resource_index) = adjacent_resource_index(position, resources)
            && let Some(first_step) = first_step
        {
            return Some((resource_index, first_step));
        }

        for next in adjacent_positions(position) {
            if !passable_positions.contains(&next) || !visited.insert(next) {
                continue;
            }
            queue.push_back((next, first_step.or(Some(next))));
        }
    }

    None
}

fn adjacent_positions(position: Position) -> [Position; 4] {
    [
        Position {
            x: position.x - 1,
            y: position.y,
        },
        Position {
            x: position.x + 1,
            y: position.y,
        },
        Position {
            x: position.x,
            y: position.y - 1,
        },
        Position {
            x: position.x,
            y: position.y + 1,
        },
    ]
}

pub(crate) fn manhattan(a: Position, b: Position) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

#[cfg(test)]
fn nearest_resource_to_move(
    actor_pos: Position,
    resources: &[ResourceTile],
    passable_neighbors: &[Position],
) -> Option<(ResourceTile, Position)> {
    resources
        .iter()
        .filter(|tile| manhattan(actor_pos, tile.position) > 1)
        .filter_map(|tile| {
            best_step_toward(tile.position, passable_neighbors).map(|step| (*tile, step))
        })
        .min_by_key(|(tile, step)| {
            (
                manhattan(*step, tile.position),
                manhattan(actor_pos, tile.position),
            )
        })
}

#[cfg(test)]
fn best_step_toward(target: Position, passable_neighbors: &[Position]) -> Option<Position> {
    passable_neighbors
        .iter()
        .copied()
        .min_by_key(|position| manhattan(*position, target))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rand_game_common::fb::ResourceKind;

    use super::*;

    #[test]
    fn nearest_resource_to_move_ignores_actor_tile() {
        let actor_pos = Position { x: 0, y: 0 };
        let resources = [
            ResourceTile {
                position: actor_pos,
                resource: ResourceKind::Iron,
                amount: 100,
            },
            ResourceTile {
                position: Position { x: 3, y: 0 },
                resource: ResourceKind::Copper,
                amount: 100,
            },
        ];

        let passable_neighbors = [Position { x: 1, y: 0 }];
        let (_resource, target) =
            nearest_resource_to_move(actor_pos, &resources, &passable_neighbors)
                .expect("resource away from actor should be selected");

        assert_eq!(manhattan(actor_pos, target), 1);
    }

    #[test]
    fn nearest_resource_to_move_avoids_blocked_direct_step() {
        let actor_pos = Position { x: 1, y: 0 };
        let resources = [ResourceTile {
            position: Position { x: 0, y: -1 },
            resource: ResourceKind::Iron,
            amount: 100,
        }];
        let passable_neighbors = [Position { x: 2, y: 0 }, Position { x: 1, y: 1 }];

        let (_resource, target) =
            nearest_resource_to_move(actor_pos, &resources, &passable_neighbors)
                .expect("a passable neighbor should be selected");

        assert_eq!(manhattan(actor_pos, target), 1);
        assert_ne!(target, Position { x: 0, y: 0 });
    }

    #[test]
    fn next_step_toward_resource_finds_path_to_mining_position() {
        let actor_pos = Position { x: 0, y: 0 };
        let resources = [ResourceTile {
            position: Position { x: 3, y: 0 },
            resource: ResourceKind::Energy,
            amount: 80,
        }];
        let passable_positions = [
            Position { x: 1, y: 0 },
            Position { x: 2, y: 0 },
            Position { x: 2, y: 1 },
        ]
        .into_iter()
        .collect::<HashSet<_>>();

        let (resource_index, step) =
            next_step_toward_resource(actor_pos, &resources, &passable_positions)
                .expect("path to adjacent mining position");

        assert_eq!(resource_index, 0);
        assert_eq!(step, Position { x: 1, y: 0 });
    }

    #[test]
    fn adjacent_resource_index_ignores_depleted_resources() {
        let actor_pos = Position { x: 0, y: 0 };
        let resources = [
            ResourceTile {
                position: Position { x: 1, y: 0 },
                resource: ResourceKind::Energy,
                amount: 0,
            },
            ResourceTile {
                position: Position { x: 0, y: 1 },
                resource: ResourceKind::Iron,
                amount: 10,
            },
        ];

        assert_eq!(adjacent_resource_index(actor_pos, &resources), Some(1));
    }
}
