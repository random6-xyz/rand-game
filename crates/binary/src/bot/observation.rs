use std::collections::{HashMap, HashSet};

use rand_game_common::fb::*;

use super::model::{Actor, Position, ResourceTile, SmallCargo};

pub(crate) fn ready_actors(observation: Observation<'_>) -> Vec<Actor> {
    let mut actors = Vec::new();
    let Some(entities) = observation.owned_entities() else {
        return actors;
    };

    for index in 0..entities.len() {
        let entity = entities.get(index);
        let Some(position) = entity.position().map(to_position) else {
            continue;
        };
        let mut cargo = SmallCargo::default();
        if let Some(items) = entity.cargo() {
            for i in 0..items.len() {
                let stack = items.get(i);
                match stack.kind() {
                    Some("iron-ore") => cargo.iron += stack.amount(),
                    Some("copper-ore") => cargo.copper += stack.amount(),
                    Some("energy") => cargo.energy += stack.amount(),
                    Some("stone") => cargo.stone += stack.amount(),
                    Some("tree") => cargo.tree += stack.amount(),
                    Some("water") => cargo.water += stack.amount(),
                    _ => {}
                }
            }
        }
        actors.push(Actor {
            id: entity.id(),
            position,
            cargo,
        });
    }

    actors.sort_by_key(|actor| actor.id);
    actors
}

pub(crate) fn visible_resource_tiles(observation: Observation<'_>) -> Vec<ResourceTile> {
    let mut resources = Vec::new();
    let Some(tiles) = observation.visible_tiles() else {
        return resources;
    };

    for index in 0..tiles.len() {
        let tile = tiles.get(index);
        let Some(resource) = tile.resource() else {
            continue;
        };
        if resource.kind() == ResourceKind::None || resource.amount() == 0 {
            continue;
        }
        let Some(position) = tile.position() else {
            continue;
        };
        resources.push(ResourceTile {
            position: to_position(position),
            resource: resource.kind(),
            amount: resource.amount(),
        });
    }

    resources
}

pub(crate) fn visible_passable_positions(observation: Observation<'_>) -> HashSet<Position> {
    let mut positions = HashSet::new();
    let Some(tiles) = observation.visible_tiles() else {
        return positions;
    };

    for index in 0..tiles.len() {
        let tile = tiles.get(index);
        let Some(position) = tile.position().map(to_position) else {
            continue;
        };
        if tile.building().is_some() {
            continue;
        }

        positions.insert(position);
    }

    positions
}

fn to_position(position: &Vec2I) -> Position {
    Position {
        x: position.x(),
        y: position.y(),
    }
}

pub(crate) fn worker_cargo_map(observation: Observation<'_>) -> HashMap<String, u32> {
    let mut cargo = HashMap::new();
    let Some(entities) = observation.owned_entities() else {
        return cargo;
    };
    for index in 0..entities.len() {
        let entity = entities.get(index);
        if let Some(items) = entity.cargo() {
            for i in 0..items.len() {
                let stack = items.get(i);
                if let Some(kind) = stack.kind() {
                    *cargo.entry(kind.to_string()).or_default() += stack.amount();
                }
            }
        }
    }
    cargo
}

pub(crate) fn worker_entity(observation: Observation<'_>) -> Option<(u64, Position)> {
    let entities = observation.owned_entities()?;
    let mut best_id: u64 = 0;
    let mut best_position: Option<Position> = None;
    for index in 0..entities.len() {
        let entity = entities.get(index);
        let id = entity.id();
        if id < best_id {
            continue;
        }
        let Some(pos) = entity.position() else {
            continue;
        };
        best_id = id;
        best_position = Some(to_position(pos));
    }
    best_position.map(|pos| (best_id, pos))
}
