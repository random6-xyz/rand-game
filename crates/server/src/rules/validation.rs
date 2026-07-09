use rand_game_common::fb;
use rand_game_common::rules::{BuildingSpec, ItemStackSpec, RecipeSpec, ResearchSpec, RuleCatalog};

use crate::model::{ItemStack, Position, ResourceKind, ValidatedAction};
use crate::protocol;
use crate::world::WorldState;

use super::ServerRules;

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
    rules: &ServerRules,
    catalog: Option<&RuleCatalog>,
    debug_max_actions: Option<u32>,
) -> Result<ValidationReport, Box<dyn std::error::Error>> {
    let output = fb::root_as_game_output(output_payload)?;
    let mut report = ValidationReport::default();

    if output.protocol_version() != fb::ProtocolVersion::V1 {
        report.rejected.push("unsupported protocol_version".into());
        return Ok(report);
    }

    let runtime_profile = world
        .player_runtime_profile_with_rules(player_id, rules)
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
        match validate_action(world, player_id, action, rules, catalog) {
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
    rules: &ServerRules,
    catalog: Option<&RuleCatalog>,
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
        fb::ActionKind::Mine => validate_mine(world, actor.position, action, rules),
        fb::ActionKind::Build => {
            validate_build(world, player_id, actor.position, action, rules, catalog)
        }
        fb::ActionKind::Lift => validate_lift(world, actor.position, action, rules),
        fb::ActionKind::Put => validate_put(actor.cargo.as_slice(), action),
        fb::ActionKind::Craft => validate_craft(world, player_id, action, catalog),
        fb::ActionKind::Research => validate_research(world, player_id, action, catalog),
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
    rules: &ServerRules,
) -> Result<ValidatedAction, String> {
    let target = required_target_position(action, "Mine")?;
    if actor_position.manhattan(target) != 1 {
        return Err("mine target must be orthogonally adjacent".into());
    }
    let tile = world.tile_at(target);
    let resource = tile.resource.ok_or("mine target has no resource")?;
    let requested = action.amount().clamp(1, rules.max_mine_amount.max(1));
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
    rules: &ServerRules,
    catalog: Option<&RuleCatalog>,
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
            && building.spec_id == "entity"
            && building.position.manhattan(target) <= rules.build_core_radius
    });
    if !near_owned_core {
        return Err("build target must be near owned core".into());
    }
    let spec_id = protocol::to_model_building_spec_id(&action)
        .ok_or("build action requires building_spec_id")?;
    if spec_id == "entity" {
        return Err("building another core is not allowed in MVP".into());
    }
    let spec = validate_building_spec_exists(&spec_id, catalog)?;
    let actor = world
        .entities
        .get(&action.actor_entity_id())
        .ok_or("actor entity does not exist")?;
    validate_build_costs(actor.cargo.as_slice(), spec)?;

    Ok(ValidatedAction::Build {
        actor_entity_id: action.actor_entity_id(),
        target,
        building_spec_id: spec_id,
        inputs: spec
            .inputs
            .iter()
            .map(|stack| ItemStack {
                kind: stack.kind.clone(),
                amount: stack.amount,
            })
            .collect(),
    })
}

fn validate_building_spec_exists<'a>(
    spec_id: &str,
    catalog: Option<&'a RuleCatalog>,
) -> Result<&'a BuildingSpec, String> {
    let catalog = catalog.ok_or("build action requires rule catalog")?;
    let spec = catalog
        .buildings
        .buildings
        .iter()
        .find(|building| building.id == spec_id)
        .ok_or_else(|| format!("unknown building spec `{spec_id}`"))?;
    if spec.width != 1 {
        return Err(format!(
            "building spec `{spec_id}` has width {}, but multi-tile buildings are not supported yet",
            spec.width
        ));
    }
    Ok(spec)
}

fn validate_build_costs(actor_cargo: &[ItemStack], spec: &BuildingSpec) -> Result<(), String> {
    for input in &spec.inputs {
        let available = actor_cargo
            .iter()
            .filter(|stack| stack.kind == input.kind)
            .map(|stack| stack.amount)
            .sum::<u32>();
        if available < input.amount {
            return Err(format!(
                "building `{}` requires {} {} in actor cargo, but only {} available",
                spec.id, input.amount, input.kind, available
            ));
        }
    }
    Ok(())
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
    rules: &ServerRules,
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
    let amount = action.amount().clamp(1, rules.max_mine_amount.max(1));
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
    actor_cargo: &[ItemStack],
    action: fb::Action<'_>,
) -> Result<ValidatedAction, String> {
    let fb_resource = action
        .resource()
        .ok_or("put action requires resource field")?;
    let kind =
        to_model_resource_kind(fb_resource.kind()).ok_or("put action has invalid resource kind")?;
    let available = actor_cargo
        .iter()
        .filter(|stack| stack.kind == kind.item_id())
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

fn validate_craft(
    world: &WorldState,
    player_id: u64,
    action: fb::Action<'_>,
    catalog: Option<&RuleCatalog>,
) -> Result<ValidatedAction, String> {
    let catalog = catalog.ok_or("craft action requires rule catalog")?;
    let recipe_id = action
        .recipe_id()
        .filter(|recipe_id| !recipe_id.trim().is_empty())
        .ok_or("craft action requires recipe_id")?;
    let recipe = catalog
        .recipes
        .recipes
        .iter()
        .find(|recipe| recipe.id == recipe_id)
        .ok_or_else(|| format!("unknown recipe `{recipe_id}`"))?;
    let actor = world
        .entities
        .get(&action.actor_entity_id())
        .ok_or("actor entity does not exist")?;
    if actor.owner_id != player_id {
        return Err("actor entity is not owned by player".into());
    }
    let target_building_id = validate_recipe_workplace(world, player_id, action, recipe)?;
    validate_recipe_inputs(actor.cargo.as_slice(), recipe)?;
    validate_recipe_outputs(actor.cargo.as_slice(), recipe)?;
    validate_recipe_researched(world, player_id, recipe, catalog)?;

    Ok(ValidatedAction::Craft {
        actor_entity_id: action.actor_entity_id(),
        recipe_id: recipe.id.clone(),
        target_building_id,
        inputs: recipe
            .inputs
            .iter()
            .map(|stack| ItemStack {
                kind: stack.kind.clone(),
                amount: stack.amount,
            })
            .collect(),
        outputs: recipe
            .outputs
            .iter()
            .map(|stack| ItemStack {
                kind: stack.kind.clone(),
                amount: stack.amount,
            })
            .collect(),
    })
}

fn validate_recipe_workplace(
    world: &WorldState,
    player_id: u64,
    action: fb::Action<'_>,
    recipe: &RecipeSpec,
) -> Result<Option<u64>, String> {
    if recipe.building.iter().any(|building| building == "entity")
        && action.target_building_id() == 0
    {
        return Ok(None);
    }

    let target_building_id = action.target_building_id();
    if target_building_id == 0 {
        return Err("craft action requires target_building_id for building recipe".into());
    }
    let building = world
        .buildings
        .get(&target_building_id)
        .ok_or("craft target building does not exist")?;
    if building.owner_id != player_id {
        return Err("craft target building is not owned by player".into());
    }
    if building.spec_id.is_empty() {
        return Err("craft target building has no spec_id".into());
    }
    if !recipe.building.iter().any(|b| b == &building.spec_id) {
        return Err(format!(
            "recipe `{}` cannot be crafted by building `{}`",
            recipe.id, building.spec_id
        ));
    }

    Ok(Some(target_building_id))
}

fn validate_recipe_researched(
    world: &WorldState,
    player_id: u64,
    recipe: &RecipeSpec,
    catalog: &RuleCatalog,
) -> Result<(), String> {
    let player = world
        .players
        .get(&player_id)
        .ok_or("player does not exist")?;
    for research in &catalog.researches.researches {
        if research.unlocked_recipes.iter().any(|id| id == &recipe.id)
            && !player.researched_ids.contains(&research.id)
        {
            return Err(format!(
                "recipe `{}` requires researching `{}` first",
                recipe.id, research.id
            ));
        }
    }
    Ok(())
}

trait RecipeSpecRef {
    fn id(&self) -> &str;
    fn inputs(&self) -> &[ItemStackSpec];
}

impl RecipeSpecRef for RecipeSpec {
    fn id(&self) -> &str {
        &self.id
    }
    fn inputs(&self) -> &[ItemStackSpec] {
        &self.inputs
    }
}

impl RecipeSpecRef for ResearchSpec {
    fn id(&self) -> &str {
        &self.id
    }
    fn inputs(&self) -> &[ItemStackSpec] {
        &self.inputs
    }
}

fn validate_recipe_inputs(
    actor_cargo: &[ItemStack],
    spec: &dyn RecipeSpecRef,
) -> Result<(), String> {
    for input in spec.inputs() {
        let available = actor_cargo
            .iter()
            .filter(|stack| stack.kind == input.kind)
            .map(|stack| stack.amount)
            .sum::<u32>();
        if available < input.amount {
            return Err(format!(
                "`{}` requires {} {}, but actor cargo has {}",
                spec.id(),
                input.amount,
                input.kind,
                available
            ));
        }
    }
    Ok(())
}

fn validate_recipe_outputs(actor_cargo: &[ItemStack], recipe: &RecipeSpec) -> Result<(), String> {
    for output in &recipe.outputs {
        let current = actor_cargo
            .iter()
            .filter(|stack| stack.kind == output.kind)
            .map(|stack| stack.amount)
            .sum::<u32>();
        if current.saturating_add(output.amount) > recipe.max_stack {
            return Err(format!(
                "recipe `{}` output `{}` would exceed max_stack {}",
                recipe.id, output.kind, recipe.max_stack
            ));
        }
    }
    Ok(())
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

fn validate_research(
    world: &WorldState,
    player_id: u64,
    action: fb::Action<'_>,
    catalog: Option<&RuleCatalog>,
) -> Result<ValidatedAction, String> {
    let catalog = catalog.ok_or("research action requires rule catalog")?;
    let research_id = action
        .recipe_id()
        .filter(|id| !id.trim().is_empty())
        .ok_or("research action requires research_id")?;
    let research = catalog
        .researches
        .researches
        .iter()
        .find(|r| r.id == research_id)
        .ok_or_else(|| format!("unknown research `{research_id}`"))?;
    let actor = world
        .entities
        .get(&action.actor_entity_id())
        .ok_or("actor entity does not exist")?;
    if actor.owner_id != player_id {
        return Err("actor entity is not owned by player".into());
    }
    let player = world
        .players
        .get(&player_id)
        .ok_or("player does not exist")?;
    if player.researched_ids.contains(research_id) {
        return Err(format!("research `{research_id}` is already researched"));
    }
    validate_recipe_inputs(actor.cargo.as_slice(), research)?;

    Ok(ValidatedAction::Research {
        actor_entity_id: action.actor_entity_id(),
        research_id: research.id.clone(),
        inputs: research
            .inputs
            .iter()
            .map(|stack| ItemStack {
                kind: stack.kind.clone(),
                amount: stack.amount,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Building, BuildingKind, ItemStack};
    use rand_game_common::rules::default_rule_catalog;
    use rand_game_common::rules::{
        BuildingCatalog, BuildingSpec, RecipeCatalog, ResearchCatalog, RuleCatalog,
        validate_rule_catalog,
    };

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
            validate_action(&world, 1, action, &ServerRules::default(), None),
            Err("lift action requires resource field".into())
        );
    }

    #[test]
    fn build_validation_uses_yaml_catalog_when_enabled() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let action = build_action(player.worker_entity_id, "min-1");
        let catalog = catalog_with_building("min-1", 1);

        let validated = validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog))
            .expect("valid build");

        assert!(matches!(validated, ValidatedAction::Build { .. }));
    }

    #[test]
    fn build_validation_rejects_missing_yaml_building() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let action = build_action(player.worker_entity_id, "min-1");
        let catalog = catalog_with_building("asm-1", 1);

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err("unknown building spec `min-1`".into())
        );
    }

    #[test]
    fn build_validation_rejects_multi_tile_yaml_building() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let action = build_action(player.worker_entity_id, "min-1");
        let catalog = catalog_with_building("min-1", 2);

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err(
                "building spec `min-1` has width 2, but multi-tile buildings are not supported yet"
                    .into()
            )
        );
    }

    #[test]
    fn craft_validation_accepts_all_generated_recipes_with_inputs() {
        let mut world = WorldState::new();
        let catalog = default_rule_catalog();
        let player = world.players.get_mut(&1).expect("player 1");
        for research in &catalog.researches.researches {
            player.researched_ids.insert(research.id.clone());
        }
        let player = world.players.get(&1).expect("player 1");
        let actor_id = player.worker_entity_id;
        world.buildings.insert(
            99,
            Building {
                id: 99,
                kind: BuildingKind::Assembler,
                spec_id: "asm-1".into(),
                owner_id: 1,
                position: Position::new(2, 0),
                power: 0,
            },
        );
        world.buildings.insert(
            98,
            Building {
                id: 98,
                kind: BuildingKind::Furnace,
                spec_id: "fur-1".into(),
                owner_id: 1,
                position: Position::new(3, 0),
                power: 0,
            },
        );

        for recipe in &catalog.recipes.recipes {
            let actor = world.entities.get_mut(&actor_id).expect("actor");
            actor.cargo = recipe
                .inputs
                .iter()
                .map(|stack| ItemStack {
                    kind: stack.kind.clone(),
                    amount: stack.amount,
                })
                .collect();
            let target_building_id = if recipe.building.iter().any(|building| building == "entity")
            {
                0
            } else if recipe
                .building
                .iter()
                .any(|building| building.starts_with("asm"))
            {
                99
            } else {
                98
            };
            let action = craft_action(actor_id, &recipe.id, target_building_id);

            let validated =
                validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog))
                    .expect("valid craft");

            assert!(matches!(validated, ValidatedAction::Craft { .. }));
        }
    }

    #[test]
    fn craft_validation_rejects_unknown_recipe() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let action = craft_action(player.worker_entity_id, "missing", 0);
        let catalog = default_rule_catalog();

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err("unknown recipe `missing`".into())
        );
    }

    #[test]
    fn craft_validation_rejects_missing_inputs() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let action = craft_action(player.worker_entity_id, "iron-plate", 0);
        let catalog = default_rule_catalog();

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err("`iron-plate` requires 1 iron-ore, but actor cargo has 0".into())
        );
    }

    #[test]
    fn research_validation_accepts_valid_research() {
        let mut world = WorldState::new();
        let catalog = default_rule_catalog();
        let player = world.players.get(&1).expect("player 1");
        let actor_id = player.worker_entity_id;
        let actor = world.entities.get_mut(&actor_id).expect("actor");
        actor.cargo = vec![ItemStack {
            kind: "iron-ore".into(),
            amount: 10,
        }];
        let action = research_action(actor_id, "basic-smelting");

        let validated = validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog))
            .expect("valid research");

        match validated {
            ValidatedAction::Research {
                actor_entity_id,
                ref research_id,
                ref inputs,
            } => {
                assert_eq!(actor_entity_id, actor_id);
                assert_eq!(research_id, "basic-smelting");
                assert_eq!(inputs.len(), 1);
                assert_eq!(inputs[0].kind, "iron-ore");
                assert_eq!(inputs[0].amount, 10);
            }
            _ => panic!("expected Research action"),
        }
    }

    #[test]
    fn research_validation_rejects_already_researched() {
        let mut world = WorldState::new();
        let catalog = default_rule_catalog();
        let player = world.players.get_mut(&1).expect("player 1");
        player.researched_ids.insert("basic-smelting".to_string());
        let player = world.players.get(&1).expect("player 1");
        let actor_id = player.worker_entity_id;
        let actor = world.entities.get_mut(&actor_id).expect("actor");
        actor.cargo = vec![ItemStack {
            kind: "iron-ore".into(),
            amount: 10,
        }];
        let action = research_action(actor_id, "basic-smelting");

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err("research `basic-smelting` is already researched".into())
        );
    }

    #[test]
    fn research_validation_rejects_missing_inputs() {
        let world = WorldState::new();
        let catalog = default_rule_catalog();
        let player = world.players.get(&1).expect("player 1");
        let action = research_action(player.worker_entity_id, "basic-smelting");

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err("`basic-smelting` requires 10 iron-ore, but actor cargo has 0".into())
        );
    }

    #[test]
    fn research_validation_rejects_unknown_research() {
        let world = WorldState::new();
        let catalog = default_rule_catalog();
        let player = world.players.get(&1).expect("player 1");
        let action = research_action(player.worker_entity_id, "missing");

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err("unknown research `missing`".into())
        );
    }

    #[test]
    fn research_validation_requires_catalog() {
        let world = WorldState::new();
        let player = world.players.get(&1).expect("player 1");
        let action = research_action(player.worker_entity_id, "basic-smelting");

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), None),
            Err("research action requires rule catalog".into())
        );
    }

    #[test]
    fn craft_validation_rejects_unresearched_recipe() {
        let mut world = WorldState::new();
        let catalog = default_rule_catalog();
        let player = world.players.get(&1).expect("player 1");
        let actor_id = player.worker_entity_id;
        let actor = world.entities.get_mut(&actor_id).expect("actor");
        actor.cargo = vec![
            ItemStack {
                kind: "iron-plate".into(),
                amount: 1,
            },
            ItemStack {
                kind: "copper-wire".into(),
                amount: 3,
            },
        ];
        let action = craft_action(actor_id, "basic-circuit", 0);

        assert_eq!(
            validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog)),
            Err(
                "recipe `basic-circuit` requires researching `advanced-manufacturing` first".into()
            )
        );
    }

    #[test]
    fn craft_validation_accepts_after_research() {
        let mut world = WorldState::new();
        let catalog = default_rule_catalog();
        let player = world.players.get_mut(&1).expect("player 1");
        player
            .researched_ids
            .insert("advanced-manufacturing".to_string());
        let player = world.players.get(&1).expect("player 1");
        let actor_id = player.worker_entity_id;
        let actor = world.entities.get_mut(&actor_id).expect("actor");
        actor.cargo = vec![
            ItemStack {
                kind: "iron-plate".into(),
                amount: 1,
            },
            ItemStack {
                kind: "copper-wire".into(),
                amount: 3,
            },
        ];
        let action = craft_action(actor_id, "basic-circuit", 0);

        let validated = validate_action(&world, 1, action, &ServerRules::default(), Some(&catalog))
            .expect("valid craft after research");

        assert!(matches!(validated, ValidatedAction::Craft { .. }));
    }

    fn build_action(actor_entity_id: u64, building_spec_id: &str) -> fb::Action<'static> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let target = fb::Vec2I::new(2, 0);
        let building_spec_id = fbb.create_string(building_spec_id);
        let action = fb::Action::create(
            &mut fbb,
            &fb::ActionArgs {
                kind: fb::ActionKind::Build,
                actor_entity_id,
                target_position: Some(&target),
                building_spec_id: Some(building_spec_id),
                ..Default::default()
            },
        );
        fbb.finish_minimal(action);
        let data = fbb.finished_data().to_vec().leak();
        flatbuffers::root::<fb::Action<'static>>(data).expect("action")
    }

    fn craft_action(
        actor_entity_id: u64,
        recipe_id: &str,
        target_building_id: u64,
    ) -> fb::Action<'static> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let recipe_id = fbb.create_string(recipe_id);
        let action = fb::Action::create(
            &mut fbb,
            &fb::ActionArgs {
                kind: fb::ActionKind::Craft,
                actor_entity_id,
                target_building_id,
                recipe_id: Some(recipe_id),
                ..Default::default()
            },
        );
        fbb.finish_minimal(action);
        let data = fbb.finished_data().to_vec().leak();
        flatbuffers::root::<fb::Action<'static>>(data).expect("action")
    }

    fn research_action(actor_entity_id: u64, research_id: &str) -> fb::Action<'static> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let research_id = fbb.create_string(research_id);
        let action = fb::Action::create(
            &mut fbb,
            &fb::ActionArgs {
                kind: fb::ActionKind::Research,
                actor_entity_id,
                recipe_id: Some(research_id),
                ..Default::default()
            },
        );
        fbb.finish_minimal(action);
        let data = fbb.finished_data().to_vec().leak();
        flatbuffers::root::<fb::Action<'static>>(data).expect("action")
    }

    fn catalog_with_building(id: &str, width: u32) -> RuleCatalog {
        let catalog = RuleCatalog {
            buildings: BuildingCatalog {
                buildings: vec![
                    BuildingSpec {
                        name: "Entity".into(),
                        id: "entity".into(),
                        crafting_time: Some(1),
                        mining_time: Some(1),
                        smelting_time: None,
                        electricity: Some(0),
                        energy: None,
                        module_slot: Some(0),
                        width: 1,
                        capacity: None,
                        inputs: vec![],
                    },
                    BuildingSpec {
                        name: id.into(),
                        id: id.into(),
                        crafting_time: Some(1),
                        mining_time: Some(1),
                        smelting_time: None,
                        electricity: Some(0),
                        energy: None,
                        module_slot: Some(0),
                        width,
                        capacity: None,
                        inputs: vec![],
                    },
                ],
            },
            recipes: RecipeCatalog {
                recipes: Vec::new(),
            },
            researches: ResearchCatalog {
                researches: Vec::new(),
            },
        };
        validate_rule_catalog(&catalog).expect("valid catalog");
        catalog
    }
}
