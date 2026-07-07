use fallingsand_core::{Cell, MaterialId};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub const COMPRESSION_THRESHOLD: usize = 256;
pub const CELL_WIRE_BYTES: usize = 3;
pub const MAX_DECOMPRESSED_LEN: usize = 64 * 1024 * 1024;

const TAG_RAW: u8 = 0;
const TAG_LZ4: u8 = 1;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("postcard: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("decompression failed: {0}")]
    Decompress(#[from] lz4_flex::block::DecompressError),
    #[error("empty payload")]
    Empty,
    #[error("unknown compression tag {0}")]
    UnknownTag(u8),
    #[error("declared decompressed size {0} exceeds limit {MAX_DECOMPRESSED_LEN}")]
    TooLarge(usize),
    #[error("cell payload has invalid length {0}")]
    BadCellPayload(usize),
}

pub fn encode_message<T: Serialize>(message: &T) -> Vec<u8> {
    let raw = postcard::to_allocvec(message).expect("message serialization is infallible");
    if raw.len() > COMPRESSION_THRESHOLD {
        let mut out = Vec::with_capacity(raw.len() / 2 + 16);
        out.push(TAG_LZ4);
        out.extend_from_slice(&lz4_flex::compress_prepend_size(&raw));
        if out.len() < raw.len() + 1 {
            return out;
        }
    }
    let mut out = Vec::with_capacity(raw.len() + 1);
    out.push(TAG_RAW);
    out.extend_from_slice(&raw);
    out
}

pub fn decode_message<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, WireError> {
    let (&tag, payload) = bytes.split_first().ok_or(WireError::Empty)?;
    match tag {
        TAG_RAW => Ok(postcard::from_bytes(payload)?),
        TAG_LZ4 => {
            if let Some(prefix) = payload.get(..4) {
                let size = u32::from_le_bytes(prefix.try_into().unwrap()) as usize;
                if size > MAX_DECOMPRESSED_LEN {
                    return Err(WireError::TooLarge(size));
                }
            }
            let raw = lz4_flex::decompress_size_prepended(payload)?;
            Ok(postcard::from_bytes(&raw)?)
        }
        other => Err(WireError::UnknownTag(other)),
    }
}

pub fn cells_to_wire(cells: &[Cell]) -> Vec<u8> {
    let mut out = Vec::with_capacity(cells.len() * CELL_WIRE_BYTES);
    for cell in cells {
        out.extend_from_slice(&cell.material.0.to_le_bytes());
        out.push(cell.shade_flags);
    }
    out
}

pub fn cells_from_wire(bytes: &[u8]) -> Result<Vec<Cell>, WireError> {
    if !bytes.len().is_multiple_of(CELL_WIRE_BYTES) {
        return Err(WireError::BadCellPayload(bytes.len()));
    }
    Ok(bytes
        .chunks_exact(CELL_WIRE_BYTES)
        .map(|raw| Cell {
            material: MaterialId(u16::from_le_bytes([raw[0], raw[1]])),
            vx: 0,
            vy: 0,
            shade_flags: raw[2],
            updated: 0,
        })
        .collect())
}
