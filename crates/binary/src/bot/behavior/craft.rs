use std::collections::HashMap;

use rand_game_common::rules::RecipeSpec;

use super::super::model::{ActionPlan, Actor, PlannedAction, SmallCargo};
use super::BehaviorContext;

struct EntityRecipe {
    recipe_id: &'static str,
    inputs: &'static [(&'static str, u32)],
}

const ENTITY_RECIPES: &[EntityRecipe] = &[
    EntityRecipe {
        recipe_id: "iron-plate",
        inputs: &[("iron-ore", 1)],
    },
    EntityRecipe {
        recipe_id: "copper-plate",
        inputs: &[("copper-ore", 1)],
    },
    EntityRecipe {
        recipe_id: "iron-gear",
        inputs: &[("iron-plate", 2)],
    },
    EntityRecipe {
        recipe_id: "iron-rod",
        inputs: &[("iron-plate", 3)],
    },
    EntityRecipe {
        recipe_id: "copper-wire",
        inputs: &[("copper-ore", 1)],
    },
    EntityRecipe {
        recipe_id: "basic-circuit",
        inputs: &[("iron-plate", 1), ("copper-wire", 3)],
    },
    EntityRecipe {
        recipe_id: "conveyor-belt",
        inputs: &[("iron-plate", 1), ("iron-gear", 1)],
    },
];

pub(crate) fn try_craft_recipe(
    recipe: &RecipeSpec,
    actor_id: u64,
    cargo: &HashMap<String, u32>,
) -> Option<PlannedAction> {
    for input in &recipe.inputs {
        let have = cargo.get(&input.kind).copied().unwrap_or(0);
        if have < input.amount {
            return None;
        }
    }

    eprintln!("sample_bot: crafting {} with actor {}", recipe.id, actor_id);

    Some(PlannedAction {
        actor_id,
        plan: ActionPlan::Craft {
            recipe_id: recipe.id.clone(),
            target_building_id: 0,
        },
    })
}

pub(crate) fn try_craft_any_entity_recipe(
    actor: &mut Actor,
    _context: &mut BehaviorContext<'_>,
) -> Option<PlannedAction> {
    let cargo_map = actor.cargo.to_map();
    for entry in ENTITY_RECIPES {
        let can_craft = entry
            .inputs
            .iter()
            .all(|(kind, amount)| cargo_map.get(*kind).copied().unwrap_or(0) >= *amount);

        if can_craft {
            eprintln!(
                "sample_bot: crafting {} with actor {}",
                entry.recipe_id, actor.id
            );
            for (kind, amount) in entry.inputs {
                deduct_from_cargo(&mut actor.cargo, kind, *amount);
            }
            return Some(PlannedAction {
                actor_id: actor.id,
                plan: ActionPlan::Craft {
                    recipe_id: entry.recipe_id.to_string(),
                    target_building_id: 0,
                },
            });
        }
    }
    None
}

fn deduct_from_cargo(cargo: &mut SmallCargo, kind: &str, amount: u32) {
    match kind {
        "iron-ore" => cargo.iron = cargo.iron.saturating_sub(amount),
        "copper-ore" => cargo.copper = cargo.copper.saturating_sub(amount),
        "energy" => cargo.energy = cargo.energy.saturating_sub(amount),
        "stone" => cargo.stone = cargo.stone.saturating_sub(amount),
        "tree" => cargo.tree = cargo.tree.saturating_sub(amount),
        "water" => cargo.water = cargo.water.saturating_sub(amount),
        _ => {}
    }
}
