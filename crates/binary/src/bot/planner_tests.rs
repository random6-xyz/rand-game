use std::collections::HashSet;

use rand_game_common::fb::ResourceKind;

use crate::bot::model::ActionPlan;

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
