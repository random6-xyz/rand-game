use flatbuffers::FlatBufferBuilder;
use rand_game_common::fb::*;

use super::model::{ActionPlan, PlannedAction};

pub(crate) fn build_output_with_actions(planned_actions: Vec<PlannedAction>) -> Vec<u8> {
    build_output_with_actions_and_memory(planned_actions, &[])
}

pub(crate) fn build_output_with_actions_and_memory(
    planned_actions: Vec<PlannedAction>,
    persistent_memory: &[u8],
) -> Vec<u8> {
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
            ActionPlan::Build {
                target,
                building_kind,
            } => {
                let target_position = Vec2I::new(target.x, target.y);
                Action::create(
                    &mut fbb,
                    &ActionArgs {
                        kind: ActionKind::Build,
                        actor_entity_id: planned_action.actor_id,
                        target_position: Some(&target_position),
                        building_kind,
                        ..Default::default()
                    },
                )
            }
            ActionPlan::Lift { resource, amount } => {
                let resource = ResourceStack::new(resource, amount);
                Action::create(
                    &mut fbb,
                    &ActionArgs {
                        kind: ActionKind::Lift,
                        actor_entity_id: planned_action.actor_id,
                        resource: Some(&resource),
                        amount,
                        ..Default::default()
                    },
                )
            }
            ActionPlan::Put { resource, amount } => {
                let resource = ResourceStack::new(resource, amount);
                Action::create(
                    &mut fbb,
                    &ActionArgs {
                        kind: ActionKind::Put,
                        actor_entity_id: planned_action.actor_id,
                        resource: Some(&resource),
                        amount,
                        ..Default::default()
                    },
                )
            }
            ActionPlan::Craft {
                recipe_id,
                target_building_id,
            } => {
                let recipe_id = fbb.create_string(&recipe_id);
                Action::create(
                    &mut fbb,
                    &ActionArgs {
                        kind: ActionKind::Craft,
                        actor_entity_id: planned_action.actor_id,
                        target_building_id,
                        recipe_id: Some(recipe_id),
                        ..Default::default()
                    },
                )
            }
            ActionPlan::Research { research_id } => {
                let research_id = fbb.create_string(&research_id);
                Action::create(
                    &mut fbb,
                    &ActionArgs {
                        kind: ActionKind::Research,
                        actor_entity_id: planned_action.actor_id,
                        recipe_id: Some(research_id),
                        ..Default::default()
                    },
                )
            }
        };
        action_offsets.push(action);
    }

    finish_output(&mut fbb, &action_offsets, persistent_memory)
}

pub(crate) fn build_output_without_actions() -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let action_offsets: [flatbuffers::WIPOffset<Action<'_>>; 0] = [];
    finish_output(&mut fbb, &action_offsets, &[])
}

fn finish_output<'fbb>(
    fbb: &mut FlatBufferBuilder<'fbb>,
    action_offsets: &[flatbuffers::WIPOffset<Action<'fbb>>],
    persistent_memory: &[u8],
) -> Vec<u8> {
    let actions = fbb.create_vector(action_offsets);
    let persistent_memory = fbb.create_vector(persistent_memory);
    let output = GameOutput::create(
        fbb,
        &GameOutputArgs {
            protocol_version: ProtocolVersion::V1,
            actions: Some(actions),
            persistent_memory: Some(persistent_memory),
        },
    );

    finish_game_output_buffer(fbb, output);
    fbb.finished_data().to_vec()
}
