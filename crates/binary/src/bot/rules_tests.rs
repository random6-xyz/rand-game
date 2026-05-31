use rand_game_common::rules::{default_rule_catalog, validate_rule_catalog};

#[test]
fn generated_common_rules_api_is_usable_by_sample_bot_crate() {
    let catalog = default_rule_catalog();
    validate_rule_catalog(&catalog).expect("valid catalog");
    assert!(catalog.buildings.buildings.iter().any(|b| b.id == "entity"));
    assert!(catalog.recipes.recipes.iter().any(|r| r.id == "iron-plate"));
}
