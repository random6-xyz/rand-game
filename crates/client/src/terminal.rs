use std::fs::File;
use std::io::{Read, Write};
use std::process::Command;

pub(crate) struct TerminalMode {
    saved: String,
    tty: File,
}

impl TerminalMode {
    pub(crate) fn enter() -> Result<Self, Box<dyn std::error::Error>> {
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

    pub(crate) fn handle_input(
        &mut self,
        x: &mut i32,
        y: &mut i32,
        pan_step: i32,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let mut buffer = [0_u8; 64];
        let read = self.tty.read(&mut buffer)?;

        for byte in &buffer[..read] {
            match byte.to_ascii_lowercase() {
                b'w' => *y += pan_step,
                b'a' => *x -= pan_step,
                b's' => *y -= pan_step,
                b'd' => *x += pan_step,
                b'q' | 0x03 => return Ok(true),
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

pub(crate) fn write_raw_terminal_frame(frame: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::io::stdout().write_all(frame.replace('\n', "\r\n").as_bytes())?;
    Ok(())
}
