use std::collections::HashSet;

use super::behavior::{BehaviorContext, plan_next_actor_action};
use super::model::{Actor, PlannedAction, Position, ResourceTile};

pub(crate) fn plan_single_tick_actions(
    actors: &[Actor],
    resources: &[ResourceTile],
    passable_positions: &HashSet<Position>,
    max_actions: usize,
) -> Vec<PlannedAction> {
    let mut actions = Vec::new();
    let mut resources = resources.to_vec();

    for actor in actors {
        if actions.len() >= max_actions {
            break;
        }

        let mut actor = actor.clone();
        let mut context = BehaviorContext::new(&mut resources, passable_positions, false);
        if let Some(action) = plan_next_actor_action(&mut actor, &mut context) {
            actions.push(action);
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

            let mut context = BehaviorContext::new(&mut resources, &passable_positions, true);
            if let Some(action) = plan_next_actor_action(actor, &mut context) {
                actions.push(action);
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
#[path = "planner_tests.rs"]
mod planner_tests;
