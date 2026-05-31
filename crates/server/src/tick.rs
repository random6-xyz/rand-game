use std::path::Path;
use std::time::Duration;

use rand_game_common::fb;

use crate::action_log::ActionLogEntry;
use crate::protocol;
use crate::rules;
use crate::runner;
use crate::state::ServerConfig;
use crate::state::{BotStderrEvent, SharedState};
use crate::storage;

pub async fn run_tick_loop(state: SharedState) {
    let tick_interval_ms = state.inner().config.rules.tick_interval_ms.max(1);
    let mut interval = tokio::time::interval(Duration::from_millis(tick_interval_ms));

    loop {
        interval.tick().await;
        if let Err(err) = tick_once(state.clone()).await {
            eprintln!("tick failed: {err}");
        }
    }
}

pub async fn tick_once(state: SharedState) -> Result<(), Box<dyn std::error::Error>> {
    let run_request = {
        let mut world = state.inner().world.lock().await;
        world.advance_tick();

        let Some(player_id) = world.primary_player_id() else {
            return Ok(());
        };
        let should_run = should_run_player_bot(&world, player_id, &state.inner().config.rules);
        if !should_run {
            return Ok(());
        }

        let bot_path = world
            .player_bot_path(player_id)
            .filter(|path| path.exists());
        let Some(bot_path) = bot_path else {
            return Ok(());
        };
        let input_frame = protocol::build_game_input_frame(
            &world,
            player_id,
            &state.inner().config.rules,
            state.inner().config.debug_max_actions,
        )?;
        Some((player_id, bot_path, world.tick, input_frame))
    };

    let Some((player_id, bot_path, tick, input_frame)) = run_request else {
        return Ok(());
    };

    let bot_result = runner::run_bot(&bot_path, &input_frame)?;
    if !bot_result.stderr.trim().is_empty() {
        let event = BotStderrEvent {
            tick,
            player_id,
            bot_path: bot_path.display().to_string(),
            stderr: bot_result.stderr.clone(),
        };
        let _ = state.inner().bot_stderr.send(event);

        if state.inner().config.log_bot_stderr {
            log_bot_stderr(player_id, &bot_path, &bot_result.stderr);
        }
    }

    let entries = {
        let mut world = state.inner().world.lock().await;
        let tick = world.tick;
        let mut entries = Vec::new();

        if state.inner().config.debug_max_actions.is_some() {
            apply_debug_output_sequentially(
                &mut world,
                player_id,
                tick,
                &bot_result.output_payload,
                &state.inner().config,
                &mut entries,
            )?;
        } else {
            let validation = rules::validate_game_output(
                &world,
                player_id,
                &bot_result.output_payload,
                &state.inner().config.rules,
                Some(&state.inner().config.rule_catalog),
                state.inner().config.debug_max_actions,
            )?;
            for rejection in &validation.rejected {
                eprintln!("rejected action: {rejection}");
            }

            if let Some(memory) = validation.persistent_memory {
                world.set_player_persistent_memory(player_id, memory)?;
            }

            for action in validation.actions {
                let result = world.apply_action(player_id, &action);
                entries.push(ActionLogEntry::new(tick, player_id, action, result));
            }
        }
        storage::save_world(&world)?;

        entries
    };

    let mut action_log = state.inner().action_log.lock().await;
    let entries = compact_entries(entries);
    for entry in entries {
        eprintln!("tick {}: {}", entry.tick, entry.summary());
        action_log.push(entry);
    }
    storage::save_action_log(&action_log)?;

    Ok(())
}

fn apply_debug_output_sequentially(
    world: &mut crate::world::WorldState,
    player_id: u64,
    tick: u64,
    output_payload: &[u8],
    config: &ServerConfig,
    entries: &mut Vec<ActionLogEntry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = fb::root_as_game_output(output_payload)?;
    if output.protocol_version() != fb::ProtocolVersion::V1 {
        eprintln!("rejected action: unsupported protocol_version");
        return Ok(());
    }

    let runtime_profile = world
        .player_runtime_profile_with_rules(player_id, &config.rules)
        .ok_or("player has no runtime profile")?;
    if let Some(memory) = output.persistent_memory() {
        if memory.len() > runtime_profile.max_persistent_memory_bytes as usize {
            eprintln!(
                "rejected action: persistent memory {} bytes exceeds max {}",
                memory.len(),
                runtime_profile.max_persistent_memory_bytes
            );
        } else {
            world.set_player_persistent_memory(player_id, memory.bytes().to_vec())?;
        }
    }

    let Some(actions) = output.actions() else {
        return Ok(());
    };
    let max_actions = config
        .debug_max_actions
        .unwrap_or(runtime_profile.max_actions) as usize;
    if actions.len() > max_actions {
        eprintln!(
            "rejected action: action count {} exceeds max {}",
            actions.len(),
            max_actions
        );
    }

    for index in 0..actions.len().min(max_actions) {
        let action = actions.get(index);
        match rules::validate_action(
            world,
            player_id,
            action,
            &config.rules,
            Some(&config.rule_catalog),
        ) {
            Ok(action) => {
                let result = world.apply_action(player_id, &action);
                entries.push(ActionLogEntry::new(tick, player_id, action, result));
            }
            Err(reason) => eprintln!("rejected action: action {index}: {reason}"),
        }
    }

    Ok(())
}

fn compact_entries(entries: Vec<ActionLogEntry>) -> Vec<ActionLogEntry> {
    let mut compacted: Vec<ActionLogEntry> = Vec::new();
    for mut entry in entries {
        if entry.count == 0 {
            entry.count = 1;
        }
        if let Some(existing) = compacted
            .iter_mut()
            .find(|existing| existing.can_merge(&entry))
        {
            existing.count += entry.count;
        } else {
            compacted.push(entry);
        }
    }
    compacted
}

fn should_run_player_bot(
    world: &crate::world::WorldState,
    player_id: u64,
    rules: &crate::rules::ServerRules,
) -> bool {
    let Some(player) = world.players.get(&player_id) else {
        return false;
    };
    let interval = rules
        .runtime_profile(player.core_tier)
        .run_interval_ticks
        .max(1);
    let run_phase = player.core_entity_id % interval;
    world.tick % interval == run_phase
}

fn log_bot_stderr(player_id: u64, bot_path: &Path, stderr: &str) {
    for line in stderr.trim().lines() {
        eprintln!(
            "bot stderr [player_id={player_id} path={}]: {line}",
            bot_path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::action_log::ActionLogEntry;
    use crate::model::{Position, ValidatedAction};

    use super::compact_entries;

    #[test]
    fn compacts_repeated_actions_with_interleaved_targets() {
        let entries = vec![
            mine_entry(2, Position::new(0, -1)),
            mine_entry(3, Position::new(0, 0)),
            mine_entry(2, Position::new(0, -1)),
            mine_entry(3, Position::new(0, 0)),
        ];

        let compacted = compact_entries(entries);

        assert_eq!(compacted.len(), 2);
        assert_eq!(compacted[0].count, 2);
        assert_eq!(compacted[1].count, 2);
        assert_eq!(
            compacted[0].summary(),
            "mined 1 Energy at (0, -1) (2 times)"
        );
        assert_eq!(compacted[1].summary(), "mined 1 Energy at (0, 0) (2 times)");
    }

    fn mine_entry(actor_entity_id: u64, target: Position) -> ActionLogEntry {
        ActionLogEntry::new(
            7,
            1,
            ValidatedAction::Mine {
                actor_entity_id,
                target,
                amount: 1,
            },
            format!(
                "entity {actor_entity_id} mined 1 Energy at ({}, {})",
                target.x, target.y
            ),
        )
    }
}
