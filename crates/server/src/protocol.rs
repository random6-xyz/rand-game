use flatbuffers::FlatBufferBuilder;
use rand_game_common::fb::{self, *};
use rand_game_common::framing::{FrameKind, encode_frame};

use crate::model;
use crate::world::WorldState;

pub fn build_game_input_frame(
    world: &WorldState,
    player_id: u64,
    debug_max_actions: Option<u32>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let payload = build_game_input_payload(world, player_id, debug_max_actions)?;
    if !game_input_buffer_has_identifier(&payload) {
        return Err("generated payload is not a BWI1 GameInput flatbuffer".into());
    }
    root_as_game_input(&payload)?;
    Ok(encode_frame(FrameKind::GameInput, &payload)?)
}

pub fn build_game_input_payload(
    world: &WorldState,
    player_id: u64,
    debug_max_actions: Option<u32>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let owned_entities = world.owned_entities(player_id);
    let center = owned_entities
        .first()
        .map(|entity| entity.position)
        .ok_or("player has no owned entities")?;
    let visible_tiles = world.visible_tiles_for(player_id);
    let visible_monsters = world.visible_monsters_for(player_id);
    let environment_events = world.environment_events_for(player_id);
    let runtime_profile = world
        .player_runtime_profile(player_id)
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
                        hp: building.hp,
                        max_hp: building.max_hp,
                        power: building.power,
                    },
                )
            });
        let tile_offset = fb::Tile::create(
            &mut fbb,
            &fb::TileArgs {
                position: Some(&position),
                base_terrain: to_fb_terrain_kind(tile.base_terrain),
                terrain: to_fb_terrain_kind(tile.terrain),
                resource: resource.as_ref(),
                building,
                owner_id: tile.owner_id.unwrap_or_default(),
                danger_level: tile.danger_level,
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
            .map(|resource| {
                fb::ResourceStack::new(to_fb_resource_kind(resource.kind), resource.amount)
            })
            .collect::<Vec<_>>();
        let cargo = fbb.create_vector(&cargo_items);
        let entity_offset = fb::Entity::create(
            &mut fbb,
            &fb::EntityArgs {
                id: entity.id,
                kind: to_fb_entity_kind(entity.kind),
                position: Some(&position),
                hp: entity.hp,
                max_hp: entity.max_hp,
                energy: entity.energy,
                cargo: Some(cargo),
                cooldown_until_tick: entity.cooldown_until_tick,
            },
        );
        entity_offsets.push(entity_offset);
    }
    let owned_entities = fbb.create_vector(&entity_offsets);

    let mut monster_offsets = Vec::with_capacity(visible_monsters.len());
    for monster in &visible_monsters {
        let position = Vec2I::new(monster.position.x, monster.position.y);
        monster_offsets.push(fb::Monster::create(
            &mut fbb,
            &fb::MonsterArgs {
                id: monster.id,
                kind: to_fb_monster_kind(monster.kind),
                position: Some(&position),
                hp: monster.hp,
                max_hp: monster.max_hp,
                target_entity_id: monster.target_entity_id,
            },
        ));
    }
    let visible_monsters = fbb.create_vector(&monster_offsets);

    let mut event_offsets = Vec::with_capacity(environment_events.len());
    for event in &environment_events {
        let center = Vec2I::new(event.center.x, event.center.y);
        event_offsets.push(fb::EnvironmentEvent::create(
            &mut fbb,
            &fb::EnvironmentEventArgs {
                id: event.id,
                kind: to_fb_environment_event_kind(event.kind),
                center: Some(&center),
                radius: event.radius,
                starts_at_tick: event.starts_at_tick,
                ends_at_tick: event.ends_at_tick,
                intensity: event.intensity,
            },
        ));
    }
    let environment_events = fbb.create_vector(&event_offsets);

    let world_info = WorldInfo::create(
        &mut fbb,
        &WorldInfoArgs {
            tick: world.tick,
            world_seed: world.world_seed,
            map_id: world.map_id,
            map_kind: to_fb_map_kind(world.map_kind()),
            ruleset_version: 1,
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
            visible_monsters: Some(visible_monsters),
            environment_events: Some(environment_events),
            incoming_signals: None,
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
            max_signal_bytes: runtime_profile.max_signal_bytes,
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

pub fn to_model_resource_kind(kind: fb::ResourceKind) -> Option<model::ResourceKind> {
    match kind {
        fb::ResourceKind::Iron => Some(model::ResourceKind::Iron),
        fb::ResourceKind::Copper => Some(model::ResourceKind::Copper),
        fb::ResourceKind::Energy => Some(model::ResourceKind::Energy),
        _ => None,
    }
}

pub fn to_model_building_kind(kind: fb::BuildingKind) -> Option<model::BuildingKind> {
    match kind {
        fb::BuildingKind::Core => Some(model::BuildingKind::Core),
        fb::BuildingKind::Miner => Some(model::BuildingKind::Miner),
        fb::BuildingKind::Storage => Some(model::BuildingKind::Storage),
        fb::BuildingKind::Solar => Some(model::BuildingKind::Solar),
        fb::BuildingKind::Relay => Some(model::BuildingKind::Relay),
        fb::BuildingKind::Wall => Some(model::BuildingKind::Wall),
        fb::BuildingKind::Turret => Some(model::BuildingKind::Turret),
        _ => None,
    }
}

pub fn to_fb_resource_kind(kind: model::ResourceKind) -> fb::ResourceKind {
    match kind {
        model::ResourceKind::Iron => fb::ResourceKind::Iron,
        model::ResourceKind::Copper => fb::ResourceKind::Copper,
        model::ResourceKind::Energy => fb::ResourceKind::Energy,
    }
}

fn to_fb_terrain_kind(kind: model::TerrainKind) -> fb::TerrainKind {
    match kind {
        model::TerrainKind::Plain => fb::TerrainKind::Plain,
        model::TerrainKind::Rock => fb::TerrainKind::Rock,
        model::TerrainKind::Water => fb::TerrainKind::Water,
        model::TerrainKind::Mountain => fb::TerrainKind::Mountain,
        model::TerrainKind::Ruin => fb::TerrainKind::Ruin,
    }
}

fn to_fb_map_kind(kind: model::MapKind) -> fb::MapKind {
    match kind {
        model::MapKind::Resource => fb::MapKind::Resource,
        model::MapKind::Hazard => fb::MapKind::Hazard,
        model::MapKind::Monster => fb::MapKind::Monster,
        model::MapKind::Event => fb::MapKind::Event,
        model::MapKind::War => fb::MapKind::War,
    }
}

fn to_fb_monster_kind(kind: model::MonsterKind) -> fb::MonsterKind {
    match kind {
        model::MonsterKind::Drone => fb::MonsterKind::Drone,
        model::MonsterKind::Swarm => fb::MonsterKind::Swarm,
        model::MonsterKind::Guardian => fb::MonsterKind::Guardian,
    }
}

fn to_fb_environment_event_kind(kind: model::EnvironmentEventKind) -> fb::EnvironmentEventKind {
    match kind {
        model::EnvironmentEventKind::Storm => fb::EnvironmentEventKind::Storm,
        model::EnvironmentEventKind::Radiation => fb::EnvironmentEventKind::Radiation,
        model::EnvironmentEventKind::Meteor => fb::EnvironmentEventKind::Meteor,
        model::EnvironmentEventKind::ResourceSurge => fb::EnvironmentEventKind::ResourceSurge,
    }
}

fn to_fb_building_kind(kind: model::BuildingKind) -> fb::BuildingKind {
    match kind {
        model::BuildingKind::Core => fb::BuildingKind::Core,
        model::BuildingKind::Miner => fb::BuildingKind::Miner,
        model::BuildingKind::Storage => fb::BuildingKind::Storage,
        model::BuildingKind::Solar => fb::BuildingKind::Solar,
        model::BuildingKind::Relay => fb::BuildingKind::Relay,
        model::BuildingKind::Wall => fb::BuildingKind::Wall,
        model::BuildingKind::Turret => fb::BuildingKind::Turret,
    }
}

fn to_fb_entity_kind(kind: model::EntityKind) -> fb::EntityKind {
    match kind {
        model::EntityKind::Core => fb::EntityKind::Core,
        model::EntityKind::Worker => fb::EntityKind::Worker,
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
        let frame = build_game_input_frame(&world, 1, None).expect("build input frame");
        let payload = decode_frame(&frame, FrameKind::GameInput).expect("decode frame");

        assert!(game_input_buffer_has_identifier(payload));
        root_as_game_input(payload).expect("valid game input");
    }

    #[test]
    fn debug_max_actions_overrides_runtime_profile() {
        let world = WorldState::new();
        let payload = build_game_input_payload(&world, 1, Some(1000)).expect("build input payload");
        let input = root_as_game_input(&payload).expect("valid game input");
        let limits = input.runtime_limits().expect("runtime limits");
        let action_limits = limits.action_limits().expect("action limits");

        assert_eq!(action_limits.max_actions(), 1000);
    }
}
