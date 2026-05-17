//! Litematica `.litematic` root tag construction (schema v6).
//!
//! See `specs/001-hdl-compiler-cli/contracts/litematic-nbt.md` for the
//! tag tree this module emits.

use std::collections::BTreeMap;

use fastnbt::LongArray;
use rb_core::{Bbox3, Pos3};
use serde::{Deserialize, Serialize};

use crate::pack::pack_block_states;
use crate::palette::{BlockState, Palette};
use crate::BlockGrid;

/// Litematica `Version` field — schema version of the `.litematic`
/// container. v6 is the long-stable Litematica schema in MC 1.20+ and
/// still in use for 26.1-compatible Litematica builds.
pub const LITEMATICA_SCHEMA_VERSION: i32 = 6;

/// Minecraft Java Edition 26.1 data version (placeholder constant —
/// the actual integer value is published by Mojang per release). We use
/// the most recent confirmed value at the time of writing; updating
/// this is a one-line change if a more authoritative number is
/// available at integration time.
pub const MC_DATA_VERSION_26_1: i32 = 4_188;

/// Root of a Litematica file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LitematicaRoot {
    /// Java Edition data version (per release).
    #[serde(rename = "MinecraftDataVersion")]
    pub minecraft_data_version: i32,
    /// Litematica schema version.
    #[serde(rename = "Version")]
    pub version: i32,
    /// File-level metadata.
    #[serde(rename = "Metadata")]
    pub metadata: LitematicaMetadata,
    /// Region map (name → region). v1 always emits a single region
    /// called `"main"`.
    #[serde(rename = "Regions")]
    pub regions: BTreeMap<String, Region>,
}

/// Litematica `Metadata` compound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LitematicaMetadata {
    /// Schematic name (used as Litematica list display name).
    #[serde(rename = "Name")]
    pub name: String,
    /// Author string.
    #[serde(rename = "Author")]
    pub author: String,
    /// Human-readable description.
    #[serde(rename = "Description")]
    pub description: String,
    /// Always 0 for v1 (FR-017: no wall-clock in output).
    #[serde(rename = "TimeCreated")]
    pub time_created: i64,
    /// Always 0 for v1 (FR-017).
    #[serde(rename = "TimeModified")]
    pub time_modified: i64,
    /// Non-air block count.
    #[serde(rename = "TotalBlocks")]
    pub total_blocks: i32,
    /// Total cell count = volume of `EnclosingSize`.
    #[serde(rename = "TotalVolume")]
    pub total_volume: i32,
    /// Region count (v1 always 1).
    #[serde(rename = "RegionCount")]
    pub region_count: i32,
    /// Bounding-box dimensions.
    #[serde(rename = "EnclosingSize")]
    pub enclosing_size: Xyz,
}

/// One Litematica region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Region origin in world coords.
    #[serde(rename = "Position")]
    pub position: Xyz,
    /// Signed size; v1 always emits positive values.
    #[serde(rename = "Size")]
    pub size: Xyz,
    /// Distinct block-states present in the region (index 0 = AIR).
    #[serde(rename = "BlockStatePalette")]
    pub block_state_palette: Vec<BlockState>,
    /// Packed-long indices into the palette.
    #[serde(rename = "BlockStates")]
    pub block_states: LongArray,
    /// Always empty in v1.
    #[serde(rename = "Entities")]
    pub entities: Vec<fastnbt::Value>,
    /// Always empty in v1.
    #[serde(rename = "TileEntities")]
    pub tile_entities: Vec<fastnbt::Value>,
    /// Always empty in v1.
    #[serde(rename = "PendingBlockTicks")]
    pub pending_block_ticks: Vec<fastnbt::Value>,
    /// Always empty in v1.
    #[serde(rename = "PendingFluidTicks")]
    pub pending_fluid_ticks: Vec<fastnbt::Value>,
}

/// 3-int compound used by Litematica for `Position`, `Size`,
/// `EnclosingSize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Xyz {
    /// X.
    pub x: i32,
    /// Y.
    pub y: i32,
    /// Z.
    pub z: i32,
}

impl From<Pos3> for Xyz {
    fn from(p: Pos3) -> Self {
        Self {
            x: p.x,
            y: p.y,
            z: p.z,
        }
    }
}

/// Build the Litematica root tag from a [`BlockGrid`].
pub fn build_root(grid: &BlockGrid, name: &str, description: &str) -> LitematicaRoot {
    let bounds = grid.bounds;
    let w = bounds.width() as i32;
    let h = bounds.height() as i32;
    let d = bounds.depth() as i32;

    let mut palette = Palette::new();
    let total_indices = (w as usize) * (h as usize) * (d as usize);
    let mut indices: Vec<u32> = Vec::with_capacity(total_indices);

    let air_idx = palette.insert(BlockState::simple("minecraft:air"));

    for y in bounds.min.y..=bounds.max.y {
        for z in bounds.min.z..=bounds.max.z {
            for x in bounds.min.x..=bounds.max.x {
                let p = Pos3::new(x, y, z);
                let idx = match grid.blocks.get(&p) {
                    Some(state) => palette.insert(state.clone()),
                    None => air_idx,
                };
                indices.push(idx);
            }
        }
    }

    let bits = palette.bits_per_index();
    let packed = pack_block_states(&indices, bits);

    let total_blocks = grid
        .blocks
        .values()
        .filter(|s| s.name.as_str() != "minecraft:air")
        .count() as i32;
    let total_volume = (w as i64).saturating_mul(h as i64).saturating_mul(d as i64) as i32;

    let region = Region {
        position: Xyz::from(Pos3::ORIGIN),
        size: Xyz { x: w, y: h, z: d },
        block_state_palette: palette.entries().to_vec(),
        block_states: LongArray::new(packed),
        entities: Vec::new(),
        tile_entities: Vec::new(),
        pending_block_ticks: Vec::new(),
        pending_fluid_ticks: Vec::new(),
    };

    let mut regions = BTreeMap::new();
    regions.insert("main".to_string(), region);

    LitematicaRoot {
        minecraft_data_version: MC_DATA_VERSION_26_1,
        version: LITEMATICA_SCHEMA_VERSION,
        metadata: LitematicaMetadata {
            name: name.to_string(),
            author: "redstonebuilder".to_string(),
            description: description.to_string(),
            time_created: 0,
            time_modified: 0,
            total_blocks,
            total_volume,
            region_count: 1,
            enclosing_size: Xyz { x: w, y: h, z: d },
        },
        regions,
    }
}

/// Compute the [`Bbox3`] needed to populate a placed-block grid
/// — convenience for callers building a [`BlockGrid`].
pub fn bounds_for_origin_and_size(origin: Pos3, size: Xyz) -> Bbox3 {
    Bbox3::from_corners(
        origin,
        Pos3::new(
            origin.x + size.x.saturating_sub(1),
            origin.y + size.y.saturating_sub(1),
            origin.z + size.z.saturating_sub(1),
        ),
    )
}
