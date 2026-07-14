use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Hex32(pub [u8; 32]);

#[derive(Clone, Copy, PartialEq, Eq)]
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

impl Hex32 {
    pub fn to_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl FromStr for Hex32 {
    type Err = HexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if bytes.len() != 64 {
            return Err(HexError::Length(s.chars().count()));
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

impl TryFrom<&str> for Hex32 {
    type Error = HexError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
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
