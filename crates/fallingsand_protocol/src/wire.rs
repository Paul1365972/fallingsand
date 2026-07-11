use fallingsand_core::{Cell, MaterialId};
use rustc_hash::FxHashMap;
use serde::Serialize;
use serde::de::DeserializeOwned;

const COMPRESSION_THRESHOLD: usize = 256;
const CELL_WIRE_BYTES: usize = 3;
const MAX_DECOMPRESSED_LEN: usize = 64 * 1024 * 1024;

const TAG_RAW: u8 = 0;
const TAG_LZ4: u8 = 1;

const CELLS_UNIFORM: u8 = 0;
const CELLS_PALETTE: u8 = 1;
const CELLS_RAW: u8 = 2;
const MAX_PALETTE: usize = 256;

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
    #[error("invalid material id {0}")]
    InvalidMaterial(u16),
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

fn cell_entry(cell: Cell) -> [u8; CELL_WIRE_BYTES] {
    let material = cell.material.0.to_le_bytes();
    [material[0], material[1], cell.shade_flags]
}

fn entry_cell(entry: &[u8]) -> Result<Cell, WireError> {
    let material = u16::from_le_bytes([entry[0], entry[1]]);
    if material as usize >= fallingsand_core::content::MATERIAL_COUNT {
        return Err(WireError::InvalidMaterial(material));
    }
    Ok(Cell {
        material: MaterialId(material),
        vx: 0,
        vy: 0,
        shade_flags: entry[2],
        updated: 0,
    })
}

fn index_bits(palette_len: usize) -> u32 {
    usize::BITS - (palette_len - 1).leading_zeros()
}

fn cells_to_wire_raw(cells: &[Cell]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + cells.len() * CELL_WIRE_BYTES);
    out.push(CELLS_RAW);
    for &cell in cells {
        out.extend_from_slice(&cell_entry(cell));
    }
    out
}

pub fn cells_to_wire(cells: &[Cell]) -> Vec<u8> {
    let mut palette: Vec<[u8; CELL_WIRE_BYTES]> = Vec::new();
    let mut lookup: FxHashMap<u32, u8> = FxHashMap::default();
    let mut indices: Vec<u8> = Vec::with_capacity(cells.len());
    for &cell in cells {
        let key = cell.material.0 as u32 | (cell.shade_flags as u32) << 16;
        match lookup.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => indices.push(*entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                if palette.len() == MAX_PALETTE {
                    return cells_to_wire_raw(cells);
                }
                let index = palette.len() as u8;
                entry.insert(index);
                palette.push(cell_entry(cell));
                indices.push(index);
            }
        }
    }
    if palette.len() <= 1 {
        let mut out = Vec::with_capacity(1 + CELL_WIRE_BYTES);
        out.push(CELLS_UNIFORM);
        out.extend_from_slice(&palette.first().copied().unwrap_or_default());
        return out;
    }
    let bits = index_bits(palette.len());
    let packed_len = (cells.len() * bits as usize).div_ceil(8);
    let paletted_len = 2 + palette.len() * CELL_WIRE_BYTES + packed_len;
    if paletted_len > cells.len() * CELL_WIRE_BYTES {
        return cells_to_wire_raw(cells);
    }
    let mut out = Vec::with_capacity(paletted_len);
    out.push(CELLS_PALETTE);
    out.push((palette.len() - 1) as u8);
    for entry in &palette {
        out.extend_from_slice(entry);
    }
    let mut acc: u32 = 0;
    let mut filled: u32 = 0;
    for &index in &indices {
        acc |= (index as u32) << filled;
        filled += bits;
        while filled >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            filled -= 8;
        }
    }
    if filled > 0 {
        out.push(acc as u8);
    }
    out
}

pub fn cells_from_wire(bytes: &[u8], count: usize) -> Result<Vec<Cell>, WireError> {
    let bad = || WireError::BadCellPayload(bytes.len());
    let (&tag, payload) = bytes.split_first().ok_or(WireError::Empty)?;
    match tag {
        CELLS_UNIFORM => {
            if payload.len() != CELL_WIRE_BYTES {
                return Err(bad());
            }
            Ok(vec![entry_cell(payload)?; count])
        }
        CELLS_PALETTE => {
            let (&len_minus_one, rest) = payload.split_first().ok_or_else(bad)?;
            let palette_len = len_minus_one as usize + 1;
            if palette_len < 2 {
                return Err(bad());
            }
            let (palette_bytes, packed) = rest
                .split_at_checked(palette_len * CELL_WIRE_BYTES)
                .ok_or_else(bad)?;
            let bits = index_bits(palette_len);
            if packed.len() != (count * bits as usize).div_ceil(8) {
                return Err(bad());
            }
            let palette: Vec<Cell> = palette_bytes
                .chunks_exact(CELL_WIRE_BYTES)
                .map(entry_cell)
                .collect::<Result<_, _>>()?;
            let mask = (1u32 << bits) - 1;
            let mut cells = Vec::with_capacity(count);
            let mut acc: u32 = 0;
            let mut filled: u32 = 0;
            let mut packed = packed.iter();
            for _ in 0..count {
                while filled < bits {
                    acc |= (*packed.next().ok_or_else(bad)? as u32) << filled;
                    filled += 8;
                }
                let index = (acc & mask) as usize;
                acc >>= bits;
                filled -= bits;
                cells.push(*palette.get(index).ok_or_else(bad)?);
            }
            if acc != 0 {
                return Err(bad());
            }
            Ok(cells)
        }
        CELLS_RAW => {
            if payload.len() != count * CELL_WIRE_BYTES {
                return Err(bad());
            }
            payload
                .chunks_exact(CELL_WIRE_BYTES)
                .map(entry_cell)
                .collect()
        }
        _ => Err(bad()),
    }
}
