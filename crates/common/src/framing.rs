use std::fmt;
use std::io::{self, Read, Write};

pub const FRAME_HEADER_LEN: usize = 8;
pub const GAME_INPUT_MAGIC: [u8; 4] = *b"BWI1";
pub const GAME_OUTPUT_MAGIC: [u8; 4] = *b"BWO1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    GameInput,
    GameOutput,
}

impl FrameKind {
    pub const fn magic(self) -> [u8; 4] {
        match self {
            Self::GameInput => GAME_INPUT_MAGIC,
            Self::GameOutput => GAME_OUTPUT_MAGIC,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooShort { len: usize },
    WrongMagic { expected: [u8; 4], actual: [u8; 4] },
    LengthMismatch { declared: usize, actual: usize },
    PayloadTooLarge { len: usize },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "frame too short: {len} bytes"),
            Self::WrongMagic { expected, actual } => write!(
                f,
                "wrong frame magic: expected {}, got {}",
                magic_to_string(*expected),
                magic_to_string(*actual)
            ),
            Self::LengthMismatch { declared, actual } => {
                write!(
                    f,
                    "frame length mismatch: declared {declared}, actual {actual}"
                )
            }
            Self::PayloadTooLarge { len } => {
                write!(f, "payload too large for u32 frame length: {len}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

pub fn encode_frame(kind: FrameKind, payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| FrameError::PayloadTooLarge { len: payload.len() })?;
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    frame.extend_from_slice(&kind.magic());
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

pub fn decode_frame(frame: &[u8], expected_kind: FrameKind) -> Result<&[u8], FrameError> {
    if frame.len() < FRAME_HEADER_LEN {
        return Err(FrameError::TooShort { len: frame.len() });
    }

    let actual_magic = [frame[0], frame[1], frame[2], frame[3]];
    let expected_magic = expected_kind.magic();
    if actual_magic != expected_magic {
        return Err(FrameError::WrongMagic {
            expected: expected_magic,
            actual: actual_magic,
        });
    }

    let declared = u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
    let payload = &frame[FRAME_HEADER_LEN..];
    if declared != payload.len() {
        return Err(FrameError::LengthMismatch {
            declared,
            actual: payload.len(),
        });
    }

    Ok(payload)
}

pub fn write_frame<W: Write>(
    mut writer: W,
    kind: FrameKind,
    payload: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let frame = encode_frame(kind, payload)?;
    writer.write_all(&frame)?;
    Ok(())
}

pub fn read_frame<R: Read>(
    mut reader: R,
    expected_kind: FrameKind,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut header = [0_u8; FRAME_HEADER_LEN];
    reader.read_exact(&mut header)?;

    let actual_magic = [header[0], header[1], header[2], header[3]];
    let expected_magic = expected_kind.magic();
    if actual_magic != expected_magic {
        return Err(Box::new(FrameError::WrongMagic {
            expected: expected_magic,
            actual: actual_magic,
        }));
    }

    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let mut payload = vec![0_u8; len];
    reader
        .read_exact(&mut payload)
        .map_err(|err| match err.kind() {
            io::ErrorKind::UnexpectedEof => io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("unexpected EOF while reading {len} byte frame payload"),
            ),
            _ => err,
        })?;
    Ok(payload)
}

fn magic_to_string(magic: [u8; 4]) -> String {
    String::from_utf8_lossy(&magic).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_frame() {
        let payload = b"payload";
        let frame = encode_frame(FrameKind::GameInput, payload).expect("encode frame");

        assert_eq!(&frame[..4], b"BWI1");
        assert_eq!(
            decode_frame(&frame, FrameKind::GameInput).expect("decode frame"),
            payload
        );
    }

    #[test]
    fn rejects_wrong_magic() {
        let frame = encode_frame(FrameKind::GameInput, b"payload").expect("encode frame");

        assert!(matches!(
            decode_frame(&frame, FrameKind::GameOutput),
            Err(FrameError::WrongMagic { .. })
        ));
    }
}
