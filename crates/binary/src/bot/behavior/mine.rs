use super::super::model::{ActionPlan, Actor, PlannedAction};
use super::super::pathfinding::adjacent_resource_index;
use super::BehaviorContext;

pub(crate) fn try_mine_adjacent(
    actor: &mut Actor,
    context: &mut BehaviorContext<'_>,
) -> Option<PlannedAction> {
    let resource_index = adjacent_resource_index(actor.position, context.resources)?;
    let simulate_effects = context.should_simulate_effects();
    let resource = &mut context.resources[resource_index];
    let amount = resource.amount.min(25);

    eprintln!(
        "sample_bot: mining {:?} x{} at ({}, {}) with actor {}",
        resource.resource, amount, resource.position.x, resource.position.y, actor.id
    );

    let action = PlannedAction {
        actor_id: actor.id,
        plan: ActionPlan::Mine {
            target: resource.position,
            resource: resource.resource,
            amount,
        },
    };

    if simulate_effects {
        resource.amount -= amount;
    }

    Some(action)
}
