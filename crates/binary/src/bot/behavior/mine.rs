use rand_game_common::fb::ResourceKind;

use super::super::model::{ActionPlan, Actor, PlannedAction, SmallCargo};
use super::super::pathfinding::{adjacent_resource_index, adjacent_resource_index_by_kind};
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
        add_to_cargo(&mut actor.cargo, resource.resource, amount);
    }

    Some(action)
}

pub(crate) fn try_mine_specific(
    actor: &mut Actor,
    context: &mut BehaviorContext<'_>,
    kind: ResourceKind,
) -> Option<PlannedAction> {
    let resource_index = adjacent_resource_index_by_kind(actor.position, context.resources, kind)?;
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
        add_to_cargo(&mut actor.cargo, resource.resource, amount);
    }

    Some(action)
}

fn add_to_cargo(cargo: &mut SmallCargo, kind: ResourceKind, amount: u32) {
    match kind {
        ResourceKind::Iron => cargo.iron += amount,
        ResourceKind::Copper => cargo.copper += amount,
        ResourceKind::Energy => cargo.energy += amount,
        ResourceKind::Stone => cargo.stone += amount,
        ResourceKind::Tree => cargo.tree += amount,
        ResourceKind::Water => cargo.water += amount,
        _ => {}
    }
}
