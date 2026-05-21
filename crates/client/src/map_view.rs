use std::io::{Write, stdout};
use std::thread;
use std::time::Duration;

use crate::http;
use crate::options::{parse_i32_option, parse_options};
use crate::terminal::{TerminalMode, TerminalSize, terminal_size, write_raw_terminal_frame};
use crate::{
    DEFAULT_MAP_VIEW_INTERVAL_MS, DEFAULT_MAP_VIEW_RADIUS, DEFAULT_MAP_VIEW_X, DEFAULT_MAP_VIEW_Y,
    DEFAULT_PLAYER_ID, DEFAULT_SERVER_ADDR, MAP_VIEW_PAN_STEP,
};

const MAP_ROW_PREFIX_WIDTH: u16 = 6;
const MAP_AXIS_HEIGHT: u16 = 1;
const MAP_HEADER_HEIGHT: u16 = 2;
const MAP_CONTROLS_HEIGHT: u16 = 2;

const RESET: &str = "\x1b[0m";
const CORE_COLOR: &str = "\x1b[1;35m";
const WORKER_COLOR: &str = "\x1b[1;36m";
const BUILDING_COLOR: &str = "\x1b[1;37m";
const IRON_COLOR: &str = "\x1b[38;5;250m";
const COPPER_COLOR: &str = "\x1b[38;5;166m";
const ENERGY_COLOR: &str = "\x1b[1;33m";
const ROCK_COLOR: &str = "\x1b[38;5;245m";
const WATER_COLOR: &str = "\x1b[34m";
const MOUNTAIN_COLOR: &str = "\x1b[1;37m";
const RUIN_COLOR: &str = "\x1b[38;5;94m";
const DANGER_COLOR: &str = "\x1b[1;31m";
const UNSEEN_COLOR: &str = "\x1b[90m";
const ENTITY_COLOR: &str = "\x1b[1;32m";
const TREE_COLOR: &str = "\x1b[32m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MapViewBounds {
    request_radius: i32,
    x_radius: i32,
    y_radius: i32,
    crop: bool,
}

pub(crate) fn map_view(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let player_id = options
        .player_id
        .unwrap_or_else(|| DEFAULT_PLAYER_ID.into());
    let mut x = parse_i32_option(options.x.as_deref(), DEFAULT_MAP_VIEW_X, "--x")?;
    let mut y = parse_i32_option(options.y.as_deref(), DEFAULT_MAP_VIEW_Y, "--y")?;
    let map_id = options.map_id;
    let bounds = map_view_bounds(options.radius.as_deref(), options.once)?;
    let request_radius = bounds.request_radius.to_string();

    if options.once {
        let body = fetch_map_view(&addr, &player_id, map_id.as_deref(), x, y, &request_radius)?;
        print!("{}", colorize_map_view(&crop_map_view(&body, y, bounds)));
        return Ok(());
    }

    let mut terminal = TerminalMode::enter()?;
    print!("\x1b[?25l");

    loop {
        match fetch_map_view(&addr, &player_id, map_id.as_deref(), x, y, &request_radius) {
            Ok(body) => {
                let body = crop_map_view(&body, y, bounds);
                write_raw_terminal_frame(&format!(
                    "\x1b[2J\x1b[H{}\ncontrols: w/a/s/d move viewport by {MAP_VIEW_PAN_STEP}, q/Ctrl-C quit | center=({x}, {y})\n",
                    colorize_map_view(&body)
                ))?;
            }
            Err(err) => {
                write_raw_terminal_frame(&format!(
                    "\x1b[2J\x1b[Hmap-view request failed: {err}\n\ncontrols: w/a/s/d move viewport by {MAP_VIEW_PAN_STEP}, q/Ctrl-C quit | center=({x}, {y})\n"
                ))?;
            }
        }
        stdout().flush()?;

        if terminal.handle_input(&mut x, &mut y, MAP_VIEW_PAN_STEP)? {
            break;
        }
        thread::sleep(Duration::from_millis(DEFAULT_MAP_VIEW_INTERVAL_MS));
    }

    print!("\x1b[?25h\x1b[2J\x1b[H");
    stdout().flush()?;
    Ok(())
}

fn map_view_bounds(
    value: Option<&str>,
    once: bool,
) -> Result<MapViewBounds, Box<dyn std::error::Error>> {
    if let Some(value) = value {
        let radius = parse_i32_option(Some(value), DEFAULT_MAP_VIEW_RADIUS, "--radius")?;
        return Ok(MapViewBounds {
            request_radius: radius,
            x_radius: radius,
            y_radius: radius,
            crop: false,
        });
    }

    Ok(auto_map_view_bounds(terminal_size(), once))
}

fn auto_map_view_bounds(size: Option<TerminalSize>, once: bool) -> MapViewBounds {
    let Some(size) = size else {
        let radius = DEFAULT_MAP_VIEW_RADIUS.parse().unwrap_or(8);
        return MapViewBounds {
            request_radius: radius,
            x_radius: radius,
            y_radius: radius,
            crop: false,
        };
    };

    let extra_height =
        MAP_HEADER_HEIGHT + MAP_AXIS_HEIGHT + if once { 0 } else { MAP_CONTROLS_HEIGHT };
    let x_radius = i32::from(size.cols.saturating_sub(MAP_ROW_PREFIX_WIDTH + 1) / 2);
    let y_radius = i32::from(size.rows.saturating_sub(extra_height + 1) / 2);

    MapViewBounds {
        request_radius: x_radius.max(y_radius),
        x_radius,
        y_radius,
        crop: true,
    }
}

fn fetch_map_view(
    addr: &str,
    player_id: &str,
    map_id: Option<&str>,
    x: i32,
    y: i32,
    radius: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut path = format!("/map-view?player_id={player_id}&x={x}&y={y}&radius={radius}");

    if let Some(map_id) = map_id {
        path.push_str("&map_id=");
        path.push_str(map_id);
    }

    let response = http::get(addr, &path)?;
    Ok(http::response_body(&response)?.to_string())
}

fn colorize_map_view(body: &str) -> String {
    let mut output = String::new();

    for line in body.lines() {
        if let Some((prefix, glyphs)) = map_row(line) {
            output.push_str(prefix);
            for glyph in glyphs.chars() {
                push_colored_glyph(&mut output, glyph);
            }
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }

    output
}

fn crop_map_view(body: &str, center_y: i32, bounds: MapViewBounds) -> String {
    if !bounds.crop {
        return body.to_string();
    }

    let mut output = String::new();
    for line in body.lines() {
        if let Some((y, prefix, glyphs)) = parsed_map_row(line) {
            if (center_y - bounds.y_radius..=center_y + bounds.y_radius).contains(&y) {
                output.push_str(prefix);
                output.push_str(&crop_glyphs(glyphs, bounds.x_radius));
                output.push('\n');
            }
        } else if let Some(axis) = map_axis(line) {
            output.push_str("      ");
            output.push_str(&crop_glyphs(axis, bounds.x_radius));
            output.push('\n');
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

fn crop_glyphs(glyphs: &str, x_radius: i32) -> String {
    let width = usize::try_from(x_radius.saturating_mul(2).saturating_add(1)).unwrap_or(usize::MAX);
    let glyph_count = glyphs.chars().count();
    let actual_radius = glyph_count.saturating_sub(1) / 2;
    let x_radius = usize::try_from(x_radius).unwrap_or_default();
    let start = actual_radius.saturating_sub(x_radius);

    glyphs.chars().skip(start).take(width).collect()
}

fn map_row(line: &str) -> Option<(&str, &str)> {
    let (_, prefix, glyphs) = parsed_map_row(line)?;
    Some((prefix, glyphs))
}

fn parsed_map_row(line: &str) -> Option<(i32, &str, &str)> {
    if line.len() < 6 || line.as_bytes().get(5) != Some(&b' ') {
        return None;
    }

    let y = line[..5].trim().parse::<i32>().ok()?;
    let (prefix, glyphs) = line.split_at(6);
    Some((y, prefix, glyphs))
}

fn map_axis(line: &str) -> Option<&str> {
    let axis = line.strip_prefix("      ")?;
    if axis.chars().all(|glyph| matches!(glyph, '-' | '+')) {
        Some(axis)
    } else {
        None
    }
}

fn push_colored_glyph(output: &mut String, glyph: char) {
    if let Some(color) = glyph_color(glyph) {
        output.push_str(color);
        output.push(glyph);
        output.push_str(RESET);
    } else {
        output.push(glyph);
    }
}

fn glyph_color(glyph: char) -> Option<&'static str> {
    match glyph {
        'C' => Some(CORE_COLOR),
        'W' => Some(WORKER_COLOR),
        'E' => Some(ENTITY_COLOR),
        'B' => Some(BUILDING_COLOR),
        'i' => Some(IRON_COLOR),
        'c' => Some(COPPER_COLOR),
        'e' => Some(ENERGY_COLOR),
        'r' => Some(ROCK_COLOR),
        's' => Some(ROCK_COLOR),
        't' => Some(TREE_COLOR),
        'w' => Some(WATER_COLOR),
        '~' => Some(WATER_COLOR),
        '^' => Some(MOUNTAIN_COLOR),
        'x' => Some(RUIN_COLOR),
        '!' => Some(DANGER_COLOR),
        '?' => Some(UNSEEN_COLOR),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorizes_map_rows_but_not_plain_tiles() {
        let body = "tick=1\nlegend\n    0 CWi.ce r~^x!?\n      ---+---\n";

        let colorized = colorize_map_view(body);

        assert!(colorized.contains(CORE_COLOR));
        assert!(colorized.contains(WATER_COLOR));
        assert!(colorized.contains(MOUNTAIN_COLOR));
        assert!(colorized.contains("."));
        assert!(!colorized.contains(&format!("{RESET}.{RESET}")));
        assert!(colorized.contains("legend"));
    }

    #[test]
    fn auto_bounds_use_terminal_size_for_interactive_view() {
        let bounds = auto_map_view_bounds(Some(TerminalSize { rows: 24, cols: 80 }), false);

        assert_eq!(
            bounds,
            MapViewBounds {
                request_radius: 36,
                x_radius: 36,
                y_radius: 9,
                crop: true,
            }
        );
    }

    #[test]
    fn auto_bounds_reserve_less_height_for_once_view() {
        let bounds = auto_map_view_bounds(Some(TerminalSize { rows: 24, cols: 80 }), true);

        assert_eq!(
            bounds,
            MapViewBounds {
                request_radius: 36,
                x_radius: 36,
                y_radius: 10,
                crop: true,
            }
        );
    }

    #[test]
    fn auto_bounds_fall_back_without_terminal_size() {
        let bounds = auto_map_view_bounds(None, false);

        assert_eq!(
            bounds,
            MapViewBounds {
                request_radius: 8,
                x_radius: 8,
                y_radius: 8,
                crop: false,
            }
        );
    }

    #[test]
    fn crop_map_view_keeps_rectangular_view_around_center() {
        let body = "tick=1\nlegend\n    2 abcdefg\n    1 hijklmn\n    0 opqrstu\n   -1 vwxyz12\n   -2 3456789\n      ---+---\n";

        let cropped = crop_map_view(
            body,
            0,
            MapViewBounds {
                request_radius: 3,
                x_radius: 1,
                y_radius: 1,
                crop: true,
            },
        );

        assert_eq!(
            cropped,
            "tick=1\nlegend\n    1 jkl\n    0 qrs\n   -1 xyz\n      -+-\n"
        );
    }
}
