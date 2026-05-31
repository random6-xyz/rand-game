use std::env;
use std::fs;
use std::path::PathBuf;

use flatbuffers::FlatBufferBuilder;
use rand_game_common::fb::*;
use rand_game_common::framing::{FrameKind, decode_frame, encode_frame};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = output_dir()?;
    fs::create_dir_all(&out_dir)?;

    let input_path = out_dir.join("game_input_example.bwi");
    let input = build_game_input();
    assert!(game_input_buffer_has_identifier(&input));
    root_as_game_input(&input)?;
    let framed_input = encode_frame(FrameKind::GameInput, &input)?;
    root_as_game_input(decode_frame(&framed_input, FrameKind::GameInput)?)?;
    fs::write(&input_path, &framed_input)?;

    let output_path = out_dir.join("game_output_example.bwo");
    let output = build_game_output();
    assert!(game_output_buffer_has_identifier(&output));
    root_as_game_output(&output)?;
    let framed_output = encode_frame(FrameKind::GameOutput, &output)?;
    root_as_game_output(decode_frame(&framed_output, FrameKind::GameOutput)?)?;
    fs::write(&output_path, &framed_output)?;

    println!(
        "wrote {} ({} bytes)",
        input_path.display(),
        framed_input.len()
    );
    println!(
        "wrote {} ({} bytes)",
        output_path.display(),
        framed_output.len()
    );

    Ok(())
}

fn output_dir() -> Result<PathBuf, String> {
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (None, None) => Ok(PathBuf::from("target/flatbuffers_examples")),
        (Some("--out-dir"), Some(path)) => Ok(PathBuf::from(path)),
        _ => Err(
            "usage: cargo run -p rand-game-common --example gen_example -- [--out-dir PATH]".into(),
        ),
    }
}

fn build_game_input() -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();

    let core_building = Building::create(
        &mut fbb,
        &BuildingArgs {
            id: 100,
            kind: BuildingKind::Storage,
            owner_id: 1,
            power: 25,
        },
    );

    let tile_origin_pos = Vec2I::new(0, 0);
    let tile_origin_resource = ResourceStack::new(ResourceKind::Iron, 320);
    let tile_origin = Tile::create(
        &mut fbb,
        &TileArgs {
            position: Some(&tile_origin_pos),
            resource: Some(&tile_origin_resource),
            building: Some(core_building),
            owner_id: 1,
        },
    );

    let tile_rock_pos = Vec2I::new(1, 0);
    let tile_rock_resource = ResourceStack::new(ResourceKind::Copper, 80);
    let tile_rock = Tile::create(
        &mut fbb,
        &TileArgs {
            position: Some(&tile_rock_pos),
            resource: Some(&tile_rock_resource),
            building: None,
            owner_id: 0,
        },
    );

    let visible_tiles = fbb.create_vector(&[tile_origin, tile_rock]);

    let worker_pos = Vec2I::new(0, 1);
    let iron_ore = fbb.create_string("iron-ore");
    let worker_cargo_items = [ItemStack::create(
        &mut fbb,
        &ItemStackArgs {
            kind: Some(iron_ore),
            amount: 12,
        },
    )];
    let worker_cargo = fbb.create_vector(&worker_cargo_items);
    let worker = Entity::create(
        &mut fbb,
        &EntityArgs {
            id: 200,
            position: Some(&worker_pos),
            cargo: Some(worker_cargo),
        },
    );
    let owned_entities = fbb.create_vector(&[worker]);

    let persistent_memory = fbb.create_vector(b"example-memory-v1");

    let world = WorldInfo::create(
        &mut fbb,
        &WorldInfoArgs {
            tick: 123,
            map_id: 7,
            map_kind: MapKind::Resource,
            ruleset_version: 1,
        },
    );

    let center = Vec2I::new(0, 0);
    let observation = Observation::create(
        &mut fbb,
        &ObservationArgs {
            center: Some(&center),
            radius: 4,
            visible_tiles: Some(visible_tiles),
            owned_entities: Some(owned_entities),
        },
    );

    let compute_budget = ComputeBudget::create(
        &mut fbb,
        &ComputeBudgetArgs {
            cpu_time_ms: 50,
            wall_time_ms: 100,
            memory_bytes: 64 * 1024 * 1024,
            stdout_bytes: 4096,
            stderr_bytes: 4096,
        },
    );
    let action_limits = ActionLimits::create(
        &mut fbb,
        &ActionLimitsArgs {
            max_actions: 8,
            max_persistent_memory_bytes: 4096,
        },
    );
    let runtime_limits = RuntimeLimits::create(
        &mut fbb,
        &RuntimeLimitsArgs {
            compute_budget: Some(compute_budget),
            action_limits: Some(action_limits),
        },
    );

    let game_input = GameInput::create(
        &mut fbb,
        &GameInputArgs {
            protocol_version: ProtocolVersion::V1,
            world: Some(world),
            observation: Some(observation),
            persistent_memory: Some(persistent_memory),
            runtime_limits: Some(runtime_limits),
        },
    );

    finish_game_input_buffer(&mut fbb, game_input);
    fbb.finished_data().to_vec()
}

fn build_game_output() -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();

    let move_target = Vec2I::new(1, 0);
    let move_action = Action::create(
        &mut fbb,
        &ActionArgs {
            kind: ActionKind::Move,
            actor_entity_id: 200,
            target_position: Some(&move_target),
            ..Default::default()
        },
    );

    let mine_resource = ResourceStack::new(ResourceKind::Copper, 25);
    let mine_action = Action::create(
        &mut fbb,
        &ActionArgs {
            kind: ActionKind::Mine,
            actor_entity_id: 200,
            target_position: Some(&move_target),
            resource: Some(&mine_resource),
            amount: 25,
            ..Default::default()
        },
    );

    let actions = fbb.create_vector(&[move_action, mine_action]);
    let persistent_memory = fbb.create_vector(b"next-memory-state");

    let game_output = GameOutput::create(
        &mut fbb,
        &GameOutputArgs {
            protocol_version: ProtocolVersion::V1,
            actions: Some(actions),
            persistent_memory: Some(persistent_memory),
        },
    );

    finish_game_output_buffer(&mut fbb, game_output);
    fbb.finished_data().to_vec()
}
