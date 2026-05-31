use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;

mod http;
mod map_view;
mod options;
mod terminal;

const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_PLAYER_ID: &str = "1";
const DEFAULT_BOT_PATH: &str = "target/debug/rand-game-binary";
const DEFAULT_MAP_VIEW_X: &str = "0";
const DEFAULT_MAP_VIEW_Y: &str = "0";
const DEFAULT_MAP_VIEW_RADIUS: &str = "8";
const DEFAULT_WORLD_RADIUS: &str = "4";
pub(crate) const DEFAULT_MAP_VIEW_INTERVAL_MS: u64 = 250;
pub(crate) const MAP_VIEW_PAN_STEP: i32 = 5;

fn main() {
    if let Err(err) = run() {
        eprintln!("client: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };

    match command.as_str() {
        "help" | "-h" | "--help" => print_help(),
        "health" => get_text("/health", args.collect())?,
        "world" => world(args.collect())?,
        "entities" => get_text("/entities", args.collect())?,
        "action-log" => get_text("/action-log", args.collect())?,
        "bot-stderr" => bot_stderr(args.collect())?,
        "upload-bot" => upload_bot(args.collect())?,
        "map-view" => map_view::map_view(args.collect())?,
        other => return Err(format!("unknown command `{other}`. Try `client help`.").into()),
    }

    Ok(())
}

fn print_help() {
    println!(
        "rand-game client\n\
\n\
Usage:\n\
  client <command> [options]\n\
\n\
Commands:\n\
  health                                      Print server health JSON\n\
  world [--x N] [--y N] [--radius N]         Print world region JSON\n\
  entities                                   Print entities JSON\n\
  action-log                                 Print action log JSON\n\
  bot-stderr [--player-id N]                 Stream bot stderr events\n\
  upload-bot [--player-id N] [--path P]      Upload a bot binary\n\
  map-view [--player-id N] [--map-id N]      Open an interactive ASCII map\n\
           [--x N] [--y N] [--radius N]\n\
\n\
Options:\n\
  --addr HOST:PORT                           Server address, default 127.0.0.1:3000\n\
  --player-id N                              Player id, default 1\n\
  --path P                                   Bot path, default target/debug/rand-game-binary\n\
  --once                                     Print map-view once and exit\n"
    );
}

fn get_text(path: &str, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = options::parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let response = http::get(&addr, path)?;
    let body = http::response_body(&response)?;
    println!("{body}");
    Ok(())
}

fn world(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = options::parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let x = options.x.unwrap_or_else(|| DEFAULT_MAP_VIEW_X.into());
    let y = options.y.unwrap_or_else(|| DEFAULT_MAP_VIEW_Y.into());
    let radius = options
        .radius
        .unwrap_or_else(|| DEFAULT_WORLD_RADIUS.into());
    let path = format!("/world?x={x}&y={y}&radius={radius}");
    let response = http::get(&addr, &path)?;
    let body = http::response_body(&response)?;
    println!("{body}");
    Ok(())
}

fn upload_bot(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = options::parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let player_id = options
        .player_id
        .unwrap_or_else(|| DEFAULT_PLAYER_ID.into());
    let bot_path = options.path.unwrap_or_else(|| DEFAULT_BOT_PATH.into());
    let body = fs::read(&bot_path)?;
    let request = format!(
        "POST /bots?player_id={player_id} HTTP/1.1\r\n\
Host: {addr}\r\n\
Content-Type: application/octet-stream\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n",
        body.len()
    );

    let mut stream = TcpStream::connect(&addr)?;
    stream.write_all(request.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let body = http::response_body(&response)?;
    println!("{body}");
    Ok(())
}

fn bot_stderr(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = options::parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let mut url = format!("ws://{addr}/bot-stderr");
    if let Some(player_id) = options.player_id {
        url.push_str("?player_id=");
        url.push_str(&player_id);
    }

    let (mut socket, _) = tungstenite::connect(url.as_str())?;
    loop {
        let message = socket.read()?;
        if message.is_close() {
            break;
        }
        if let Ok(text) = message.to_text() {
            println!("{text}");
        }
    }

    Ok(())
}
