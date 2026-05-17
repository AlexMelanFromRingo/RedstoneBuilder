//! Sparse 3D grid keyed by [`Pos3`].
//!
//! Used by the router's obstruction map and as a working buffer when
//! building the final block layout.

use std::collections::HashMap;

use rb_core::Pos3;
use serde::Serialize;

/// What occupies a given cell in the router's obstruction grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum CellState {
    /// Cell is free.
    #[default]
    Air,
    /// Cell hosts a non-redstone block placed by a macro-cell, or the
    /// support block under a routed dust segment.
    Solid,
    /// Cell hosts a redstone dust segment for the given net.
    Dust(NetTag),
    /// Cell hosts a repeater for the given net.
    Repeater(NetTag),
    /// Cell is reserved as an adjacency shadow of a `Dust` cell owned
    /// by the given net — no foreign dust may step here.
    Obstructed(NetTag),
}

/// A `NetId`-equivalent newtype kept here to avoid a circular dep.
/// In practice this is just a wrapper over `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct NetTag(pub u32);

/// Sparse 3D grid. Missing cells are considered [`CellState::Air`].
#[derive(Debug, Clone, Default)]
pub struct Grid3D {
    cells: HashMap<Pos3, CellState>,
}

impl Grid3D {
    /// Create an empty grid.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the current state at `p`.
    pub fn get(&self, p: Pos3) -> CellState {
        self.cells.get(&p).copied().unwrap_or(CellState::Air)
    }

    /// Write `state` at `p`, returning the previous state.
    pub fn set(&mut self, p: Pos3, state: CellState) -> CellState {
        let prev = self.get(p);
        if state == CellState::Air {
            self.cells.remove(&p);
        } else {
            self.cells.insert(p, state);
        }
        prev
    }

    /// True iff `p` is either Air or already owned by `net`.
    pub fn is_passable_for(&self, p: Pos3, net: NetTag) -> bool {
        match self.get(p) {
            CellState::Air => true,
            CellState::Solid => false,
            CellState::Dust(n) | CellState::Repeater(n) | CellState::Obstructed(n) => n == net,
        }
    }

    /// Iterate over occupied cells in deterministic `(y, z, x)` order.
    pub fn iter_sorted(&self) -> impl Iterator<Item = (Pos3, CellState)> + '_ {
        let mut entries: Vec<(Pos3, CellState)> =
            self.cells.iter().map(|(&p, &s)| (p, s)).collect();
        entries.sort_by_key(|(p, _)| (p.y, p.z, p.x));
        entries.into_iter()
    }

    /// Number of populated (non-Air) cells.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// True iff nothing is populated.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}
