use std::collections::{HashSet, VecDeque};
use std::io::{self, Write};

use flatbuffers::FlatBufferBuilder;
use rand_game_common::fb::*;
use rand_game_common::framing::{FrameKind, read_frame, write_frame};

const DEBUG_SIMULATION_MIN_ACTIONS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Position {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy)]
struct ResourceTile {
    position: Position,
    resource: ResourceKind,
    amount: u32,
}

#[derive(Debug, Clone, Copy)]
struct Actor {
    id: u64,
    position: Position,
}

pub fn run_sample_bot() -> Result<(), Box<dyn std::error::Error>> {
    let input_payload = read_frame(io::stdin().lock(), FrameKind::GameInput)?;

    if !game_input_buffer_has_identifier(&input_payload) {
        return Err("stdin frame payload is not a BWI1 GameInput flatbuffer".into());
    }

    let game_input = root_as_game_input(&input_payload)?;
    let output_payload = build_game_output(game_input)?;

    if !game_output_buffer_has_identifier(&output_payload) {
        return Err("generated payload is not a BWO1 GameOutput flatbuffer".into());
    }
    root_as_game_output(&output_payload)?;

    write_frame(io::stdout().lock(), FrameKind::GameOutput, &output_payload)?;
    io::stdout().lock().flush()?;

    Ok(())
}

fn build_game_output(input: GameInput<'_>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let observation = match input.observation() {
        Some(observation) => observation,
        None => return empty_output("missing observation"),
    };

    let max_actions = input
        .runtime_limits()
        .and_then(|limits| limits.action_limits())
        .map(|limits| limits.max_actions() as usize)
        .unwrap_or(1);
    if max_actions == 0 {
        return empty_output("max_actions is zero");
    }

    let actors = ready_actors(observation);
    if actors.is_empty() {
        return empty_output("no ready owned worker or core entity");
    };

    let resources = visible_resource_tiles(observation);
    let passable_positions = visible_passable_positions(observation);
    let actions = if max_actions >= DEBUG_SIMULATION_MIN_ACTIONS {
        plan_debug_simulation_actions(actors, resources, passable_positions, max_actions)
    } else {
        plan_single_tick_actions(&actors, &resources, &passable_positions, max_actions)
    };

    if actions.is_empty() {
        return empty_output("no profitable action found");
    }

    Ok(build_output_with_actions(actions))
}

fn ready_actors(observation: Observation<'_>) -> Vec<Actor> {
    let mut actors = Vec::new();
    let Some(entities) = observation.owned_entities() else {
        return actors;
    };

    for index in 0..entities.len() {
        let entity = entities.get(index);
        let Some(position) = entity.position().map(to_position) else {
            continue;
        };
        actors.push(Actor {
            id: entity.id(),
            position,
        });
    }

    actors.sort_by_key(|actor| actor.id);
    actors
}

fn plan_single_tick_actions(
    actors: &[Actor],
    resources: &[ResourceTile],
    passable_positions: &HashSet<Position>,
    max_actions: usize,
) -> Vec<PlannedAction> {
    let mut actions = Vec::new();

    for actor in actors {
        if actions.len() >= max_actions {
            break;
        }

        if let Some(resource_index) = adjacent_resource_index(actor.position, resources) {
            let resource = resources[resource_index];
            let amount = resource.amount.min(25);
            eprintln!(
                "sample_bot: mining {:?} x{} at ({}, {}) with actor {}",
                resource.resource, amount, resource.position.x, resource.position.y, actor.id
            );
            actions.push(PlannedAction {
                actor_id: actor.id,
                plan: ActionPlan::Mine {
                    target: resource.position,
                    resource: resource.resource,
                    amount,
                },
            });
            continue;
        }

        if let Some((resource_index, target)) =
            next_step_toward_resource(actor.position, resources, passable_positions)
        {
            let resource = resources[resource_index];
            eprintln!(
                "sample_bot: moving actor {} from ({}, {}) toward resource at ({}, {}) via ({}, {})",
                actor.id,
                actor.position.x,
                actor.position.y,
                resource.position.x,
                resource.position.y,
                target.x,
                target.y
            );
            actions.push(PlannedAction {
                actor_id: actor.id,
                plan: ActionPlan::Move { target },
            });
        }
    }

    actions
}

fn plan_debug_simulation_actions(
    mut actors: Vec<Actor>,
    mut resources: Vec<ResourceTile>,
    passable_positions: HashSet<Position>,
    max_actions: usize,
) -> Vec<PlannedAction> {
    let mut actions = Vec::new();

    while actions.len() < max_actions {
        let mut progressed = false;

        for actor in &mut actors {
            if actions.len() >= max_actions {
                break;
            }

            if let Some(resource_index) = adjacent_resource_index(actor.position, &resources) {
                let resource = &mut resources[resource_index];
                let amount = resource.amount.min(25);
                eprintln!(
                    "sample_bot: mining {:?} x{} at ({}, {}) with actor {}",
                    resource.resource, amount, resource.position.x, resource.position.y, actor.id
                );
                actions.push(PlannedAction {
                    actor_id: actor.id,
                    plan: ActionPlan::Mine {
                        target: resource.position,
                        resource: resource.resource,
                        amount,
                    },
                });
                resource.amount -= amount;
                progressed = true;
                continue;
            }

            if let Some((resource_index, target)) =
                next_step_toward_resource(actor.position, &resources, &passable_positions)
            {
                let resource = resources[resource_index];
                eprintln!(
                    "sample_bot: moving actor {} from ({}, {}) toward resource at ({}, {}) via ({}, {})",
                    actor.id,
                    actor.position.x,
                    actor.position.y,
                    resource.position.x,
                    resource.position.y,
                    target.x,
                    target.y
                );
                actions.push(PlannedAction {
                    actor_id: actor.id,
                    plan: ActionPlan::Move { target },
                });
                actor.position = target;
                progressed = true;
            }
        }

        if !progressed {
            break;
        }
    }

    actions
}

fn visible_resource_tiles(observation: Observation<'_>) -> Vec<ResourceTile> {
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

fn visible_passable_positions(observation: Observation<'_>) -> HashSet<Position> {
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

fn adjacent_resource_index(actor_pos: Position, resources: &[ResourceTile]) -> Option<usize> {
    resources
        .iter()
        .enumerate()
        .filter(|(_, resource)| resource.amount > 0)
        .filter(|(_, resource)| manhattan(actor_pos, resource.position) == 1)
        .min_by_key(|(_, resource)| (resource.position.x, resource.position.y))
        .map(|(index, _)| index)
}

fn next_step_toward_resource(
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

fn empty_output(reason: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    eprintln!("sample_bot: {reason}; emitting empty action list");
    Ok(build_output_without_actions(reason))
}

enum ActionPlan {
    Mine {
        target: Position,
        resource: ResourceKind,
        amount: u32,
    },
    Move {
        target: Position,
    },
}

struct PlannedAction {
    actor_id: u64,
    plan: ActionPlan,
}

fn build_output_with_actions(planned_actions: Vec<PlannedAction>) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let mut action_offsets = Vec::with_capacity(planned_actions.len());

    for planned_action in planned_actions {
        let action = match planned_action.plan {
            ActionPlan::Mine {
                target,
                resource,
                amount,
            } => {
                let target_position = Vec2I::new(target.x, target.y);
                let resource = ResourceStack::new(resource, amount);
                Action::create(
                    &mut fbb,
                    &ActionArgs {
                        kind: ActionKind::Mine,
                        actor_entity_id: planned_action.actor_id,
                        target_position: Some(&target_position),
                        resource: Some(&resource),
                        amount,
                        ..Default::default()
                    },
                )
            }
            ActionPlan::Move { target } => {
                let target_position = Vec2I::new(target.x, target.y);
                Action::create(
                    &mut fbb,
                    &ActionArgs {
                        kind: ActionKind::Move,
                        actor_entity_id: planned_action.actor_id,
                        target_position: Some(&target_position),
                        ..Default::default()
                    },
                )
            }
        };
        action_offsets.push(action);
    }

    let actions = fbb.create_vector(&action_offsets);
    let persistent_memory = fbb.create_vector::<u8>(&[]);
    let output = GameOutput::create(
        &mut fbb,
        &GameOutputArgs {
            protocol_version: ProtocolVersion::V1,
            actions: Some(actions),
            persistent_memory: Some(persistent_memory),
        },
    );

    finish_game_output_buffer(&mut fbb, output);
    fbb.finished_data().to_vec()
}

fn build_output_without_actions(_reason: &str) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let empty_actions: [flatbuffers::WIPOffset<Action<'_>>; 0] = [];
    let actions = fbb.create_vector(&empty_actions);
    let persistent_memory = fbb.create_vector::<u8>(&[]);
    let output = GameOutput::create(
        &mut fbb,
        &GameOutputArgs {
            protocol_version: ProtocolVersion::V1,
            actions: Some(actions),
            persistent_memory: Some(persistent_memory),
        },
    );

    finish_game_output_buffer(&mut fbb, output);
    fbb.finished_data().to_vec()
}

fn manhattan(a: Position, b: Position) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn to_position(position: &Vec2I) -> Position {
    Position {
        x: position.x(),
        y: position.y(),
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn single_tick_planner_does_not_chain_actor_movement() {
        let actors = [Actor {
            id: 3,
            position: Position { x: 0, y: 0 },
        }];
        let resources = [ResourceTile {
            position: Position { x: 3, y: 0 },
            resource: ResourceKind::Energy,
            amount: 80,
        }];
        let passable_positions = [Position { x: 1, y: 0 }, Position { x: 2, y: 0 }]
            .into_iter()
            .collect::<HashSet<_>>();

        let actions = plan_single_tick_actions(&actors, &resources, &passable_positions, 8);

        assert_eq!(actions.len(), 1);
        assert!(
            matches!(actions[0].plan, ActionPlan::Move { target } if target == Position { x: 1, y: 0 })
        );
    }

    #[test]
    fn debug_planner_chains_move_then_mine() {
        let actors = vec![Actor {
            id: 3,
            position: Position { x: 0, y: 0 },
        }];
        let resources = vec![ResourceTile {
            position: Position { x: 3, y: 0 },
            resource: ResourceKind::Energy,
            amount: 30,
        }];
        let passable_positions = [Position { x: 1, y: 0 }, Position { x: 2, y: 0 }]
            .into_iter()
            .collect::<HashSet<_>>();

        let actions = plan_debug_simulation_actions(actors, resources, passable_positions, 100);

        assert!(actions.len() >= 4);
        assert!(
            matches!(actions[0].plan, ActionPlan::Move { target } if target == Position { x: 1, y: 0 })
        );
        assert!(
            matches!(actions[1].plan, ActionPlan::Move { target } if target == Position { x: 2, y: 0 })
        );
        assert!(matches!(
            actions[2].plan,
            ActionPlan::Mine { amount: 25, .. }
        ));
        assert!(matches!(
            actions[3].plan,
            ActionPlan::Mine { amount: 5, .. }
        ));
    }
}
