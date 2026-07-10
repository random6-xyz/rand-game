use rand_game_common::fb::ResourceKind;

use super::super::model::{ActionPlan, Actor, PlannedAction};
use super::BehaviorContext;

pub(crate) fn try_put_excess(
    actor: &mut Actor,
    _context: &mut BehaviorContext<'_>,
) -> Option<PlannedAction> {
    let total = actor.cargo.total_items();
    if total <= 50 {
        return None;
    }

    let amount = (total - 50).min(100);

    let (resource, amount) = if actor.cargo.iron > 0 {
        let amt = actor.cargo.iron.min(amount);
        actor.cargo.iron -= amt;
        (ResourceKind::Iron, amt)
    } else if actor.cargo.copper > 0 {
        let amt = actor.cargo.copper.min(amount);
        actor.cargo.copper -= amt;
        (ResourceKind::Copper, amt)
    } else if actor.cargo.stone > 0 {
        let amt = actor.cargo.stone.min(amount);
        actor.cargo.stone -= amt;
        (ResourceKind::Stone, amt)
    } else if actor.cargo.tree > 0 {
        let amt = actor.cargo.tree.min(amount);
        actor.cargo.tree -= amt;
        (ResourceKind::Tree, amt)
    } else if actor.cargo.water > 0 {
        let amt = actor.cargo.water.min(amount);
        actor.cargo.water -= amt;
        (ResourceKind::Water, amt)
    } else {
        return None;
    };

    eprintln!(
        "sample_bot: putting {:?} x{} with actor {} (cargo full at {})",
        resource, amount, actor.id, total
    );

    Some(PlannedAction {
        actor_id: actor.id,
        plan: ActionPlan::Put { resource, amount },
    })
}
