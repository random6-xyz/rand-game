use std::collections::HashSet;

use crate::model::{Position, ResourceKind};
use crate::world::WorldState;

pub fn render_ascii_map(
    world: &WorldState,
    player_id: u64,
    center: Position,
    radius: i32,
    reveal_all: bool,
) -> String {
    let visible_positions = world
        .visible_tiles_for(player_id)
        .into_iter()
        .map(|tile| tile.position)
        .collect::<HashSet<_>>();
    let entity_positions = world
        .entities
        .values()
        .map(|entity| entity.position)
        .collect::<HashSet<_>>();

    let mut output = String::new();
    output.push_str(&format!(
        "tick={} map_id={} player_id={} center=({}, {}) radius={} reveal_all={}\n",
        world.tick, world.map_id, player_id, center.x, center.y, radius, reveal_all
    ));
    output.push_str(
        "legend: E entity, B building, i iron, c copper, e energy, s stone, t tree, w water, . empty, ? unseen\n",
    );

    for ty in (center.y - radius..=center.y + radius).rev() {
        output.push_str(&format!("{ty:>5} "));
        for tx in center.x - radius..=center.x + radius {
            let position = Position::new(tx, ty);
            output.push(tile_glyph(
                world,
                &visible_positions,
                &entity_positions,
                position,
                reveal_all,
            ));
        }
        output.push('\n');
    }
    output.push_str("      ");
    for tx in center.x - radius..=center.x + radius {
        output.push(if tx == center.x { '+' } else { '-' });
    }
    output.push('\n');
    output
}

fn tile_glyph(
    world: &WorldState,
    visible_positions: &HashSet<Position>,
    entity_positions: &HashSet<Position>,
    position: Position,
    reveal_all: bool,
) -> char {
    if !reveal_all && !visible_positions.contains(&position) {
        return '?';
    }
    if entity_positions.contains(&position) {
        return 'E';
    }

    let tile = world.tile_at(position);
    if tile.building_id.is_some() {
        return 'B';
    }
    if let Some(resource) = tile.resource {
        return match resource.kind {
            ResourceKind::Iron => 'i',
            ResourceKind::Copper => 'c',
            ResourceKind::Energy => 'e',
            ResourceKind::Stone => 's',
            ResourceKind::Tree => 't',
            ResourceKind::Water => 'w',
        };
    }
    '.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_map_renders_visible_owned_entities() {
        let world = WorldState::new();

        let map = render_ascii_map(&world, 1, Position::new(0, 0), 2, false);

        assert!(map.contains("player_id=1"));
        assert!(map.contains('E'));
    }

    #[test]
    fn ascii_map_debug_mode_reveals_tiles_outside_visibility() {
        let world = WorldState::new();

        let hidden_map = render_ascii_map(&world, 1, Position::new(100, 100), 1, false);
        let debug_map = render_ascii_map(&world, 1, Position::new(100, 100), 1, true);

        assert!(hidden_map.contains('?'));
        assert!(!debug_map.lines().skip(2).any(|line| line.contains('?')));
        assert!(debug_map.contains("reveal_all=true"));
    }
}
