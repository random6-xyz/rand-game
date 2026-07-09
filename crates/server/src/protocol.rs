use flatbuffers::FlatBufferBuilder;
use rand_game_common::fb::{self, *};
use rand_game_common::framing::{FrameKind, encode_frame};

use crate::model;
use crate::rules::ServerRules;
use crate::world::WorldState;

pub fn build_game_input_frame(
    world: &WorldState,
    player_id: u64,
    rules: &ServerRules,
    debug_max_actions: Option<u32>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let payload = build_game_input_payload(world, player_id, rules, debug_max_actions)?;
    if !game_input_buffer_has_identifier(&payload) {
        return Err("generated payload is not a BWI1 GameInput flatbuffer".into());
    }
    root_as_game_input(&payload)?;
    Ok(encode_frame(FrameKind::GameInput, &payload)?)
}

pub fn build_game_input_payload(
    world: &WorldState,
    player_id: u64,
    rules: &ServerRules,
    debug_max_actions: Option<u32>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let owned_entities = world.owned_entities(player_id);
    let center = owned_entities
        .first()
        .map(|entity| entity.position)
        .ok_or("player has no owned entities")?;
    let visible_tiles = world.visible_tiles_for(player_id);
    let runtime_profile = world
        .player_runtime_profile_with_rules(player_id, rules)
        .ok_or("player has no runtime profile")?;

    let mut fbb = FlatBufferBuilder::new();

    let mut tile_offsets = Vec::with_capacity(visible_tiles.len());
    for tile in &visible_tiles {
        let position = Vec2I::new(tile.position.x, tile.position.y);
        let resource = tile.resource.map(|resource| {
            fb::ResourceStack::new(to_fb_resource_kind(resource.kind), resource.amount)
        });
        let building = tile
            .building_id
            .and_then(|building_id| world.buildings.get(&building_id))
            .map(|building| {
                fb::Building::create(
                    &mut fbb,
                    &fb::BuildingArgs {
                        id: building.id,
                        kind: to_fb_building_kind(building.kind),
                        owner_id: building.owner_id,
                        power: building.power,
                    },
                )
            });
        let tile_offset = fb::Tile::create(
            &mut fbb,
            &fb::TileArgs {
                position: Some(&position),
                resource: resource.as_ref(),
                building,
                owner_id: tile.owner_id.unwrap_or_default(),
            },
        );
        tile_offsets.push(tile_offset);
    }
    let visible_tiles = fbb.create_vector(&tile_offsets);

    let mut entity_offsets = Vec::with_capacity(owned_entities.len());
    for entity in owned_entities {
        let position = Vec2I::new(entity.position.x, entity.position.y);
        let cargo_items = entity
            .cargo
            .iter()
            .map(|item| {
                let kind = fbb.create_string(&item.kind);
                fb::ItemStack::create(
                    &mut fbb,
                    &fb::ItemStackArgs {
                        kind: Some(kind),
                        amount: item.amount,
                    },
                )
            })
            .collect::<Vec<_>>();
        let cargo = fbb.create_vector(&cargo_items);
        let entity_offset = fb::Entity::create(
            &mut fbb,
            &fb::EntityArgs {
                id: entity.id,
                position: Some(&position),
                cargo: Some(cargo),
            },
        );
        entity_offsets.push(entity_offset);
    }
    let owned_entities = fbb.create_vector(&entity_offsets);

    let world_info = WorldInfo::create(
        &mut fbb,
        &WorldInfoArgs {
            tick: world.tick,
            map_id: world.map_id,
            map_kind: to_fb_map_kind(world.map_kind()),
            ruleset_version: rules.ruleset_version,
        },
    );

    let center = Vec2I::new(center.x, center.y);
    let observation = Observation::create(
        &mut fbb,
        &ObservationArgs {
            center: Some(&center),
            radius: world.observation_radius,
            visible_tiles: Some(visible_tiles),
            owned_entities: Some(owned_entities),
        },
    );

    let compute_budget = ComputeBudget::create(
        &mut fbb,
        &ComputeBudgetArgs {
            cpu_time_ms: runtime_profile.cpu_time_ms,
            wall_time_ms: runtime_profile.wall_time_ms,
            memory_bytes: runtime_profile.memory_bytes,
            stdout_bytes: runtime_profile.stdout_bytes,
            stderr_bytes: runtime_profile.stderr_bytes,
        },
    );
    let action_limits = ActionLimits::create(
        &mut fbb,
        &ActionLimitsArgs {
            max_actions: debug_max_actions.unwrap_or(runtime_profile.max_actions),
            max_persistent_memory_bytes: runtime_profile.max_persistent_memory_bytes,
        },
    );
    let runtime_limits = RuntimeLimits::create(
        &mut fbb,
        &RuntimeLimitsArgs {
            compute_budget: Some(compute_budget),
            action_limits: Some(action_limits),
        },
    );
    let persistent_memory = fbb.create_vector(world.player_persistent_memory(player_id));

    let game_input = GameInput::create(
        &mut fbb,
        &GameInputArgs {
            protocol_version: ProtocolVersion::V1,
            world: Some(world_info),
            observation: Some(observation),
            persistent_memory: Some(persistent_memory),
            runtime_limits: Some(runtime_limits),
        },
    );

    finish_game_input_buffer(&mut fbb, game_input);
    Ok(fbb.finished_data().to_vec())
}

pub fn to_model_position(position: &Vec2I) -> model::Position {
    model::Position::new(position.x(), position.y())
}

pub fn to_model_building_kind(kind: fb::BuildingKind) -> Option<model::BuildingKind> {
    match kind {
        fb::BuildingKind::Miner => Some(model::BuildingKind::Miner),
        fb::BuildingKind::Storage => Some(model::BuildingKind::Storage),
        fb::BuildingKind::Solar => Some(model::BuildingKind::Solar),
        fb::BuildingKind::Assembler => Some(model::BuildingKind::Assembler),
        fb::BuildingKind::Furnace => Some(model::BuildingKind::Furnace),
        _ => None,
    }
}

pub fn to_fb_resource_kind(kind: model::ResourceKind) -> fb::ResourceKind {
    match kind {
        model::ResourceKind::Iron => fb::ResourceKind::Iron,
        model::ResourceKind::Copper => fb::ResourceKind::Copper,
        model::ResourceKind::Energy => fb::ResourceKind::Energy,
        model::ResourceKind::Stone => fb::ResourceKind::Stone,
        model::ResourceKind::Tree => fb::ResourceKind::Tree,
        model::ResourceKind::Water => fb::ResourceKind::Water,
    }
}

fn to_fb_map_kind(kind: model::MapKind) -> fb::MapKind {
    match kind {
        model::MapKind::Resource => fb::MapKind::Resource,
        model::MapKind::Hazard => fb::MapKind::Hazard,
        model::MapKind::Monster => fb::MapKind::Monster,
    }
}

fn to_fb_building_kind(kind: model::BuildingKind) -> fb::BuildingKind {
    match kind {
        model::BuildingKind::None => fb::BuildingKind::None,
        model::BuildingKind::Miner => fb::BuildingKind::Miner,
        model::BuildingKind::Storage => fb::BuildingKind::Storage,
        model::BuildingKind::Solar => fb::BuildingKind::Solar,
        model::BuildingKind::Assembler => fb::BuildingKind::Assembler,
        model::BuildingKind::Furnace => fb::BuildingKind::Furnace,
    }
}

#[cfg(test)]
mod tests {
    use rand_game_common::fb::{game_input_buffer_has_identifier, root_as_game_input};
    use rand_game_common::framing::{FrameKind, decode_frame};

    use super::*;

    #[test]
    fn builds_valid_framed_game_input() {
        let world = WorldState::new();
        let rules = ServerRules::default();
        let frame = build_game_input_frame(&world, 1, &rules, None).expect("build input frame");
        let payload = decode_frame(&frame, FrameKind::GameInput).expect("decode frame");

        assert!(game_input_buffer_has_identifier(payload));
        root_as_game_input(payload).expect("valid game input");
    }

    #[test]
    fn debug_max_actions_overrides_runtime_profile() {
        let world = WorldState::new();
        let rules = ServerRules::default();
        let payload =
            build_game_input_payload(&world, 1, &rules, Some(1000)).expect("build input payload");
        let input = root_as_game_input(&payload).expect("valid game input");
        let limits = input.runtime_limits().expect("runtime limits");
        let action_limits = limits.action_limits().expect("action limits");

        assert_eq!(action_limits.max_actions(), 1000);
    }

    #[test]
    fn game_input_uses_configured_ruleset_version() {
        let world = WorldState::new();
        let rules = ServerRules {
            ruleset_version: 7,
            ..ServerRules::default()
        };
        let payload =
            build_game_input_payload(&world, 1, &rules, None).expect("build input payload");
        let input = root_as_game_input(&payload).expect("valid game input");

        assert_eq!(input.world().expect("world").ruleset_version(), 7);
    }
}
