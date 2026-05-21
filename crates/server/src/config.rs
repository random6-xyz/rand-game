use crate::state::ServerConfig;

pub fn parse_config() -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let mut config = ServerConfig {
        debug_max_actions: parse_env_u32("RAND_GAME_DEBUG_MAX_ACTIONS")?,
        log_bot_stderr: parse_env_bool("RAND_GAME_LOG_BOT_STDERR")?,
    };
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--debug-max-actions" => {
                let value = args.next().ok_or("missing value for --debug-max-actions")?;
                config.debug_max_actions = Some(value.parse()?);
            }
            "--log-bot-stderr" => {
                config.log_bot_stderr = true;
            }
            "--help" | "-h" => {
                println!(
                    "rand-game-server\n\nUsage:\n  rand-game-server [--debug-max-actions N] [--log-bot-stderr]\n\nEnvironment:\n  RAND_GAME_DEBUG_MAX_ACTIONS=N\n  RAND_GAME_LOG_BOT_STDERR=1"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown option `{other}`").into()),
        }
    }

    Ok(config)
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
