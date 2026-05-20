//! Read Sponge `.schem` files (WorldEdit / Litematica external format).
//!
//! Format spec: <https://github.com/SpongePowered/Schematic-Specification>
//!
//! Used by RedstoneBuilder v3 to ingest stub schematics built in MC
//! and exported via WorldEdit. We only need the read path; for write
//! we still emit `.litematic` (Litematica's native format, see
//! [`crate::writer`]).

use std::collections::BTreeMap;
use std::io::Read;

use fastnbt::{from_bytes, ByteArray, IntArray, Value};
use flate2::read::GzDecoder;
use rb_core::Pos3;
use serde::Deserialize;
use smol_str::SmolStr;

use crate::palette::BlockState;

/// A decoded Sponge `.schem`. Coordinates are stub-local (start at
/// `(0, 0, 0)`); the file's `Offset` field is preserved separately.
#[derive(Debug, Clone)]
pub struct SpongeSchematic {
    /// Schematic dimensions (in cells).
    pub width: u16,
    /// Schematic height (Y).
    pub height: u16,
    /// Schematic length (Z).
    pub length: u16,
    /// World-relative offset stored in the file (for placement when
    /// re-imported into WorldEdit). Most of our consumers can ignore
    /// this.
    pub offset: [i32; 3],
    /// Minecraft data version of the originating MC release.
    pub data_version: i32,
    /// Decoded block grid. Air cells are NOT stored (sparse).
    pub blocks: BTreeMap<Pos3, BlockState>,
    /// Block entities — typically signs, with their `Pos` and text.
    pub block_entities: Vec<BlockEntity>,
}

/// A block entity (TE) as stored in `.schem`. We keep the raw
/// `attributes` map so callers can pull out sign text, item lists,
/// etc. as needed.
#[derive(Debug, Clone)]
pub struct BlockEntity {
    /// `minecraft:sign`, `minecraft:repeater`, ... (identifier).
    pub id: SmolStr,
    /// Stub-local position.
    pub pos: Pos3,
    /// Remaining NBT fields, untyped.
    pub raw: BTreeMap<SmolStr, Value>,
}

impl BlockEntity {
    /// Convenience: for sign block-entities, return the 4 visible
    /// text lines (stripped of the JSON-text-component wrapper that
    /// MC 1.13+ uses). Empty `Vec` if this BE is not a sign or the
    /// text fields are missing.
    pub fn sign_text(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::with_capacity(4);
        // Pre-1.20 layout: top-level Text1/Text2/Text3/Text4 string
        // fields each containing a JSON text component.
        for key in ["Text1", "Text2", "Text3", "Text4"] {
            if let Some(Value::String(s)) = self.raw.get(key) {
                out.push(strip_json_text(s));
            }
        }
        // 1.20+ layout: front_text.messages (List of String).
        if out.is_empty() {
            if let Some(Value::Compound(front)) = self.raw.get("front_text") {
                if let Some(Value::List(messages)) = front.get("messages") {
                    for msg in messages.iter().take(4) {
                        if let Value::String(s) = msg {
                            out.push(strip_json_text(s));
                        }
                    }
                }
            }
        }
        out
    }
}

/// Decode a Sponge `.schem` byte stream (typically gzip-compressed
/// NBT).
pub fn decode_sponge_schem(bytes: &[u8]) -> Result<SpongeSchematic, SpongeError> {
    let raw_nbt = decompress(bytes)?;
    let parsed: RawSchem = from_bytes(&raw_nbt).map_err(|e| SpongeError::BadNbt(e.to_string()))?;

    let width = parsed.width;
    let height = parsed.height;
    let length = parsed.length;

    // Sponge palette: BlockState string → index. Invert.
    let mut index_to_state: BTreeMap<u32, BlockState> = BTreeMap::new();
    for (state_string, idx) in &parsed.palette {
        index_to_state.insert(*idx as u32, parse_block_state(state_string)?);
    }

    // BlockData is VarInt-encoded for palettes ≤2³² (effectively all).
    // For palettes ≤127, each VarInt is one byte; for larger,
    // multi-byte encoding kicks in.
    // ByteArray returns &[i8]; copy as u8 for VarInt decoding (the
    // workspace forbids `unsafe_code`, so we can't reinterpret).
    let bd_u8: Vec<u8> = parsed.block_data.iter().map(|&b| b as u8).collect();
    let block_indices = decode_var_int_blockdata(&bd_u8)?;
    let expected = (width as usize) * (height as usize) * (length as usize);
    if block_indices.len() != expected {
        return Err(SpongeError::Mismatch(format!(
            "BlockData has {} cells but dims say {expected}",
            block_indices.len()
        )));
    }

    let mut blocks: BTreeMap<Pos3, BlockState> = BTreeMap::new();
    let w = width as usize;
    let l = length as usize;
    for (idx, palette_idx) in block_indices.iter().enumerate() {
        let y = (idx / (w * l)) as i32;
        let z = ((idx / w) % l) as i32;
        let x = (idx % w) as i32;
        if let Some(state) = index_to_state.get(palette_idx) {
            if state.name.as_str() != "minecraft:air" {
                blocks.insert(Pos3::new(x, y, z), state.clone());
            }
        }
    }

    let block_entities = parsed
        .block_entities
        .unwrap_or_default()
        .into_iter()
        .map(parse_block_entity)
        .collect::<Result<Vec<_>, _>>()?;

    let offset = parsed
        .offset
        .map(|ia| {
            let slice: &[i32] = ia.as_ref();
            [
                slice.first().copied().unwrap_or(0),
                slice.get(1).copied().unwrap_or(0),
                slice.get(2).copied().unwrap_or(0),
            ]
        })
        .unwrap_or([0, 0, 0]);

    Ok(SpongeSchematic {
        width,
        height,
        length,
        offset,
        data_version: parsed.data_version,
        blocks,
        block_entities,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawSchem {
    width: u16,
    height: u16,
    length: u16,
    /// Sponge stores Offset as TAG_Int_Array (3 entries).
    #[serde(default)]
    offset: Option<IntArray>,
    data_version: i32,
    palette: BTreeMap<String, i32>,
    block_data: ByteArray,
    #[serde(default)]
    block_entities: Option<Vec<RawBlockEntity>>,
}

#[derive(Debug, Deserialize)]
struct RawBlockEntity {
    /// MC 1.16+ writes `Id`; older versions and some tools use `id`.
    #[serde(alias = "id", alias = "Id", default)]
    id: Option<String>,
    /// Block entity Pos is TAG_Int_Array (3 entries).
    #[serde(rename = "Pos", alias = "pos")]
    pos: IntArray,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

/// Errors from Sponge .schem decoding.
#[derive(Debug, thiserror::Error)]
pub enum SpongeError {
    /// I/O while decompressing the gzip stream.
    #[error("decompression failed: {0}")]
    Decompress(#[from] std::io::Error),
    /// The NBT was structurally invalid or didn't match our schema.
    #[error("bad NBT: {0}")]
    BadNbt(String),
    /// Dimension fields and BlockData length disagree.
    #[error("schematic structure mismatch: {0}")]
    Mismatch(String),
    /// A palette block-state string couldn't be parsed.
    #[error("malformed block state string: {0}")]
    BadBlockState(String),
}

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

fn decompress(bytes: &[u8]) -> Result<Vec<u8>, SpongeError> {
    // Sponge .schem are gzip-compressed NBT. Some tools emit
    // uncompressed; sniff the gzip magic (1F 8B) to decide.
    if bytes.len() >= 2 && bytes[0] == 0x1F && bytes[1] == 0x8B {
        let mut out = Vec::with_capacity(bytes.len() * 4);
        GzDecoder::new(bytes).read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Sponge VarInt block-data: little-endian 7-bit groups, MSB = "more".
/// Returns one palette index per cell, in Sponge iteration order.
fn decode_var_int_blockdata(bytes: &[u8]) -> Result<Vec<u32>, SpongeError> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let mut value: u32 = 0;
        let mut shift = 0;
        loop {
            if i >= bytes.len() {
                return Err(SpongeError::BadNbt(
                    "truncated VarInt in BlockData".to_string(),
                ));
            }
            let byte = bytes[i];
            i += 1;
            value |= ((byte & 0x7F) as u32) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 32 {
                return Err(SpongeError::BadNbt(
                    "VarInt overflow in BlockData".to_string(),
                ));
            }
        }
        out.push(value);
    }
    Ok(out)
}

/// Parse `"minecraft:repeater[delay=1,facing=west,...]"` into a
/// [`BlockState`]. Tolerates the no-properties form
/// `"minecraft:stone"`.
fn parse_block_state(s: &str) -> Result<BlockState, SpongeError> {
    let (name, props_str) = match s.find('[') {
        Some(idx) => {
            let rest = &s[idx + 1..];
            let end = rest
                .rfind(']')
                .ok_or_else(|| SpongeError::BadBlockState(format!("missing ']' in {s:?}")))?;
            (&s[..idx], &rest[..end])
        }
        None => (s, ""),
    };
    let mut properties: BTreeMap<SmolStr, SmolStr> = BTreeMap::new();
    if !props_str.is_empty() {
        for kv in props_str.split(',') {
            let mut parts = kv.splitn(2, '=');
            let k = parts
                .next()
                .ok_or_else(|| SpongeError::BadBlockState(s.to_string()))?
                .trim();
            let v = parts
                .next()
                .ok_or_else(|| SpongeError::BadBlockState(s.to_string()))?
                .trim();
            properties.insert(SmolStr::new(k), SmolStr::new(v));
        }
    }
    Ok(BlockState {
        name: SmolStr::new(name),
        properties,
    })
}

fn parse_block_entity(raw: RawBlockEntity) -> Result<BlockEntity, SpongeError> {
    let id = raw
        .id
        .ok_or_else(|| SpongeError::BadNbt("BlockEntity missing Id/id".to_string()))?;
    let mut extra: BTreeMap<SmolStr, Value> = BTreeMap::new();
    for (k, v) in raw.extra {
        extra.insert(SmolStr::new(k), v);
    }
    let pos_slice: &[i32] = raw.pos.as_ref();
    let (px, py, pz) = match pos_slice {
        [x, y, z] => (*x, *y, *z),
        _ => {
            return Err(SpongeError::BadNbt(format!(
                "BlockEntity Pos has {} entries, expected 3",
                pos_slice.len()
            )))
        }
    };
    Ok(BlockEntity {
        id: SmolStr::new(id),
        pos: Pos3::new(px, py, pz),
        raw: extra,
    })
}

/// Strip MC 1.13+ JSON text-component wrappers (e.g.
/// `{"text":"and_h8b"}`) to just the user-visible text. Returns the
/// input unchanged if it doesn't look like JSON.
fn strip_json_text(s: &str) -> String {
    // Cheap regex-free extraction: look for `"text":"<...>"` and
    // pull the inner string. Good enough for stub sign labels.
    if let Some(start) = s.find(r#""text":""#) {
        let rest = &s[start + 8..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    s.to_string()
}
