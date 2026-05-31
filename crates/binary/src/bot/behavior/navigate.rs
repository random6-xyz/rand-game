use rand_game_common::fb::ResourceKind;

use super::super::model::{ActionPlan, Actor, PlannedAction};
use super::super::pathfinding::{next_step_toward_resource, next_step_toward_resource_by_kind};
use super::BehaviorContext;

pub(crate) fn try_move_toward_resource(
    actor: &mut Actor,
    context: &mut BehaviorContext<'_>,
) -> Option<PlannedAction> {
    let (resource_index, target) = next_step_toward_resource(
        actor.position,
        context.resources,
        context.passable_positions,
    )?;
    let resource = context.resources[resource_index];

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

    let action = PlannedAction {
        actor_id: actor.id,
        plan: ActionPlan::Move { target },
    };

    if context.should_simulate_effects() {
        actor.position = target;
    }

    Some(action)
}

pub(crate) fn try_move_toward_specific_resource(
    actor: &mut Actor,
    context: &mut BehaviorContext<'_>,
    kind: ResourceKind,
) -> Option<PlannedAction> {
    let (resource_index, target) = next_step_toward_resource_by_kind(
        actor.position,
        context.resources,
        context.passable_positions,
        kind,
    )?;
    let resource = context.resources[resource_index];

    eprintln!(
        "sample_bot: moving actor {} from ({}, {}) toward {:?} resource at ({}, {}) via ({}, {})",
        actor.id,
        actor.position.x,
        actor.position.y,
        kind,
        resource.position.x,
        resource.position.y,
        target.x,
        target.y
    );

    let action = PlannedAction {
        actor_id: actor.id,
        plan: ActionPlan::Move { target },
    };

    if context.should_simulate_effects() {
        actor.position = target;
    }

    Some(action)
}
