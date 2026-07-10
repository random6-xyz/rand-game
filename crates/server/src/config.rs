use std::path::{Path, PathBuf};

use rand_game_common::rules::default_rule_catalog;

use crate::rules::{ServerEnv, ServerRules};
use crate::state::ServerConfig;

const DEFAULT_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_ENV_PATH: &str = "config/server.env.toml";
const DEFAULT_RULES_PATH: &str = "config/server.rules.toml";

pub fn parse_config() -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let mut env_path = PathBuf::from(DEFAULT_ENV_PATH);
    let mut rules_path = PathBuf::from(DEFAULT_RULES_PATH);
    let mut config = ServerConfig {
        addr: DEFAULT_ADDR.into(),
        debug_max_actions: parse_env_u32("RAND_GAME_DEBUG_MAX_ACTIONS")?,
        log_bot_stderr: parse_env_bool("RAND_GAME_LOG_BOT_STDERR")?,
        env: ServerEnv::default(),
        rules: ServerRules::default(),
        rule_catalog: default_rule_catalog(),
    };
    let mut enable_bot_upload = false;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--addr" => {
                config.addr = args.next().ok_or("missing value for --addr")?;
            }
            "--debug-max-actions" => {
                let value = args.next().ok_or("missing value for --debug-max-actions")?;
                config.debug_max_actions = Some(value.parse()?);
            }
            "--log-bot-stderr" => {
                config.log_bot_stderr = true;
            }
            "--enable-bot-upload" => {
                enable_bot_upload = true;
            }
            "--env-path" => {
                env_path = PathBuf::from(args.next().ok_or("missing value for --env-path")?);
            }
            "--rules-path" => {
                rules_path = PathBuf::from(args.next().ok_or("missing value for --rules-path")?);
            }
            "--help" | "-h" => {
                println!(
                    "rand-game-server\n\nUsage:\n  rand-game-server [--addr HOST:PORT] [--env-path P] [--rules-path P] [--debug-max-actions N] [--log-bot-stderr] [--enable-bot-upload]\n\nEnvironment:\n  RAND_GAME_DEBUG_MAX_ACTIONS=N\n  RAND_GAME_LOG_BOT_STDERR=1"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown option `{other}`").into()),
        }
    }

    config.env = load_toml_or_default(&env_path)?;
    config.rules = load_toml_or_default(&rules_path)?;
    if enable_bot_upload {
        config.rules.enable_bot_upload = true;
    }

    Ok(config)
}

fn load_toml_or_default<T>(path: &Path) -> Result<T, Box<dyn std::error::Error>>
where
    T: Default + serde::de::DeserializeOwned,
{
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(err) => Err(err.into()),
    }
}

fn parse_env_u32(name: &str) -> Result<Option<u32>, Box<dyn std::error::Error>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value.parse()?)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn parse_env_bool(name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match std::env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON" => Ok(true),
            "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF" => Ok(false),
            _ => Err(format!("{name} must be a boolean value").into()),
        },
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(err) => Err(err.into()),
    }
}
