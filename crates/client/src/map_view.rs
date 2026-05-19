use std::io::{Write, stdout};
use std::thread;
use std::time::Duration;

use crate::http;
use crate::options::{parse_i32_option, parse_options};
use crate::terminal::{TerminalMode, write_raw_terminal_frame};
use crate::{
    DEFAULT_MAP_VIEW_INTERVAL_MS, DEFAULT_MAP_VIEW_RADIUS, DEFAULT_MAP_VIEW_X, DEFAULT_MAP_VIEW_Y,
    DEFAULT_PLAYER_ID, DEFAULT_SERVER_ADDR, MAP_VIEW_PAN_STEP,
};

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

pub(crate) fn map_view(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let player_id = options
        .player_id
        .unwrap_or_else(|| DEFAULT_PLAYER_ID.into());
    let mut x = parse_i32_option(options.x.as_deref(), DEFAULT_MAP_VIEW_X, "--x")?;
    let mut y = parse_i32_option(options.y.as_deref(), DEFAULT_MAP_VIEW_Y, "--y")?;
    let radius = options
        .radius
        .unwrap_or_else(|| DEFAULT_MAP_VIEW_RADIUS.into());
    let map_id = options.map_id;

    if options.once {
        let body = fetch_map_view(&addr, &player_id, map_id.as_deref(), x, y, &radius)?;
        print!("{}", colorize_map_view(&body));
        return Ok(());
    }

    let mut terminal = TerminalMode::enter()?;
    print!("\x1b[?25l");

    loop {
        match fetch_map_view(&addr, &player_id, map_id.as_deref(), x, y, &radius) {
            Ok(body) => {
                write_raw_terminal_frame(&format!(
                    "\x1b[2J\x1b[H{}\ncontrols: w/a/s/d move viewport by {MAP_VIEW_PAN_STEP}, q quit | center=({x}, {y})\n",
                    colorize_map_view(&body)
                ))?;
            }
            Err(err) => {
                write_raw_terminal_frame(&format!(
                    "\x1b[2J\x1b[Hmap-view request failed: {err}\n\ncontrols: w/a/s/d move viewport by {MAP_VIEW_PAN_STEP}, q quit | center=({x}, {y})\n"
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

fn map_row(line: &str) -> Option<(&str, &str)> {
    if line.len() < 6 || line.as_bytes().get(5) != Some(&b' ') {
        return None;
    }

    line[..5].trim().parse::<i32>().ok()?;
    Some(line.split_at(6))
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
}
