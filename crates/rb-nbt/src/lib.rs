//! Litematica `.litematic` NBT writer for RedstoneBuilder.
//!
//! See `specs/001-hdl-compiler-cli/contracts/litematic-nbt.md` for the
//! full output tag tree and `contracts/minecraft-blocks.md` for the
//! block-state catalogue.

#![deny(missing_docs)]

use std::collections::BTreeMap;

use rb_core::{Bbox3, Pos3};

pub mod error;
pub mod litematic;
pub mod pack;
pub mod palette;
pub mod writer;

pub use error::NbtError;
pub use litematic::{
    bounds_for_origin_and_size, build_root, LitematicaMetadata, LitematicaRoot, Region, Xyz,
    LITEMATICA_SCHEMA_VERSION, MC_DATA_VERSION_26_1,
};
pub use pack::{pack_block_states, unpack_block_states};
pub use palette::{block_state_for, BlockState, Palette};
pub use writer::{decode_from_bytes, encode_to_bytes, write_litematic};

/// Sparse 3D block grid the writer consumes. Cells not present are
/// implicitly `minecraft:air`.
#[derive(Debug, Clone)]
pub struct BlockGrid {
    /// Bounding box covered by the grid. The writer iterates every cell
    /// inside this bbox in `(y, z, x)` order; missing cells become AIR.
    pub bounds: Bbox3,
    /// Populated cells.
    pub blocks: BTreeMap<Pos3, BlockState>,
}

impl BlockGrid {
    /// Construct an empty grid with the given bounds.
    pub fn empty(bounds: Bbox3) -> Self {
        Self {
            bounds,
            blocks: BTreeMap::new(),
        }
    }

    /// Insert a block at `pos` (overwriting any previous one). Panics
    /// in debug builds if `pos` is outside `bounds`.
    pub fn insert(&mut self, pos: Pos3, state: BlockState) {
        debug_assert!(
            self.bounds.contains(pos),
            "BlockGrid::insert: {pos:?} is outside bounds {:?}",
            self.bounds
        );
        self.blocks.insert(pos, state);
    }
}
