use std::fmt;
use std::str::FromStr;

pub struct Hex32(pub [u8; 32]);

pub enum HexError {
    Length(usize),
    Digit,
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexError::Length(len) => write!(f, "expected 64 hex characters, got {len}"),
            HexError::Digit => write!(f, "expected 64 hex characters (0-9, a-f)"),
        }
    }
}

impl FromStr for Hex32 {
    type Err = HexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if bytes.len() != 64 {
            return Err(HexError::Length(bytes.len()));
        }
        let mut out = [0u8; 32];
        for (byte, pair) in out.iter_mut().zip(bytes.chunks_exact(2)) {
            let hi = digit(pair[0])?;
            let lo = digit(pair[1])?;
            *byte = hi << 4 | lo;
        }
        Ok(Hex32(out))
    }
}

fn digit(byte: u8) -> Result<u8, HexError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(HexError::Digit),
    }
}
