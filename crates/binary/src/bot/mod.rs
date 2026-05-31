mod behavior;
mod model;
mod observation;
mod output;
mod pathfinding;
mod planner;
mod verify;

#[cfg(test)]
mod rules_tests;

use std::io::{self, Write};

use rand_game_common::fb::*;
use rand_game_common::framing::{FrameKind, read_frame, write_frame};

use observation::{ready_actors, visible_passable_positions, visible_resource_tiles};
use output::{
    build_output_with_actions, build_output_with_actions_and_memory, build_output_without_actions,
};
use planner::{plan_debug_simulation_actions, plan_single_tick_actions};

const DEBUG_SIMULATION_MIN_ACTIONS: usize = 100;

pub fn run_sample_bot() -> Result<(), Box<dyn std::error::Error>> {
    let input_payload = read_frame(io::stdin().lock(), FrameKind::GameInput)?;

    if !game_input_buffer_has_identifier(&input_payload) {
        return Err("stdin frame payload is not a BWI1 GameInput flatbuffer".into());
    }

    let game_input = root_as_game_input(&input_payload)?;
    let output_payload = build_game_output(game_input)?;

    if !game_output_buffer_has_identifier(&output_payload) {
        return Err("generated payload is not a BWO1 GameOutput flatbuffer".into());
    }
    root_as_game_output(&output_payload)?;

    write_frame(io::stdout().lock(), FrameKind::GameOutput, &output_payload)?;
    io::stdout().lock().flush()?;

    Ok(())
}

fn is_verify_mode() -> bool {
    std::env::var("RAND_GAME_VERIFY_RECIPES")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn build_game_output(input: GameInput<'_>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let observation = match input.observation() {
        Some(observation) => observation,
        None => return empty_output("missing observation"),
    };

    let max_actions = input
        .runtime_limits()
        .and_then(|limits| limits.action_limits())
        .map(|limits| limits.max_actions() as usize)
        .unwrap_or(1);
    if max_actions == 0 {
        return empty_output("max_actions is zero");
    }

    let persistent_memory = input
        .persistent_memory()
        .map(|pm| {
            let mut bytes = Vec::with_capacity(pm.len());
            for i in 0..pm.len() {
                bytes.push(pm.get(i));
            }
            bytes
        })
        .unwrap_or_default();

    if is_verify_mode() {
        let (actions, new_memory) =
            verify::plan_verify_actions(observation, &persistent_memory, max_actions);
        if actions.is_empty() {
            eprintln!("sample_bot: verify: no action this tick; preserving verify state");
        }
        return Ok(build_output_with_actions_and_memory(actions, &new_memory));
    }

    let actors = ready_actors(observation);
    if actors.is_empty() {
        return empty_output("no ready owned worker or core entity");
    };

    let resources = visible_resource_tiles(observation);
    let passable_positions = visible_passable_positions(observation);
    let actions = if max_actions >= DEBUG_SIMULATION_MIN_ACTIONS {
        plan_debug_simulation_actions(actors, resources, passable_positions, max_actions)
    } else {
        plan_single_tick_actions(&actors, &resources, &passable_positions, max_actions)
    };

    if actions.is_empty() {
        return empty_output("no profitable action found");
    }

    Ok(build_output_with_actions(actions))
}

fn empty_output(reason: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    eprintln!("sample_bot: {reason}; emitting empty action list");
    Ok(build_output_without_actions())
}
