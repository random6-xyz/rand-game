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
#[path = "pathfinding_tests.rs"]
mod pathfinding_tests;
