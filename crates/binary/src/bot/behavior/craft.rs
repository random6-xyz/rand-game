use std::collections::HashMap;

use rand_game_common::rules::RecipeSpec;

use super::super::model::{ActionPlan, PlannedAction};

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
