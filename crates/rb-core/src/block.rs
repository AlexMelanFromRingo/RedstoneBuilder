//! Strongly-typed catalogue of Minecraft block primitives the
//! synthesizer and the NBT writer agree on.
//!
//! The synthesizer emits `(Pos3, BlockId, Option<Direction>)` triples;
//! `rb-nbt` is responsible for translating `BlockId` + facing into a
//! full `BlockState` with the correct property map (see
//! `contracts/minecraft-blocks.md`).
//!
//! `#[non_exhaustive]` so the Post-MVP epic can add `Observer`,
//! comparator variants, etc. without a SemVer-major bump.

use serde::{Deserialize, Serialize};

/// One Minecraft block primitive used by RedstoneBuilder's cell library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BlockId {
    /// `minecraft:air` (empty cell).
    Air,
    /// Generic non-redstone solid support block (`minecraft:stone`).
    Stone,
    /// Non-conductive solid (`minecraft:oak_planks`), used under dust
    /// to keep adjacent dust from coupling through a stone block.
    OakPlanks,
    /// `minecraft:oak_stairs` for level-change routing.
    OakStairs,
    /// `minecraft:redstone_wire` (a.k.a. dust).
    RedstoneDust,
    /// `minecraft:redstone_torch` (upright, mounted on top of the block
    /// below).
    RedstoneTorch,
    /// `minecraft:redstone_wall_torch` (side-mounted; needs a `facing`).
    RedstoneWallTorch,
    /// `minecraft:repeater` (needs a `facing`).
    Repeater,
    /// `minecraft:comparator` (reserved for `MemoryCell` in US2 and for
    /// the Post-MVP analog epic).
    Comparator,
    /// `minecraft:lever` (one per module input).
    Lever,
    /// `minecraft:redstone_lamp` (one per module output, visual).
    RedstoneLamp,
}
