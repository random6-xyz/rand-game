use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_EXAMPLE_INPUT: &str = "target/flatbuffers_examples/game_input_example.bwi";
const DEFAULT_BOT_OUTPUT: &str = "/tmp/game_output.bwo";
const DEFAULT_BOT_PATH: &str = "target/debug/rand-game-binary";
const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_PLAYER_ID: &str = "1";
const DEFAULT_MAP_VIEW_X: &str = "0";
const DEFAULT_MAP_VIEW_Y: &str = "0";
const DEFAULT_MAP_VIEW_RADIUS: &str = "8";

fn main() {
    if let Err(err) = run() {
        eprintln!("xtask: {err}");
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
        "build" => build_all()?,
        "fmt" => cargo(&["fmt"])?,
        "check" => cargo(&["check"])?,
        "test" => test_loop(args.collect())?,
        "validate" => validate()?,
        "gen-examples" => gen_examples(args.collect())?,
        "build-bot" => build_bot()?,
        "run-bot" => run_bot(args.collect())?,
        "server" => server(args.collect())?,
        "upload-bot" => upload_bot(args.collect())?,
        "map-view" => map_view(args.collect())?,
        "clean-state" => clean_state()?,
        other => {
            return Err(format!("unknown command `{other}`. Try `cargo xtask help`. ").into());
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        "rand-game xtask\n\
\n\
Usage:\n\
  cargo xtask <command> [options]\n\
\n\
Commands:\n\
  build                       Build server, common, binary, and client crates\n\
  fmt                         Run cargo fmt\n\
  check                       Run cargo check\n\
  test [map-view options]     Build bot, run server, upload bot, then print map-view every second\n\
  validate                    Run cargo fmt --check, cargo check, cargo test, cargo clippy\n\
  gen-examples [--out-dir P]   Generate framed FlatBuffers example files\n\
  build-bot                   Build rand-game-binary\n\
  run-bot [--input P] [--output P]\n\
                              Build bot, generate examples, run sample bot\n\
  server [--debug-max-actions N]\n\
                              Run rand-game-server\n\
  upload-bot [--player-id N] [--path P] [--addr HOST:PORT]\n\
                              Upload a bot binary to the running server\n\
  map-view [--player-id N] [--map-id N] [--x N] [--y N] [--radius N] [--addr HOST:PORT]\n\
                              Print an ASCII map from the running server\n\
  clean-state                 Delete var/server and var/bots\n"
    );
}

fn validate() -> Result<(), Box<dyn std::error::Error>> {
    cargo(&["fmt", "--check"])?;
    cargo(&["check"])?;
    cargo(&["test"])?;
    cargo(&["clippy", "--all-targets", "--", "-D", "warnings"])?;
    Ok(())
}

fn build_all() -> Result<(), Box<dyn std::error::Error>> {
    cargo(&[
        "build",
        "-p",
        "rand-game-server",
        "-p",
        "rand-game-common",
        "-p",
        "rand-game-binary",
        "-p",
        "client",
    ])
}

fn gen_examples(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let mut cargo_args = vec!["run", "-p", "rand-game-common", "--example", "gen_example"];

    if let Some(out_dir) = options.out_dir.as_deref() {
        cargo_args.extend(["--", "--out-dir", out_dir]);
    }

    cargo(&cargo_args)
}

fn build_bot() -> Result<(), Box<dyn std::error::Error>> {
    cargo(&["build", "-p", "rand-game-binary"])
}

fn run_bot(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let input = options
        .input
        .unwrap_or_else(|| DEFAULT_EXAMPLE_INPUT.into());
    let output = options.output.unwrap_or_else(|| DEFAULT_BOT_OUTPUT.into());

    build_bot()?;
    gen_examples(Vec::new())?;

    if let Some(parent) = Path::new(&output).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let input_file = fs::File::open(&input)?;
    let output_file = fs::File::create(&output)?;
    let status = Command::new(DEFAULT_BOT_PATH)
        .stdin(Stdio::from(input_file))
        .stdout(Stdio::from(output_file))
        .status()?;

    if !status.success() {
        return Err(format!("sample bot failed with status {status}").into());
    }

    println!("wrote {output}");
    Ok(())
}

fn server(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let mut cargo_args = vec!["run", "-p", "rand-game-server"];

    if let Some(debug_max_actions) = options.debug_max_actions.as_deref() {
        cargo_args.extend(["--", "--debug-max-actions", debug_max_actions]);
    }

    cargo(&cargo_args)
}

fn test_loop(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args.clone())?;
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());

    build_bot()?;
    let mut server = spawn_server(options.debug_max_actions.as_deref())?;

    if let Err(err) = wait_for_server(&addr, Duration::from_secs(10)) {
        stop_server(&mut server);
        return Err(err);
    }

    if let Err(err) = upload_bot(args.clone()) {
        stop_server(&mut server);
        return Err(err);
    }

    loop {
        if let Some(status) = server.try_wait()? {
            return Err(format!("server exited with status {status}").into());
        }

        if let Err(err) = map_view(args.clone()) {
            eprintln!("map-view failed: {err}");
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn spawn_server(debug_max_actions: Option<&str>) -> Result<Child, Box<dyn std::error::Error>> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut args = vec!["run", "-p", "rand-game-server"];

    if let Some(debug_max_actions) = debug_max_actions {
        args.extend(["--", "--debug-max-actions", debug_max_actions]);
    }

    println!("$ {cargo} {}", args.join(" "));
    Ok(Command::new(cargo).args(args).spawn()?)
}

fn wait_for_server(addr: &str, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let request = format!(
        "GET /health HTTP/1.1\r\n\
Host: {addr}\r\n\
Connection: close\r\n\r\n"
    );
    let started_at = Instant::now();

    while started_at.elapsed() < timeout {
        if let Ok(response) = http_request(addr, &request)
            && response
                .lines()
                .next()
                .is_some_and(|status| status.contains(" 200 "))
        {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(200));
    }

    Err(format!("server did not become ready at {addr}").into())
}

fn stop_server(server: &mut Child) {
    if let Err(err) = server.kill() {
        eprintln!("failed to stop server: {err}");
    }
    if let Err(err) = server.wait() {
        eprintln!("failed to wait for server: {err}");
    }
}

fn upload_bot(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let player_id = options
        .player_id
        .unwrap_or_else(|| DEFAULT_PLAYER_ID.into());
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let bot_path = options.path.unwrap_or_else(|| DEFAULT_BOT_PATH.into());

    if bot_path == DEFAULT_BOT_PATH && !Path::new(DEFAULT_BOT_PATH).exists() {
        build_bot()?;
    }

    let body = fs::read(&bot_path)?;
    let mut stream = TcpStream::connect(&addr)?;
    let request = format!(
        "POST /bots?player_id={player_id} HTTP/1.1\r\n\
Host: {addr}\r\n\
Content-Type: application/octet-stream\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(request.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let status_line = response.lines().next().unwrap_or("<empty response>");
    println!("{status_line}");

    if !status_line.contains(" 200 ") {
        return Err(format!("upload failed: {status_line}").into());
    }

    if let Some(body) = response.split("\r\n\r\n").nth(1) {
        println!("{body}");
    }

    Ok(())
}

fn map_view(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let player_id = options
        .player_id
        .unwrap_or_else(|| DEFAULT_PLAYER_ID.into());
    let addr = options.addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.into());
    let x = options.x.unwrap_or_else(|| DEFAULT_MAP_VIEW_X.into());
    let y = options.y.unwrap_or_else(|| DEFAULT_MAP_VIEW_Y.into());
    let radius = options
        .radius
        .unwrap_or_else(|| DEFAULT_MAP_VIEW_RADIUS.into());

    let mut path = format!("/map-view?player_id={player_id}&x={x}&y={y}&radius={radius}");
    if let Some(map_id) = options.map_id {
        path.push_str("&map_id=");
        path.push_str(&map_id);
    }

    let request = format!(
        "GET {path} HTTP/1.1\r\n\
Host: {addr}\r\n\
Connection: close\r\n\r\n"
    );
    let response = http_request(&addr, &request)?;
    let status_line = response.lines().next().unwrap_or("<empty response>");
    if !status_line.contains(" 200 ") {
        return Err(format!("map-view failed: {status_line}").into());
    }

    if let Some(body) = response.split("\r\n\r\n").nth(1) {
        print!("{body}");
    }

    Ok(())
}

fn clean_state() -> Result<(), Box<dyn std::error::Error>> {
    remove_dir_if_exists(PathBuf::from("var/server"))?;
    remove_dir_if_exists(PathBuf::from("var/bots"))?;
    Ok(())
}

fn remove_dir_if_exists(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    match fs::remove_dir_all(&path) {
        Ok(()) => println!("removed {}", path.display()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            println!("already clean: {}", path.display());
        }
        Err(err) => return Err(err.into()),
    }

    Ok(())
}

fn cargo(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    println!("$ {cargo} {}", args.join(" "));

    let status = Command::new(cargo).args(args).status()?;
    if !status.success() {
        return Err(format!("cargo command failed with status {status}").into());
    }

    Ok(())
}

#[derive(Debug, Default)]
struct Options {
    out_dir: Option<String>,
    input: Option<String>,
    output: Option<String>,
    player_id: Option<String>,
    map_id: Option<String>,
    x: Option<String>,
    y: Option<String>,
    radius: Option<String>,
    path: Option<String>,
    addr: Option<String>,
    debug_max_actions: Option<String>,
}

fn parse_options(args: Vec<String>) -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--out-dir" => options.out_dir = Some(required_value(&arg, iter.next())?),
            "--input" => options.input = Some(required_value(&arg, iter.next())?),
            "--output" => options.output = Some(required_value(&arg, iter.next())?),
            "--player-id" => options.player_id = Some(required_value(&arg, iter.next())?),
            "--map-id" => options.map_id = Some(required_value(&arg, iter.next())?),
            "--x" => options.x = Some(required_value(&arg, iter.next())?),
            "--y" => options.y = Some(required_value(&arg, iter.next())?),
            "--radius" => options.radius = Some(required_value(&arg, iter.next())?),
            "--path" => options.path = Some(required_value(&arg, iter.next())?),
            "--addr" => options.addr = Some(required_value(&arg, iter.next())?),
            "--debug-max-actions" => {
                options.debug_max_actions = Some(required_value(&arg, iter.next())?)
            }
            other => return Err(format!("unknown option `{other}`").into()),
        }
    }

    Ok(options)
}

fn required_value(flag: &str, value: Option<String>) -> Result<String, Box<dyn std::error::Error>> {
    value.ok_or_else(|| format!("missing value for {flag}").into())
}

fn http_request(addr: &str, request: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(addr)?;
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}
