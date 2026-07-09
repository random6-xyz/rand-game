use std::collections::HashSet;
use std::path::Path;

pub use crate::generated::rules::default_rule_catalog;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BuildingCatalog {
    pub buildings: Vec<BuildingSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BuildingSpec {
    pub name: String,
    pub id: String,
    pub crafting_time: Option<u32>,
    pub mining_time: Option<u32>,
    pub smelting_time: Option<u32>,
    pub electricity: Option<i32>,
    pub energy: Option<i32>,
    pub module_slot: Option<u32>,
    pub width: u32,
    pub capacity: Option<u32>,
    #[serde(default)]
    pub inputs: Vec<ItemStackSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RecipeCatalog {
    pub recipes: Vec<RecipeSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RecipeSpec {
    pub name: String,
    pub id: String,
    pub max_stack: u32,
    pub building: Vec<String>,
    pub inputs: Vec<ItemStackSpec>,
    pub crafting_time: u32,
    pub outputs: Vec<ItemStackSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ItemStackSpec {
    pub kind: String,
    pub amount: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ResearchCatalog {
    pub researches: Vec<ResearchSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ResearchSpec {
    pub name: String,
    pub id: String,
    pub inputs: Vec<ItemStackSpec>,
    pub unlocked_recipes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RuleCatalog {
    pub buildings: BuildingCatalog,
    pub recipes: RecipeCatalog,
    pub researches: ResearchCatalog,
}

pub fn parse_building_catalog_str(input: &str) -> Result<BuildingCatalog, serde_yaml::Error> {
    serde_yaml::from_str(input)
}

pub fn parse_recipe_catalog_str(input: &str) -> Result<RecipeCatalog, serde_yaml::Error> {
    serde_yaml::from_str(input)
}

pub fn parse_research_catalog_str(input: &str) -> Result<ResearchCatalog, serde_yaml::Error> {
    serde_yaml::from_str(input)
}

pub fn load_building_catalog_path(
    path: &Path,
) -> Result<BuildingCatalog, Box<dyn std::error::Error>> {
    let input = std::fs::read_to_string(path)?;
    Ok(parse_building_catalog_str(&input)?)
}

pub fn load_recipe_catalog_path(path: &Path) -> Result<RecipeCatalog, Box<dyn std::error::Error>> {
    let input = std::fs::read_to_string(path)?;
    Ok(parse_recipe_catalog_str(&input)?)
}

pub fn validate_rule_catalog(catalog: &RuleCatalog) -> Result<(), String> {
    validate_building_catalog(&catalog.buildings)?;
    validate_recipe_catalog(&catalog.recipes, &catalog.buildings)?;
    validate_research_catalog(&catalog.researches, &catalog.recipes)
}

pub fn validate_building_catalog(catalog: &BuildingCatalog) -> Result<(), String> {
    let mut ids = HashSet::new();
    for building in &catalog.buildings {
        validate_non_empty(&building.id, "building id")?;
        validate_non_empty(&building.name, "building name")?;
        if building.width == 0 {
            return Err(format!(
                "building `{}` width must be greater than 0",
                building.id
            ));
        }
        for input in &building.inputs {
            validate_non_empty(&input.kind, "building input kind")?;
            if input.amount == 0 {
                return Err(format!(
                    "building `{}` input `{}` amount must be greater than 0",
                    building.id, input.kind
                ));
            }
        }
        if !ids.insert(building.id.as_str()) {
            return Err(format!("duplicate building id `{}`", building.id));
        }
    }
    Ok(())
}

pub fn validate_recipe_catalog(
    catalog: &RecipeCatalog,
    buildings: &BuildingCatalog,
) -> Result<(), String> {
    let building_ids = buildings
        .buildings
        .iter()
        .map(|building| building.id.as_str())
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();

    for recipe in &catalog.recipes {
        validate_non_empty(&recipe.id, "recipe id")?;
        validate_non_empty(&recipe.name, "recipe name")?;
        if recipe.max_stack == 0 {
            return Err(format!(
                "recipe `{}` max_stack must be greater than 0",
                recipe.id
            ));
        }
        if recipe.crafting_time == 0 {
            return Err(format!(
                "recipe `{}` crafting_time must be greater than 0",
                recipe.id
            ));
        }
        if recipe.building.is_empty() {
            return Err(format!(
                "recipe `{}` must reference at least one building",
                recipe.id
            ));
        }
        for building_id in &recipe.building {
            validate_non_empty(building_id, "recipe building id")?;
            if !building_ids.contains(building_id.as_str()) {
                return Err(format!(
                    "recipe `{}` references unknown building `{building_id}`",
                    recipe.id
                ));
            }
        }
        validate_stacks(&recipe.id, "input", &recipe.inputs)?;
        validate_stacks(&recipe.id, "output", &recipe.outputs)?;
        if !ids.insert(recipe.id.as_str()) {
            return Err(format!("duplicate recipe id `{}`", recipe.id));
        }
    }
    Ok(())
}

pub fn validate_research_catalog(
    catalog: &ResearchCatalog,
    recipes: &RecipeCatalog,
) -> Result<(), String> {
    let recipe_ids = recipes
        .recipes
        .iter()
        .map(|recipe| recipe.id.as_str())
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();

    for research in &catalog.researches {
        validate_non_empty(&research.id, "research id")?;
        validate_non_empty(&research.name, "research name")?;
        if !ids.insert(research.id.as_str()) {
            return Err(format!("duplicate research id `{}`", research.id));
        }
        validate_stacks(&research.id, "input", &research.inputs)?;
        if research.unlocked_recipes.is_empty() {
            return Err(format!(
                "research `{}` must unlock at least one recipe",
                research.id
            ));
        }
        for recipe_id in &research.unlocked_recipes {
            validate_non_empty(recipe_id, "unlocked recipe id")?;
            if !recipe_ids.contains(recipe_id.as_str()) {
                return Err(format!(
                    "research `{}` unlocks unknown recipe `{recipe_id}`",
                    research.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_stacks(recipe_id: &str, field: &str, stacks: &[ItemStackSpec]) -> Result<(), String> {
    if stacks.is_empty() {
        return Err(format!(
            "recipe `{recipe_id}` must have at least one {field}"
        ));
    }
    for stack in stacks {
        validate_non_empty(&stack.kind, "item kind")?;
        if stack.amount == 0 {
            return Err(format!(
                "recipe `{recipe_id}` {field} `{}` amount must be greater than 0",
                stack.kind
            ));
        }
    }
    Ok(())
}

fn validate_non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILDING_YAML: &str = r#"
buildings:
  - name: "Entity"
    id: "entity"
    crafting_time: 5
    mining_time: 5
    electricity: 0
    module_slot: 0
    width: 1
  - name: "Assembly Machine 1"
    id: "asm-1"
    crafting_time: 10
    electricity: 5
    module_slot: 2
    width: 1
"#;

    const RECIPE_YAML: &str = r#"
recipes:
  - name: "Iron Plate"
    id: "iron-plate"
    max_stack: 1000
    building:
      - "entity"
      - "asm-1"
    inputs:
      - kind: "iron-ore"
        amount: 1
    crafting_time: 5
    outputs:
      - kind: "iron-plate"
        amount: 1
"#;

    #[test]
    fn parses_and_validates_catalogs() {
        let buildings = parse_building_catalog_str(BUILDING_YAML).expect("buildings");
        let recipes = parse_recipe_catalog_str(RECIPE_YAML).expect("recipes");
        let researches = ResearchCatalog {
            researches: Vec::new(),
        };
        let catalog = RuleCatalog {
            buildings,
            recipes,
            researches,
        };

        validate_rule_catalog(&catalog).expect("valid catalog");
        assert_eq!(catalog.buildings.buildings[1].id, "asm-1");
        assert_eq!(catalog.recipes.recipes[0].outputs[0].kind, "iron-plate");
    }

    #[test]
    fn rejects_unknown_recipe_building() {
        let buildings = parse_building_catalog_str(BUILDING_YAML).expect("buildings");
        let recipes = parse_recipe_catalog_str(
            r#"
recipes:
  - name: "Bad"
    id: "bad"
    max_stack: 1
    building: ["missing"]
    inputs: [{ kind: "iron-ore", amount: 1 }]
    crafting_time: 1
    outputs: [{ kind: "iron-plate", amount: 1 }]
"#,
        )
        .expect("recipes");
        let _researches = ResearchCatalog {
            researches: Vec::new(),
        };

        assert!(validate_recipe_catalog(&recipes, &buildings).is_err());
    }
}
