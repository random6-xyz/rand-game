use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use rand_game_common::fb::{game_output_buffer_has_identifier, root_as_game_output};
use rand_game_common::framing::{FrameKind, decode_frame};
use wait_timeout::ChildExt;

pub const DEFAULT_BOT_PATH: &str = "target/debug/rand-game-binary";
const BOT_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_STDOUT_BYTES: usize = 64 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub struct BotRunResult {
    pub output_payload: Vec<u8>,
    pub stderr: String,
}

pub fn run_bot(
    path: &Path,
    input_frame: &[u8],
) -> Result<BotRunResult, Box<dyn std::error::Error>> {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to spawn bot {}: {err}", path.display()))?;

    child
        .stdin
        .take()
        .ok_or("bot stdin unavailable")?
        .write_all(input_frame)?;

    let status = match child.wait_timeout(BOT_TIMEOUT)? {
        Some(status) => status,
        None => {
            child.kill()?;
            child.wait()?;
            return Err(format!("bot timed out after {}ms", BOT_TIMEOUT.as_millis()).into());
        }
    };

    let stdout = read_limited(child.stdout.take(), MAX_STDOUT_BYTES, "stdout")?;
    let stderr = read_limited(child.stderr.take(), MAX_STDERR_BYTES, "stderr")?;
    let stderr = String::from_utf8_lossy(&stderr).into_owned();

    if !status.success() {
        return Err(format!("bot exited with {status}; stderr: {stderr}").into());
    }

    let output_payload = decode_frame(&stdout, FrameKind::GameOutput)?.to_vec();
    if !game_output_buffer_has_identifier(&output_payload) {
        return Err("bot output payload is not a BWO1 GameOutput flatbuffer".into());
    }
    root_as_game_output(&output_payload)?;

    Ok(BotRunResult {
        output_payload,
        stderr,
    })
}

fn read_limited<R: Read>(
    reader: Option<R>,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let Some(mut reader) = reader else {
        return Ok(Vec::new());
    };
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(format!("bot {label} exceeded {limit} bytes").into());
    }
    Ok(bytes)
}

#[allow(dead_code)]
fn _path_buf_for_docs(path: &str) -> PathBuf {
    PathBuf::from(path)
}
