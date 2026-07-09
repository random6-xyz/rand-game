use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_BOT_PATH: &str = "target/debug/rand-game-binary";
const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_PLAYER_ID: &str = "1";
const DEFAULT_MAP_VIEW_X: &str = "0";
const DEFAULT_MAP_VIEW_Y: &str = "0";
const DEFAULT_MAP_VIEW_RADIUS: &str = "8";
const E2E_SERVER_ADDR: &str = "127.0.0.1:3100";
const E2E_RULES_PATH: &str = "target/e2e/server.rules.toml";
const E2E_RECIPES_RULES_PATH: &str = "target/e2e/server-recipes.rules.toml";

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
        "validate" => validate()?,
        "gen-examples" => gen_examples(args.collect())?,
        "server" => server(args.collect())?,
        "server-debug" => server_debug()?,
        "upload-bot" => upload_bot(args.collect())?,
        "map-view" => map_view(args.collect())?,
        "user-debug" => user_debug()?,
        "e2e-debug" => e2e_debug()?,
        "e2e-debug-recipes" => e2e_debug_recipes()?,
        "clean-state" => clean_state()?,
        other => {
            return Err(format!("unknown command `{other}`. Try `cargo xtask help`. ").into());
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        r#"rand-game xtask

Usage:
  cargo xtask <command> [options]

Commands:
  build                       Build server, common, binary, and client crates
  validate                    Run cargo fmt --check, cargo check, cargo test, cargo clippy
  gen-examples [--out-dir P]   Generate framed FlatBuffers example files
  server [--addr HOST:PORT] [--env-path P] [--rules-path P] [--debug-max-actions N]
                              Run rand-game-server
  server-debug                Clean local state and run server with debug action limit
  upload-bot [--player-id N] [--path P] [--addr HOST:PORT]
                              Upload a bot binary to the running server
  map-view [--player-id N] [--map-id N] [--x N] [--y N] [--radius N] [--addr HOST:PORT]
                              Print an ASCII map from the running server
  user-debug                  Upload default bot and run client map-view
  e2e-debug                   Run server, upload bot, and verify world changes
  e2e-debug-recipes           Run server with verify-recipes bot and verify all recipes
  clean-state                 Delete var/server and var/bots
"#
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

fn server(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options(args)?;
    let mut cargo_args = vec!["run", "-p", "rand-game-server"];
    let mut has_server_args = false;

    if let Some(env_path) = options.env_path.as_deref() {
        push_server_arg(&mut cargo_args, &mut has_server_args, "--env-path");
        cargo_args.push(env_path);
    }
    if let Some(addr) = options.addr.as_deref() {
        push_server_arg(&mut cargo_args, &mut has_server_args, "--addr");
        cargo_args.push(addr);
    }
    if let Some(rules_path) = options.rules_path.as_deref() {
        push_server_arg(&mut cargo_args, &mut has_server_args, "--rules-path");
        cargo_args.push(rules_path);
    }

    if let Some(debug_max_actions) = options.debug_max_actions.as_deref() {
        push_server_arg(&mut cargo_args, &mut has_server_args, "--debug-max-actions");
        cargo_args.push(debug_max_actions);
    }
    if options.log_bot_stderr {
        push_server_arg(&mut cargo_args, &mut has_server_args, "--log-bot-stderr");
    }

    cargo(&cargo_args)
}

fn push_server_arg<'a>(args: &mut Vec<&'a str>, has_server_args: &mut bool, value: &'a str) {
    if !*has_server_args {
        args.push("--");
        *has_server_args = true;
    }
    args.push(value);
}

fn server_debug() -> Result<(), Box<dyn std::error::Error>> {
    clean_state()?;
    server(vec!["--debug-max-actions".into(), "1000".into()])
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

fn user_debug() -> Result<(), Box<dyn std::error::Error>> {
    upload_bot(Vec::new())?;
    cargo(&[
        "run",
        "-p",
        "client",
        "--",
        "map-view",
        "--player-id",
        "1",
        "--x",
        "0",
        "--y",
        "0",
    ])
}

fn e2e_debug_recipes() -> Result<(), Box<dyn std::error::Error>> {
    clean_state()?;
    build_bot()?;
    write_e2e_recipes_rules()?;

    let mut server = spawn_e2e_recipes_server()?;
    wait_for_server(E2E_SERVER_ADDR, &mut server)?;

    upload_bot(vec!["--addr".into(), E2E_SERVER_ADDR.into()])?;
    let initial_health = get_body(E2E_SERVER_ADDR, "/health")?;

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_health = initial_health.clone();
    let mut last_entities = String::new();
    let mut last_action_log = String::new();

    while Instant::now() < deadline {
        server.ensure_running()?;
        thread::sleep(Duration::from_millis(100));

        last_health = get_body(E2E_SERVER_ADDR, "/health")?;
        last_entities = get_body(E2E_SERVER_ADDR, "/entities")?;
        last_action_log = get_body(E2E_SERVER_ADDR, "/action-log")?;

        let crafted_recipes = count_distinct_crafted_recipes(&last_action_log);
        if crafted_recipes >= 5 {
            println!("e2e-debug-recipes passed");
            println!("verified recipes: {crafted_recipes}/5");
            println!("health: {last_health}");
            return Ok(());
        }
    }

    Err(format!(
        "e2e-debug-recipes timed out (30s)\ninitial health: {initial_health}\nlast health: {last_health}\nlast entities: {last_entities}\nlast action-log: {last_action_log}"
    )
    .into())
}

fn write_e2e_recipes_rules() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(E2E_RECIPES_RULES_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        r#"tick_interval_ms = 50
observation_radius = 64

[basic_core]
run_interval_ticks = 1
cpu_time_ms = 50
wall_time_ms = 250
memory_bytes = 67108864
stdout_bytes = 65536
stderr_bytes = 65536
max_actions = 2000
max_persistent_memory_bytes = 4096
"#,
    )?;
    Ok(())
}

fn spawn_e2e_recipes_server() -> Result<ServerProcess, Box<dyn std::error::Error>> {
    let cargo = cargo_bin();
    println!(
        "$ {cargo} run -p rand-game-server -- --addr {E2E_SERVER_ADDR} --rules-path {E2E_RECIPES_RULES_PATH} --debug-max-actions 2000 --log-bot-stderr"
    );
    let child = Command::new(cargo)
        .args([
            "run",
            "-p",
            "rand-game-server",
            "--",
            "--addr",
            E2E_SERVER_ADDR,
            "--rules-path",
            E2E_RECIPES_RULES_PATH,
            "--debug-max-actions",
            "2000",
            "--log-bot-stderr",
        ])
        .env("RAND_GAME_VERIFY_RECIPES", "1")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    Ok(ServerProcess { child })
}

fn count_distinct_crafted_recipes(action_log: &str) -> usize {
    let mut recipes = std::collections::HashSet::new();
    let all_ids = [
        "iron-plate",
        "copper-plate",
        "iron-gear",
        "iron-rod",
        "copper-wire",
        "basic-circuit",
        "conveyor-belt",
    ];
    for id in &all_ids {
        if action_log.contains(&format!("\"recipe_id\":\"{id}\"")) {
            recipes.insert(id);
        }
    }
    recipes.len()
}

fn e2e_debug() -> Result<(), Box<dyn std::error::Error>> {
    clean_state()?;
    build_bot()?;
    write_e2e_rules()?;

    let mut server = spawn_e2e_server()?;
    wait_for_server(E2E_SERVER_ADDR, &mut server)?;

    upload_bot(vec!["--addr".into(), E2E_SERVER_ADDR.into()])?;
    let initial_health = get_body(E2E_SERVER_ADDR, "/health")?;

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_health = initial_health.clone();
    let mut last_entities = String::new();
    let mut last_action_log = String::new();
    let mut last_map_view = String::new();

    while Instant::now() < deadline {
        server.ensure_running()?;
        thread::sleep(Duration::from_millis(100));

        last_health = get_body(E2E_SERVER_ADDR, "/health")?;
        last_entities = get_body(E2E_SERVER_ADDR, "/entities")?;
        last_action_log = get_body(E2E_SERVER_ADDR, "/action-log")?;
        last_map_view = get_body(E2E_SERVER_ADDR, "/map-view?player_id=1&x=0&y=0&radius=8")?;

        if health_has_entries(&last_health)
            && entities_have_cargo(&last_entities)
            && action_log_has_bot_action(&last_action_log)
            && map_view_is_valid(&last_map_view)
        {
            println!("e2e-debug passed");
            println!("health: {last_health}");
            return Ok(());
        }
    }

    Err(format!(
        "e2e-debug timed out\ninitial health: {initial_health}\nlast health: {last_health}\nlast entities: {last_entities}\nlast action-log: {last_action_log}\nlast map-view:\n{last_map_view}"
    )
    .into())
}

fn write_e2e_rules() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(E2E_RULES_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        r#"tick_interval_ms = 50
observation_radius = 8

[basic_core]
run_interval_ticks = 1
cpu_time_ms = 50
wall_time_ms = 250
memory_bytes = 67108864
stdout_bytes = 65536
stderr_bytes = 65536
max_actions = 1000
max_persistent_memory_bytes = 4096
"#,
    )?;
    Ok(())
}

fn spawn_e2e_server() -> Result<ServerProcess, Box<dyn std::error::Error>> {
    let cargo = cargo_bin();
    println!(
        "$ {cargo} run -p rand-game-server -- --addr {E2E_SERVER_ADDR} --rules-path {E2E_RULES_PATH} --debug-max-actions 1000"
    );
    let child = Command::new(cargo)
        .args([
            "run",
            "-p",
            "rand-game-server",
            "--",
            "--addr",
            E2E_SERVER_ADDR,
            "--rules-path",
            E2E_RULES_PATH,
            "--debug-max-actions",
            "1000",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    Ok(ServerProcess { child })
}

fn wait_for_server(
    addr: &str,
    server: &mut ServerProcess,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        server.ensure_running()?;
        if get_body(addr, "/health").is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("server did not become ready at {addr}").into())
}

fn get_body(addr: &str, path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
Host: {addr}\r\n\
Connection: close\r\n\r\n"
    );
    let response = http_request(addr, &request)?;
    let status_line = response.lines().next().unwrap_or("<empty response>");
    if !status_line.contains(" 200 ") {
        return Err(format!("GET {path} failed: {status_line}").into());
    }
    Ok(response.split("\r\n\r\n").nth(1).unwrap_or_default().into())
}

fn health_has_entries(health: &str) -> bool {
    json_number_after(health, "\"action_log_entries\":").is_some_and(|entries| entries > 0)
}

fn entities_have_cargo(entities: &str) -> bool {
    let mut rest = entities;
    while let Some(index) = rest.find("\"amount\":") {
        rest = &rest[index + "\"amount\":".len()..];
        if json_leading_number(rest).is_some_and(|amount| amount > 0) {
            return true;
        }
    }
    false
}

fn action_log_has_bot_action(action_log: &str) -> bool {
    action_log.contains("\"Mine\"") || action_log.contains("mined")
}

fn map_view_is_valid(map_view: &str) -> bool {
    map_view.contains("tick=") && map_view.contains("reveal_all=true") && map_view.contains('E')
}

fn json_number_after(input: &str, marker: &str) -> Option<u64> {
    let (_, rest) = input.split_once(marker)?;
    json_leading_number(rest)
}

fn json_leading_number(input: &str) -> Option<u64> {
    let digits = input
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

struct ServerProcess {
    child: Child,
}

impl ServerProcess {
    fn ensure_running(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(status) = self.child.try_wait()? {
            return Err(format!("server exited early with status {status}").into());
        }
        Ok(())
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
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
    let cargo = cargo_bin();
    println!("$ {cargo} {}", args.join(" "));

    let status = Command::new(cargo).args(args).status()?;
    if !status.success() {
        return Err(format!("cargo command failed with status {status}").into());
    }

    Ok(())
}

fn cargo_bin() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

#[derive(Debug, Default)]
struct Options {
    out_dir: Option<String>,
    player_id: Option<String>,
    map_id: Option<String>,
    x: Option<String>,
    y: Option<String>,
    radius: Option<String>,
    path: Option<String>,
    addr: Option<String>,
    debug_max_actions: Option<String>,
    env_path: Option<String>,
    rules_path: Option<String>,
    log_bot_stderr: bool,
}

fn parse_options(args: Vec<String>) -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--out-dir" => options.out_dir = Some(required_value(&arg, iter.next())?),
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
            "--env-path" => options.env_path = Some(required_value(&arg, iter.next())?),
            "--rules-path" => options.rules_path = Some(required_value(&arg, iter.next())?),
            "--log-bot-stderr" => options.log_bot_stderr = true,
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
