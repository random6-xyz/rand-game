use std::collections::HashSet;

use super::model::{ActionPlan, Actor, PlannedAction, Position, ResourceTile};
use super::pathfinding::{adjacent_resource_index, next_step_toward_resource};

pub(crate) fn plan_single_tick_actions(
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

pub(crate) fn plan_debug_simulation_actions(
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

#[cfg(test)]
mod tests {
    use rand_game_common::fb::ResourceKind;

    use super::*;

    #[test]
    fn single_tick_planner_does_not_chain_actor_movement() {
        let actors = [Actor {
            id: 3,
            position: Position { x: 0, y: 0 },
            cargo: Default::default(),
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
            cargo: Default::default(),
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
