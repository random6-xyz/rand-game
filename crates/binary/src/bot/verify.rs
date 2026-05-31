use std::collections::{HashSet, VecDeque};

use rand_game_common::fb::{Observation, ResourceKind};
use rand_game_common::rules::default_rule_catalog;

use super::behavior::BehaviorContext;
use super::behavior::craft::try_craft_recipe;
use super::behavior::mine::try_mine_specific;
use super::behavior::navigate::try_move_toward_specific_resource;
use super::model::{Actor, PlannedAction, Position, ResourceTile};

const VERIFY_VERSION: u8 = 1;

#[derive(Debug, Clone)]
enum VerifyPhase {
    MineIron { needed: u32 },
    MineCopper { needed: u32 },
    CraftRecipes { remaining: VecDeque<(String, u32)> },
    Done,
}

#[derive(Debug)]
struct VerifyState {
    phase: VerifyPhase,
    completed_recipes: Vec<String>,
}

impl VerifyState {
    fn new() -> Self {
        VerifyState {
            phase: VerifyPhase::MineIron { needed: 5 },
            completed_recipes: Vec::new(),
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![VERIFY_VERSION];
        match &self.phase {
            VerifyPhase::MineIron { needed } => {
                bytes.push(0);
                bytes.extend_from_slice(&needed.to_le_bytes());
            }
            VerifyPhase::MineCopper { needed } => {
                bytes.push(1);
                bytes.extend_from_slice(&needed.to_le_bytes());
            }
            VerifyPhase::CraftRecipes { remaining } => {
                bytes.push(2);
                bytes.extend_from_slice(&(remaining.len() as u32).to_le_bytes());
                for (recipe_id, count) in remaining {
                    bytes.push(recipe_id.len() as u8);
                    bytes.extend_from_slice(recipe_id.as_bytes());
                    bytes.extend_from_slice(&count.to_le_bytes());
                }
            }
            VerifyPhase::Done => {
                bytes.push(3);
            }
        }
        bytes.extend_from_slice(&(self.completed_recipes.len() as u32).to_le_bytes());
        for recipe_id in &self.completed_recipes {
            bytes.push(recipe_id.len() as u8);
            bytes.extend_from_slice(recipe_id.as_bytes());
        }
        bytes
    }

    fn decode(data: &[u8]) -> Option<Self> {
        if data.is_empty() || data[0] != VERIFY_VERSION {
            return None;
        }
        let phase_tag = *data.get(1)?;
        let mut pos = 2usize;
        let phase = match phase_tag {
            0 => {
                let needed = read_u32_le(data, &mut pos)?;
                VerifyPhase::MineIron { needed }
            }
            1 => {
                let needed = read_u32_le(data, &mut pos)?;
                VerifyPhase::MineCopper { needed }
            }
            2 => {
                let count = read_u32_le(data, &mut pos)? as usize;
                let mut remaining = VecDeque::new();
                for _ in 0..count {
                    let len = *data.get(pos)? as usize;
                    pos += 1;
                    let id = std::str::from_utf8(data.get(pos..pos + len)?).ok()?;
                    pos += len;
                    let c = read_u32_le(data, &mut pos)?;
                    remaining.push_back((id.to_string(), c));
                }
                VerifyPhase::CraftRecipes { remaining }
            }
            3 => VerifyPhase::Done,
            _ => return None,
        };
        let completed_count = read_u32_le(data, &mut pos)? as usize;
        let mut completed_recipes = Vec::new();
        for _ in 0..completed_count {
            let len = *data.get(pos)? as usize;
            pos += 1;
            let id = std::str::from_utf8(data.get(pos..pos + len)?).ok()?;
            pos += len;
            completed_recipes.push(id.to_string());
        }
        Some(VerifyState {
            phase,
            completed_recipes,
        })
    }
}

fn read_u32_le(data: &[u8], pos: &mut usize) -> Option<u32> {
    let bytes = data.get(*pos..*pos + 4)?;
    *pos += 4;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn output_verification_done(state: &VerifyState) {
    let total: usize = 7;
    let verified = state.completed_recipes.len();
    let catalog = default_rule_catalog();

    let mut details: Vec<String> = Vec::new();
    for recipe in &catalog.recipes.recipes {
        let status = if state.completed_recipes.contains(&recipe.id) {
            "ok"
        } else {
            "failed"
        };
        details.push(format!(
            r#"{{"recipe_id":"{}","status":"{}"}}"#,
            recipe.id, status
        ));
    }
    let failed: Vec<&str> = catalog
        .recipes
        .recipes
        .iter()
        .filter(|r| !state.completed_recipes.contains(&r.id))
        .map(|r| r.id.as_str())
        .collect();

    eprintln!(
        r#"{{"event":"recipe_verification_done","verified":{},"total":{},"failed":[{}],"details":[{}]}}"#,
        verified,
        total,
        failed
            .iter()
            .map(|s| format!(r#""{s}""#))
            .collect::<Vec<_>>()
            .join(","),
        details.join(",")
    );
}

pub(crate) fn plan_verify_actions(
    observation: Observation<'_>,
    persistent_memory: &[u8],
    max_actions: usize,
) -> (Vec<PlannedAction>, Vec<u8>) {
    let mut state = VerifyState::decode(persistent_memory).unwrap_or_else(VerifyState::new);

    let Some((actor_id, actor_pos)) = super::observation::worker_entity(observation) else {
        return (Vec::new(), state.encode());
    };

    let mut actor = Actor {
        id: actor_id,
        position: actor_pos,
        cargo: Default::default(),
    };

    let mut resources = super::observation::visible_resource_tiles(observation);
    let passable_positions = super::observation::visible_passable_positions(observation);
    let cargo = super::observation::worker_cargo_map(observation);
    let catalog = default_rule_catalog();

    let mut actions: Vec<PlannedAction> = Vec::new();
    let action_budget = max_actions.min(1);

    for _ in 0..action_budget {
        let action = match &mut state.phase {
            VerifyPhase::MineIron { needed } => {
                if *needed == 0 {
                    state.phase = VerifyPhase::MineCopper { needed: 2 };
                    continue;
                }
                plan_mine_or_move(
                    &mut actor,
                    &mut resources,
                    &passable_positions,
                    ResourceKind::Iron,
                    needed,
                )
            }
            VerifyPhase::MineCopper { needed } => {
                if *needed == 0 {
                    let remaining = VecDeque::from([
                        ("iron-plate".to_string(), 5u32),
                        ("copper-plate".to_string(), 2),
                        ("iron-gear".to_string(), 1),
                        ("iron-rod".to_string(), 1),
                        ("copper-wire".to_string(), 2),
                        ("basic-circuit".to_string(), 1),
                        ("conveyor-belt".to_string(), 1),
                    ]);
                    state.phase = VerifyPhase::CraftRecipes { remaining };
                    continue;
                }
                plan_mine_or_move(
                    &mut actor,
                    &mut resources,
                    &passable_positions,
                    ResourceKind::Copper,
                    needed,
                )
            }
            VerifyPhase::CraftRecipes { remaining } => {
                if remaining.is_empty() {
                    state.phase = VerifyPhase::Done;
                    continue;
                }
                let recipe_id = remaining[0].0.clone();
                let recipe = match catalog.recipes.recipes.iter().find(|r| r.id == recipe_id) {
                    Some(r) => r,
                    None => {
                        remaining.pop_front();
                        continue;
                    }
                };
                if let Some(action) = try_craft_recipe(recipe, actor_id, &cargo) {
                    remaining[0].1 = remaining[0].1.saturating_sub(1);
                    let completed = remaining[0].1 == 0;
                    if completed {
                        let recipe_id = remaining.pop_front().unwrap().0;
                        state.completed_recipes.push(recipe_id);
                    }
                    Some(action)
                } else {
                    None
                }
            }
            VerifyPhase::Done => {
                output_verification_done(&state);
                None
            }
        };

        if let Some(action) = action {
            actions.push(action);
        }
    }

    (actions, state.encode())
}

fn plan_mine_or_move(
    actor: &mut Actor,
    resources: &mut [ResourceTile],
    passable_positions: &HashSet<Position>,
    kind: ResourceKind,
    needed: &mut u32,
) -> Option<PlannedAction> {
    let mut context = BehaviorContext::new(resources, passable_positions, false);
    if let Some(action) = try_mine_specific(actor, &mut context, kind) {
        *needed = needed.saturating_sub(1);
        return Some(action);
    }
    if let Some(action) = try_move_toward_specific_resource(actor, &mut context, kind) {
        return Some(action);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_mine_iron_with_five_needed() {
        let state = VerifyState::new();
        assert!(matches!(state.phase, VerifyPhase::MineIron { needed: 5 }));
        assert!(state.completed_recipes.is_empty());
    }

    #[test]
    fn encodes_and_decodes_mine_iron_phase() {
        let state = VerifyState {
            phase: VerifyPhase::MineIron { needed: 3 },
            completed_recipes: vec![],
        };
        let encoded = state.encode();
        let decoded = VerifyState::decode(&encoded).expect("decode");
        assert!(matches!(decoded.phase, VerifyPhase::MineIron { needed: 3 }));
    }

    #[test]
    fn encodes_and_decodes_mine_copper_phase() {
        let state = VerifyState {
            phase: VerifyPhase::MineCopper { needed: 2 },
            completed_recipes: vec![],
        };
        let encoded = state.encode();
        let decoded = VerifyState::decode(&encoded).expect("decode");
        assert!(matches!(
            decoded.phase,
            VerifyPhase::MineCopper { needed: 2 }
        ));
    }

    #[test]
    fn encodes_and_decodes_craft_recipes_phase() {
        let remaining = VecDeque::from([
            ("iron-plate".to_string(), 3u32),
            ("copper-plate".to_string(), 1),
        ]);
        let state = VerifyState {
            phase: VerifyPhase::CraftRecipes {
                remaining: remaining.clone(),
            },
            completed_recipes: vec!["iron-ore".to_string()],
        };
        let encoded = state.encode();
        let decoded = VerifyState::decode(&encoded).expect("decode");
        match decoded.phase {
            VerifyPhase::CraftRecipes {
                remaining: decoded_remaining,
            } => {
                let a: Vec<_> = decoded_remaining.into_iter().collect();
                let b: Vec<_> = remaining.into_iter().collect();
                assert_eq!(a, b);
            }
            _ => panic!("expected CraftRecipes"),
        }
        assert_eq!(decoded.completed_recipes, vec!["iron-ore"]);
    }

    #[test]
    fn encodes_and_decodes_done_phase() {
        let state = VerifyState {
            phase: VerifyPhase::Done,
            completed_recipes: vec!["iron-plate".to_string(), "copper-plate".to_string()],
        };
        let encoded = state.encode();
        let decoded = VerifyState::decode(&encoded).expect("decode");
        assert!(matches!(decoded.phase, VerifyPhase::Done));
        assert_eq!(decoded.completed_recipes.len(), 2);
    }

    #[test]
    fn decode_empty_returns_none() {
        assert!(VerifyState::decode(&[]).is_none());
    }

    #[test]
    fn decode_wrong_version_returns_none() {
        assert!(VerifyState::decode(&[99, 0, 5, 0, 0, 0, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn craft_recipe_check_requires_all_inputs() {
        let catalog = default_rule_catalog();
        let recipe = catalog
            .recipes
            .recipes
            .iter()
            .find(|r| r.id == "iron-gear")
            .expect("iron-gear recipe");

        let mut cargo = std::collections::HashMap::new();
        cargo.insert("iron-plate".to_string(), 1u32);
        assert!(try_craft_recipe(recipe, 1, &cargo).is_none());

        cargo.insert("iron-plate".to_string(), 2u32);
        assert!(try_craft_recipe(recipe, 1, &cargo).is_some());
    }

    #[test]
    fn craft_recipe_uses_target_building_id_zero() {
        let catalog = default_rule_catalog();
        let recipe = catalog
            .recipes
            .recipes
            .iter()
            .find(|r| r.id == "iron-plate")
            .expect("iron-plate recipe");

        let mut cargo = std::collections::HashMap::new();
        cargo.insert("iron-ore".to_string(), 1u32);
        let action = try_craft_recipe(recipe, 42, &cargo).expect("craft");
        match action.plan {
            super::super::model::ActionPlan::Craft {
                ref recipe_id,
                target_building_id,
            } => {
                assert_eq!(recipe_id, "iron-plate");
                assert_eq!(target_building_id, 0);
            }
            _ => panic!("expected Craft action"),
        }
    }
}
