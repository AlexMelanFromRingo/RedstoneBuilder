//! Block-state catalogue (BlockId → BlockState) and palette indexing.

use std::collections::BTreeMap;

use rb_core::{BlockId, Direction};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// One Minecraft block-state: namespaced ID + sorted property map.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockState {
    /// Block name (e.g. `"minecraft:redstone_torch"`).
    #[serde(rename = "Name")]
    pub name: SmolStr,
    /// Block-state properties (sorted for determinism).
    #[serde(
        rename = "Properties",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub properties: BTreeMap<SmolStr, SmolStr>,
}

impl BlockState {
    /// Convenience constructor for a no-property block (e.g. `AIR`).
    pub fn simple(name: &str) -> Self {
        Self {
            name: SmolStr::new(name),
            properties: BTreeMap::new(),
        }
    }
}

/// Translate a [`BlockId`] (+ optional facing) into a concrete
/// [`BlockState`] for MC Java Edition 26.1.
///
/// `facing` is consumed only by blocks whose `Name` requires a
/// `facing` property (repeaters, comparators, wall torches, levers,
/// stairs); other blocks ignore it.
pub fn block_state_for(block: BlockId, facing: Option<Direction>) -> BlockState {
    let face = facing.map(|d| SmolStr::new(d.as_str()));
    match block {
        BlockId::Air => BlockState::simple("minecraft:air"),
        BlockId::Stone => BlockState::simple("minecraft:stone"),
        BlockId::OakPlanks => BlockState::simple("minecraft:oak_planks"),
        BlockId::OakStairs => {
            let mut props = BTreeMap::new();
            props.insert(
                SmolStr::new("facing"),
                face.unwrap_or_else(|| SmolStr::new("north")),
            );
            props.insert(SmolStr::new("half"), SmolStr::new("bottom"));
            props.insert(SmolStr::new("shape"), SmolStr::new("straight"));
            props.insert(SmolStr::new("waterlogged"), SmolStr::new("false"));
            BlockState {
                name: SmolStr::new("minecraft:oak_stairs"),
                properties: props,
            }
        }
        BlockId::RedstoneDust => {
            let mut props = BTreeMap::new();
            props.insert(SmolStr::new("power"), SmolStr::new("0"));
            // Connection sides are recomputed by the game on load; emit
            // a neutral "none on all sides" baseline.
            for side in ["north", "south", "east", "west"] {
                props.insert(SmolStr::new(side), SmolStr::new("none"));
            }
            BlockState {
                name: SmolStr::new("minecraft:redstone_wire"),
                properties: props,
            }
        }
        BlockId::RedstoneTorch => {
            let mut props = BTreeMap::new();
            props.insert(SmolStr::new("lit"), SmolStr::new("true"));
            BlockState {
                name: SmolStr::new("minecraft:redstone_torch"),
                properties: props,
            }
        }
        BlockId::RedstoneWallTorch => {
            let mut props = BTreeMap::new();
            props.insert(
                SmolStr::new("facing"),
                face.unwrap_or_else(|| SmolStr::new("north")),
            );
            props.insert(SmolStr::new("lit"), SmolStr::new("true"));
            BlockState {
                name: SmolStr::new("minecraft:redstone_wall_torch"),
                properties: props,
            }
        }
        BlockId::Repeater => {
            let mut props = BTreeMap::new();
            props.insert(
                SmolStr::new("facing"),
                face.unwrap_or_else(|| SmolStr::new("north")),
            );
            props.insert(SmolStr::new("delay"), SmolStr::new("1"));
            props.insert(SmolStr::new("locked"), SmolStr::new("false"));
            props.insert(SmolStr::new("powered"), SmolStr::new("false"));
            BlockState {
                name: SmolStr::new("minecraft:repeater"),
                properties: props,
            }
        }
        BlockId::Comparator => {
            let mut props = BTreeMap::new();
            props.insert(
                SmolStr::new("facing"),
                face.unwrap_or_else(|| SmolStr::new("north")),
            );
            props.insert(SmolStr::new("mode"), SmolStr::new("compare"));
            props.insert(SmolStr::new("powered"), SmolStr::new("false"));
            BlockState {
                name: SmolStr::new("minecraft:comparator"),
                properties: props,
            }
        }
        BlockId::Lever => {
            let mut props = BTreeMap::new();
            props.insert(SmolStr::new("face"), SmolStr::new("floor"));
            props.insert(
                SmolStr::new("facing"),
                face.unwrap_or_else(|| SmolStr::new("north")),
            );
            props.insert(SmolStr::new("powered"), SmolStr::new("false"));
            BlockState {
                name: SmolStr::new("minecraft:lever"),
                properties: props,
            }
        }
        BlockId::RedstoneLamp => {
            let mut props = BTreeMap::new();
            props.insert(SmolStr::new("lit"), SmolStr::new("false"));
            BlockState {
                name: SmolStr::new("minecraft:redstone_lamp"),
                properties: props,
            }
        }
        // Forward-compat: any future BlockId variant should be added
        // here. Until then we render it as air so we never panic on
        // input we don't recognise.
        _ => BlockState::simple("minecraft:air"),
    }
}

/// Palette builder with first-encounter index assignment.
///
/// Index 0 is **always** `AIR` (per `contracts/litematic-nbt.md`).
/// Subsequent indices are assigned in the order [`insert`] sees each
/// unique [`BlockState`].
#[derive(Debug, Clone)]
pub struct Palette {
    entries: Vec<BlockState>,
    by_state: BTreeMap<BlockState, u32>,
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

impl Palette {
    /// Create a palette pre-populated with index 0 = `minecraft:air`.
    pub fn new() -> Self {
        let air = BlockState::simple("minecraft:air");
        let mut by_state = BTreeMap::new();
        by_state.insert(air.clone(), 0u32);
        Self {
            entries: vec![air],
            by_state,
        }
    }

    /// Return the index for `state`, inserting it if new.
    pub fn insert(&mut self, state: BlockState) -> u32 {
        if let Some(&idx) = self.by_state.get(&state) {
            return idx;
        }
        let idx = self.entries.len() as u32;
        self.entries.push(state.clone());
        self.by_state.insert(state, idx);
        idx
    }

    /// Number of palette entries (including AIR at index 0).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True iff only AIR is present.
    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1
    }

    /// Entries in index order, suitable for direct NBT emission.
    pub fn entries(&self) -> &[BlockState] {
        &self.entries
    }

    /// Compute the number of bits needed to address every palette entry.
    /// Always at least 2 per the Litematica spec.
    pub fn bits_per_index(&self) -> u32 {
        let n = self.entries.len().max(2) as u64;
        let mut bits = 64u32 - n.next_power_of_two().leading_zeros();
        bits = bits.saturating_sub(1);
        bits.max(2)
    }
}
