use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::thread;
use std::time::Duration;

const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_PLAYER_ID: &str = "1";
const DEFAULT_BOT_PATH: &str = "target/debug/rand-game-binary";
const DEFAULT_MAP_VIEW_X: &str = "0";
const DEFAULT_MAP_VIEW_Y: &str = "0";
const DEFAULT_MAP_VIEW_RADIUS: &str = "8";
const DEFAULT_WORLD_RADIUS: &str = "4";
const DEFAULT_MAP_VIEW_INTERVAL_MS: u64 = 250;
const MAP_VIEW_PAN_STEP: i32 = 5;

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
        "upload-bot" => upload_bot(args.collect())?,
        "map-view" => map_view(args.collect())?,
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
    let options = parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let response = get(&addr, path)?;
    let body = response_body(&response)?;
    println!("{body}");
    Ok(())
}

fn world(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let x = options.x.unwrap_or_else(|| DEFAULT_MAP_VIEW_X.into());
    let y = options.y.unwrap_or_else(|| DEFAULT_MAP_VIEW_Y.into());
    let radius = options
        .radius
        .unwrap_or_else(|| DEFAULT_WORLD_RADIUS.into());
    let path = format!("/world?x={x}&y={y}&radius={radius}");
    let response = get(&addr, &path)?;
    let body = response_body(&response)?;
    println!("{body}");
    Ok(())
}

fn upload_bot(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
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
    let body = response_body(&response)?;
    println!("{body}");
    Ok(())
}

fn map_view(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
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
        print!("{body}");
        return Ok(());
    }

    let mut terminal = TerminalMode::enter()?;
    print!("\x1b[?25l");

    loop {
        match fetch_map_view(&addr, &player_id, map_id.as_deref(), x, y, &radius) {
            Ok(body) => {
                write_raw_terminal_frame(&format!(
                    "\x1b[2J\x1b[H{body}\ncontrols: w/a/s/d move viewport by {MAP_VIEW_PAN_STEP}, q quit | center=({x}, {y})\n"
                ))?;
            }
            Err(err) => {
                write_raw_terminal_frame(&format!(
                    "\x1b[2J\x1b[Hmap-view request failed: {err}\n\ncontrols: w/a/s/d move viewport by {MAP_VIEW_PAN_STEP}, q quit | center=({x}, {y})\n"
                ))?;
            }
        }
        std::io::stdout().flush()?;

        if terminal.handle_input(&mut x, &mut y)? {
            break;
        }
        thread::sleep(Duration::from_millis(DEFAULT_MAP_VIEW_INTERVAL_MS));
    }

    print!("\x1b[?25h\x1b[2J\x1b[H");
    std::io::stdout().flush()?;
    Ok(())
}

fn write_raw_terminal_frame(frame: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::io::stdout().write_all(frame.replace('\n', "\r\n").as_bytes())?;
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

    let response = get(addr, &path)?;
    Ok(response_body(&response)?.to_string())
}

struct TerminalMode {
    saved: String,
    tty: File,
}

impl TerminalMode {
    fn enter() -> Result<Self, Box<dyn std::error::Error>> {
        let tty = File::options().read(true).write(true).open("/dev/tty")?;
        let saved = Command::new("stty")
            .arg("-g")
            .stdin(tty.try_clone()?)
            .output()?;
        if !saved.status.success() {
            return Err("failed to read terminal mode with stty".into());
        }
        let saved = String::from_utf8(saved.stdout)?.trim().to_string();

        let status = Command::new("stty")
            .args(["raw", "-echo", "min", "0", "time", "0"])
            .stdin(tty.try_clone()?)
            .status()?;
        if !status.success() {
            return Err("failed to enter raw terminal mode with stty".into());
        }

        Ok(Self { saved, tty })
    }

    fn handle_input(
        &mut self,
        x: &mut i32,
        y: &mut i32,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let mut buffer = [0_u8; 64];
        let read = self.tty.read(&mut buffer)?;

        for byte in &buffer[..read] {
            match byte.to_ascii_lowercase() {
                b'w' => *y += MAP_VIEW_PAN_STEP,
                b'a' => *x -= MAP_VIEW_PAN_STEP,
                b's' => *y -= MAP_VIEW_PAN_STEP,
                b'd' => *x += MAP_VIEW_PAN_STEP,
                b'q' => return Ok(true),
                _ => {}
            }
        }

        Ok(false)
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        let _ = Command::new("stty")
            .arg(&self.saved)
            .stdin(
                self.tty
                    .try_clone()
                    .ok()
                    .map_or(std::process::Stdio::null(), std::process::Stdio::from),
            )
            .status();
        let _ = std::io::stdout().write_all(b"\x1b[?25h");
        let _ = std::io::stdout().flush();
    }
}

fn get(addr: &str, path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
Host: {addr}\r\n\
Connection: close\r\n\r\n"
    );
    http_request(addr, &request)
}

fn response_body(response: &str) -> Result<&str, Box<dyn std::error::Error>> {
    let status_line = response.lines().next().unwrap_or("<empty response>");
    if !status_line.contains(" 200 ") {
        return Err(format!("request failed: {status_line}").into());
    }

    response
        .split("\r\n\r\n")
        .nth(1)
        .ok_or_else(|| "response has no body".into())
}

fn http_request(addr: &str, request: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(addr)?;
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

#[derive(Debug, Default)]
struct Options {
    addr: Option<String>,
    player_id: Option<String>,
    map_id: Option<String>,
    x: Option<String>,
    y: Option<String>,
    radius: Option<String>,
    path: Option<String>,
    once: bool,
}

fn parse_options(args: Vec<String>) -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--addr" => options.addr = Some(required_value(&arg, iter.next())?),
            "--player-id" => options.player_id = Some(required_value(&arg, iter.next())?),
            "--map-id" => options.map_id = Some(required_value(&arg, iter.next())?),
            "--x" => options.x = Some(required_value(&arg, iter.next())?),
            "--y" => options.y = Some(required_value(&arg, iter.next())?),
            "--radius" => options.radius = Some(required_value(&arg, iter.next())?),
            "--path" => options.path = Some(required_value(&arg, iter.next())?),
            "--once" => options.once = true,
            other => return Err(format!("unknown option `{other}`").into()),
        }
    }

    Ok(options)
}

fn required_value(flag: &str, value: Option<String>) -> Result<String, Box<dyn std::error::Error>> {
    value.ok_or_else(|| format!("missing value for {flag}").into())
}

fn parse_i32_option(
    value: Option<&str>,
    default: &str,
    flag: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    value
        .unwrap_or(default)
        .parse::<i32>()
        .map_err(|err| format!("invalid {flag}: {err}").into())
}
