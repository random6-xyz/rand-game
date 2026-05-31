use rand_game_common::fb::{ActionKind, root_as_game_output};
use rand_game_common::rules::{default_rule_catalog, validate_rule_catalog};

use crate::bot::model::{ActionPlan, PlannedAction};
use crate::bot::output::build_output_with_actions;

#[test]
fn generated_common_rules_api_is_usable_by_sample_bot_crate() {
    let catalog = default_rule_catalog();
    validate_rule_catalog(&catalog).expect("valid catalog");
    assert!(catalog.buildings.buildings.iter().any(|b| b.id == "entity"));
    assert!(!catalog.recipes.recipes.is_empty());
}

#[test]
fn all_generated_recipes_can_be_serialized_as_craft_actions() {
    let catalog = default_rule_catalog();
    validate_rule_catalog(&catalog).expect("valid catalog");

    for recipe in &catalog.recipes.recipes {
        assert!(!recipe.id.trim().is_empty());
        assert!(!recipe.name.trim().is_empty());
        assert!(!recipe.building.is_empty());
        assert!(!recipe.inputs.is_empty());
        assert!(!recipe.outputs.is_empty());
        assert!(recipe.crafting_time > 0);
        assert!(recipe.max_stack > 0);

        let payload = build_output_with_actions(vec![PlannedAction {
            actor_id: 3,
            plan: ActionPlan::Craft {
                recipe_id: recipe.id.clone(),
                target_building_id: 0,
            },
        }]);
        let output = root_as_game_output(&payload).expect("valid game output");
        let actions = output.actions().expect("actions");
        let action = actions.get(0);

        assert_eq!(actions.len(), 1);
        assert_eq!(action.kind(), ActionKind::Craft);
        assert_eq!(action.recipe_id(), Some(recipe.id.as_str()));
    }
}
