mod mine;
mod navigate;

use std::collections::HashSet;

use super::model::{Actor, PlannedAction, Position, ResourceTile};

type Behavior = fn(&mut Actor, &mut BehaviorContext<'_>) -> Option<PlannedAction>;

const BEHAVIORS: &[Behavior] = &[mine::try_mine_adjacent, navigate::try_move_toward_resource];

pub(crate) struct BehaviorContext<'a> {
    pub(crate) resources: &'a mut [ResourceTile],
    pub(crate) passable_positions: &'a HashSet<Position>,
    simulate_effects: bool,
}

impl<'a> BehaviorContext<'a> {
    pub(crate) fn new(
        resources: &'a mut [ResourceTile],
        passable_positions: &'a HashSet<Position>,
        simulate_effects: bool,
    ) -> Self {
        Self {
            resources,
            passable_positions,
            simulate_effects,
        }
    }

    pub(crate) fn should_simulate_effects(&self) -> bool {
        self.simulate_effects
    }
}

pub(crate) fn plan_next_actor_action(
    actor: &mut Actor,
    context: &mut BehaviorContext<'_>,
) -> Option<PlannedAction> {
    for behavior in BEHAVIORS {
        if let Some(action) = behavior(actor, context) {
            return Some(action);
        }
    }

    None
}
