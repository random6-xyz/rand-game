use std::collections::HashSet;

use rand_game_common::fb::ResourceKind;

use super::*;

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

fn best_step_toward(target: Position, passable_neighbors: &[Position]) -> Option<Position> {
    passable_neighbors
        .iter()
        .copied()
        .min_by_key(|position| manhattan(*position, target))
}

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
    let (_resource, target) = nearest_resource_to_move(actor_pos, &resources, &passable_neighbors)
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

    let (_resource, target) = nearest_resource_to_move(actor_pos, &resources, &passable_neighbors)
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
