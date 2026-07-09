use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use rand_game_common::fb::{game_output_buffer_has_identifier, root_as_game_output};
use rand_game_common::framing::{FrameKind, decode_frame};
use wait_timeout::ChildExt;

use crate::model::RuntimeProfile;

pub const DEFAULT_BOT_PATH: &str = "target/debug/rand-game-binary";

#[derive(Debug)]
pub struct BotRunResult {
    pub output_payload: Vec<u8>,
    pub stderr: String,
}

static NSJAIL_WARNED: OnceLock<()> = OnceLock::new();

pub fn run_bot(
    path: &Path,
    input_frame: &[u8],
    profile: &RuntimeProfile,
) -> Result<BotRunResult, Box<dyn std::error::Error>> {
    if nsjail_available() {
        match run_bot_with_nsjail(path, input_frame, profile) {
            Ok(result) => return Ok(result),
            Err(err) => {
                if NSJAIL_WARNED.set(()).is_ok() {
                    eprintln!("nsjail runner unavailable, using fallback: {err}");
                }
            }
        }
    } else if NSJAIL_WARNED.set(()).is_ok() {
        eprintln!("nsjail not found, using fallback for bot execution");
    }
    run_bot_fallback(path, input_frame, profile)
}

fn nsjail_available() -> bool {
    std::process::Command::new("nsjail")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_bot_with_nsjail(
    path: &Path,
    input_frame: &[u8],
    profile: &RuntimeProfile,
) -> Result<BotRunResult, Box<dyn std::error::Error>> {
    let wall_secs = (profile.wall_time_ms as u64).div_ceil(1000).max(1);
    let cpu_secs = (profile.cpu_time_ms as u64).div_ceil(1000).max(1);
    let mem_mb = ((profile.memory_bytes as u64) / (1024 * 1024)).max(1);
    let cpu_ms_per_sec = if profile.wall_time_ms > 0 {
        ((profile.cpu_time_ms as u64) * 1000 / (profile.wall_time_ms as u64)).min(1000)
    } else {
        100
    };

    let seccomp_policy = build_seccomp_policy();

    let mut child = Command::new("nsjail")
        .arg("--mode")
        .arg("o")
        .arg("--really_quiet")
        .arg("--time_limit")
        .arg(wall_secs.to_string())
        .arg("--rlimit_cpu")
        .arg(cpu_secs.to_string())
        .arg("--rlimit_as")
        .arg(mem_mb.to_string())
        .arg("--rlimit_fsize")
        .arg("1")
        .arg("--rlimit_nofile")
        .arg("32")
        .arg("--chroot")
        .arg("/")
        .arg("--disable_proc")
        .arg("--user")
        .arg("65534")
        .arg("--group")
        .arg("65534")
        .arg("--use_cgroupv2")
        .arg("--cgroup_mem_max")
        .arg((profile.memory_bytes as u64).to_string())
        .arg("--cgroup_cpu_ms_per_sec")
        .arg(cpu_ms_per_sec.to_string())
        .arg("--seccomp_string")
        .arg(seccomp_policy)
        .arg("--")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to spawn nsjail: {err}"))?;

    child
        .stdin
        .take()
        .ok_or("nsjail stdin unavailable")?
        .write_all(input_frame)?;

    let wall_timeout = Duration::from_millis(profile.wall_time_ms as u64 + 2000);
    let status = match child.wait_timeout(wall_timeout)? {
        Some(status) => status,
        None => {
            child.kill()?;
            child.wait()?;
            return Err(format!("nsjail timed out after {}ms", wall_timeout.as_millis()).into());
        }
    };

    let stdout = read_limited(child.stdout.take(), profile.stdout_bytes as usize, "stdout")?;
    let stderr_bytes = read_limited(child.stderr.take(), profile.stderr_bytes as usize, "stderr")?;
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();

    if !status.success() {
        return Err(format!("nsjail exited with {status}; stderr: {stderr}").into());
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

fn run_bot_fallback(
    path: &Path,
    input_frame: &[u8],
    profile: &RuntimeProfile,
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

    let timeout = Duration::from_millis(profile.wall_time_ms as u64);
    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            child.kill()?;
            child.wait()?;
            return Err(format!("bot timed out after {}ms", timeout.as_millis()).into());
        }
    };

    let stdout = read_limited(child.stdout.take(), profile.stdout_bytes as usize, "stdout")?;
    let stderr_bytes = read_limited(child.stderr.take(), profile.stderr_bytes as usize, "stderr")?;
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();

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

fn build_seccomp_policy() -> String {
    [
        "POLICY bot {",
        "    KILL {",
        "        ptrace,",
        "        process_vm_readv,",
        "        process_vm_writev,",
        "        kcmp,",
        "    }",
        "}",
        "USE bot",
        "ALLOW {",
        "    read,",
        "    write,",
        "    close,",
        "    exit,",
        "    exit_group,",
        "    brk,",
        "    mmap,",
        "    munmap,",
        "    mprotect,",
        "    mremap,",
        "    madvise,",
        "    futex,",
        "    nanosleep,",
        "    clock_gettime,",
        "    clock_getres,",
        "    getrandom,",
        "    gettimeofday,",
        "    sched_yield,",
        "    rt_sigaction,",
        "    rt_sigreturn,",
        "    rt_sigprocmask,",
        "    sigaltstack,",
        "    restart_syscall,",
        "    arch_prctl,",
        "    set_tid_address,",
        "    set_robust_list,",
        "    prlimit64,",
        "    getpid,",
        "    gettid,",
        "    tgkill,",
        "    openat,",
        "    readlink,",
        "    fstat,",
        "    fstatfs,",
        "    stat,",
        "    lstat,",
        "    newfstatat,",
        "    getdents64,",
        "    pread64,",
        "    getcwd,",
        "    getuid,",
        "    getgid,",
        "    geteuid,",
        "    getegid,",
        "    uname,",
        "}",
    ]
    .join("\n")
}
