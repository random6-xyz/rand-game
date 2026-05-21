use rand_game_common::fb;

use crate::model::{Position, ResourceKind, ResourceStack, ValidatedAction};
use crate::protocol;
use crate::world::WorldState;

use super::MAX_MINE_AMOUNT;

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub actions: Vec<ValidatedAction>,
    pub rejected: Vec<String>,
    pub persistent_memory: Option<Vec<u8>>,
}

pub fn validate_game_output(
    world: &WorldState,
    player_id: u64,
    output_payload: &[u8],
    debug_max_actions: Option<u32>,
) -> Result<ValidationReport, Box<dyn std::error::Error>> {
    let output = fb::root_as_game_output(output_payload)?;
    let mut report = ValidationReport::default();

    if output.protocol_version() != fb::ProtocolVersion::V1 {
        report.rejected.push("unsupported protocol_version".into());
        return Ok(report);
    }

    let runtime_profile = world
        .player_runtime_profile(player_id)
        .ok_or("player has no runtime profile")?;
    let max_actions = debug_max_actions.unwrap_or(runtime_profile.max_actions);

    if let Some(memory) = output.persistent_memory() {
        if memory.len() > runtime_profile.max_persistent_memory_bytes as usize {
            report.rejected.push(format!(
                "persistent memory {} bytes exceeds max {}",
                memory.len(),
                runtime_profile.max_persistent_memory_bytes
            ));
        } else {
            report.persistent_memory = Some(memory.bytes().to_vec());
        }
    }

    let Some(actions) = output.actions() else {
        return Ok(report);
    };
    if actions.len() > max_actions as usize {
        report.rejected.push(format!(
            "action count {} exceeds max {}",
            actions.len(),
            max_actions
        ));
    }

    for index in 0..actions.len().min(max_actions as usize) {
        let action = actions.get(index);
        match validate_action(world, player_id, action) {
            Ok(action) => report.actions.push(action),
            Err(reason) => report.rejected.push(format!("action {index}: {reason}")),
        }
    }

    Ok(report)
}

pub fn validate_action(
    world: &WorldState,
    player_id: u64,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let actor = world
        .entities
        .get(&action.actor_entity_id())
        .ok_or("actor entity does not exist")?;
    if actor.owner_id != player_id {
        return Err("actor entity is not owned by player".into());
    }
    match action.kind() {
        fb::ActionKind::Move => validate_move(world, actor.position, action),
        fb::ActionKind::Mine => validate_mine(world, actor.position, action),
        fb::ActionKind::Build => validate_build(world, player_id, actor.position, action),
        fb::ActionKind::Lift => validate_lift(world, actor.position, action),
        fb::ActionKind::Put => validate_put(actor.cargo.as_slice(), action),
        other => Err(format!("unsupported action kind {other:?}")),
    }
}

fn validate_move(
    world: &WorldState,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let target = required_target_position(action, "Move")?;
    if actor_position.manhattan(target) != 1 {
        return Err("move target must be orthogonally adjacent".into());
    }
    if !world.is_passable(target) {
        return Err("move target is blocked".into());
    }

    Ok(ValidatedAction::Move {
        actor_entity_id: action.actor_entity_id(),
        target,
    })
}

fn validate_mine(
    world: &WorldState,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let target = required_target_position(action, "Mine")?;
    if actor_position.manhattan(target) != 1 {
        return Err("mine target must be orthogonally adjacent".into());
    }
    let tile = world.tile_at(target);
    let resource = tile.resource.ok_or("mine target has no resource")?;
    let requested = action.amount().clamp(1, MAX_MINE_AMOUNT);
    if requested > resource.amount {
        return Err("mine amount exceeds remaining resource".into());
    }

    Ok(ValidatedAction::Mine {
        actor_entity_id: action.actor_entity_id(),
        target,
        amount: requested,
    })
}

fn validate_build(
    world: &WorldState,
    player_id: u64,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let target = required_target_position(action, "Build")?;
    if actor_position.manhattan(target) != 1 {
        return Err("build target must be orthogonally adjacent".into());
    }
    if !world.is_passable(target) {
        return Err("build target is not empty and passable".into());
    }
    let near_owned_core = world.buildings.values().any(|building| {
        building.owner_id == player_id
            && building.kind == crate::model::BuildingKind::None
            && building.position.manhattan(target) <= 4
    });
    if !near_owned_core {
        return Err("build target must be near owned core".into());
    }
    let building_kind = protocol::to_model_building_kind(action.building_kind())
        .ok_or("build action has invalid building kind")?;
    if building_kind == crate::model::BuildingKind::None {
        return Err("building another core is not allowed in MVP".into());
    }

    Ok(ValidatedAction::Build {
        actor_entity_id: action.actor_entity_id(),
        target,
        building_kind,
    })
}

fn required_target_position(action: fb::Action<'_>, kind: &str) -> Result<Position, String> {
    action
        .target_position()
        .map(protocol::to_model_position)
        .ok_or_else(|| format!("{kind:?} action requires target_position"))
}

fn validate_lift(
    world: &WorldState,
    actor_position: Position,
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let tile = world.tile_at(actor_position);
    let Some(resource) = tile.resource else {
        return Err("lift target tile has no resource".into());
    };
    let fb_resource = action
        .resource()
        .ok_or("lift action requires resource field")?;
    let kind = to_model_resource_kind(fb_resource.kind())
        .ok_or("lift action has invalid resource kind")?;
    if resource.kind != kind {
        return Err("lift resource kind does not match tile resource kind".into());
    }
    let amount = action.amount().clamp(1, MAX_MINE_AMOUNT);
    if amount > resource.amount {
        return Err("lift amount exceeds remaining resource".into());
    }

    Ok(ValidatedAction::Lift {
        actor_entity_id: action.actor_entity_id(),
        kind,
        amount,
    })
}

fn validate_put(
    actor_cargo: &[ResourceStack],
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let fb_resource = action
        .resource()
        .ok_or("put action requires resource field")?;
    let kind =
        to_model_resource_kind(fb_resource.kind()).ok_or("put action has invalid resource kind")?;
    let available = actor_cargo
        .iter()
        .filter(|stack| stack.kind == kind)
        .map(|stack| stack.amount)
        .sum::<u32>();
    if available == 0 {
        return Err("put resource kind not in actor cargo".into());
    }
    let amount = action.amount().clamp(1, available);

    Ok(ValidatedAction::Put {
        actor_entity_id: action.actor_entity_id(),
        kind,
        amount,
    })
}

fn to_model_resource_kind(kind: fb::ResourceKind) -> Option<ResourceKind> {
    match kind {
        fb::ResourceKind::Iron => Some(ResourceKind::Iron),
        fb::ResourceKind::Copper => Some(ResourceKind::Copper),
        fb::ResourceKind::Energy => Some(ResourceKind::Energy),
        fb::ResourceKind::Stone => Some(ResourceKind::Stone),
        fb::ResourceKind::Tree => Some(ResourceKind::Tree),
        fb::ResourceKind::Water => Some(ResourceKind::Water),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_action_validation_ignores_actor_cooldown() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let actor_id = player.worker_entity_id;

        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let target = fb::Vec2I::new(1, 0);
        let action = fb::Action::create(
            &mut fbb,
            &fb::ActionArgs {
                kind: fb::ActionKind::Lift,
                actor_entity_id: actor_id,
                target_position: Some(&target),
                ..Default::default()
            },
        );
        fbb.finish_minimal(action);
        let action = flatbuffers::root::<fb::Action<'_>>(fbb.finished_data()).expect("action");

        assert_eq!(
            validate_action(&world, 1, action),
            Err("lift action requires resource field".into())
        );
    }
}
