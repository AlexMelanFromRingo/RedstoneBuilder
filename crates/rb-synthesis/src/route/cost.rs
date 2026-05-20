//! v2 cost map for the A* + PathFinder router.
//!
//! Captures the vertical asymmetry of vanilla MC redstone (FR-V18):
//! dust on top of a slab transmits **upward** to the cell above with
//! low cost, but **does not** transmit downward to the cell below.
//! The cost map encodes this via per-cell `VerticalCost` attributes
//! that the edge-cost function consults.

use std::collections::{BTreeMap, BTreeSet};

use rb_core::{Bbox3, Direction, Pos3};
use serde::Serialize;

use crate::grid3d::NetTag;

/// Per-cell physical attributes for the cost map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct CellAttrs {
    /// True for cells redstone dust may occupy.
    pub base_passable: bool,
    /// Vertical-transmission semantics.
    pub vertical: VerticalCost,
}

/// Vertical-transmission semantics of a cell (FR-V18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum VerticalCost {
    /// Ordinary support block: dust on top transmits to neighbours at
    /// the same Y, and (subject to MC rules) to neighbours at Y ± 1.
    #[default]
    Symmetric,
    /// Slab: dust on top transmits **upward** to dust above; does NOT
    /// transmit downward. The downward edge is forbidden.
    UpOnly,
    /// Glass: dust does not couple electrically across the cell —
    /// router uses these for wire crossings.
    GlassCross,
    /// Solid block hosting a placed gate's internal block: foreign
    /// nets cannot pass.
    Blocked,
}

/// Cost-map state for one PathFinder iteration.
#[derive(Debug, Clone)]
pub struct CostMap {
    /// Active routing region.
    pub bounds: Bbox3,
    /// Per-cell attributes (sparse — missing cells default to
    /// `CellAttrs { base_passable: true, vertical: Symmetric }`).
    pub cells: BTreeMap<Pos3, CellAttrs>,
    /// PathFinder per-iteration congestion penalty per cell.
    pub present_cost: BTreeMap<Pos3, u16>,
    /// PathFinder accumulated-congestion penalty per cell.
    pub history_cost: BTreeMap<Pos3, u16>,
    /// Owner of each routed cell in the current iteration (set during
    /// commit; cleared at the start of each PathFinder iteration).
    pub assigned: BTreeMap<Pos3, NetTag>,
    /// Adjacency-isolation shadow: clones the v1 `Grid3D::Obstructed`
    /// semantics — when net `N` commits cell `c`, the 6 orthogonal
    /// neighbours of `c` get tagged here so that no FOREIGN net may
    /// route through them. Without this, two parallel dust runs of
    /// different nets in adjacent cells short to each other in real
    /// MC (dust auto-connects to any redstone-conductive neighbour).
    /// Multiple nets may shadow the same cell — we keep the first one
    /// to claim it; the second net is already locked out by then.
    pub shadows: BTreeMap<Pos3, NetTag>,
    /// Pin-cell allowlist: any net is allowed to enter/exit these
    /// cells regardless of `shadows` — they are stub terminals where
    /// the stub itself enforces isolation, so neighbouring pins of
    /// different nets do not short. PathFinder pre-populates this
    /// with every route's source and sink before iterating.
    pub pins: BTreeSet<Pos3>,
}

impl CostMap {
    /// Empty cost map with no special cells.
    pub fn new(bounds: Bbox3) -> Self {
        Self {
            bounds,
            cells: BTreeMap::new(),
            present_cost: BTreeMap::new(),
            history_cost: BTreeMap::new(),
            assigned: BTreeMap::new(),
            shadows: BTreeMap::new(),
            pins: BTreeSet::new(),
        }
    }

    /// Register `p` as a stub-pin terminal (exempt from adjacency
    /// shadow). Idempotent.
    pub fn mark_pin(&mut self, p: Pos3) {
        self.pins.insert(p);
    }

    /// Mark cell `p` as part of net `net`'s adjacency shadow. Idempotent
    /// per (cell, net); the first net to claim a cell wins. A cell that
    /// is also `assigned` to the same net keeps that ownership — shadow
    /// only blocks FOREIGN nets, not the net that placed the dust.
    pub fn mark_shadow(&mut self, p: Pos3, net: NetTag) {
        if self.assigned.get(&p) == Some(&net) {
            return;
        }
        self.shadows.entry(p).or_insert(net);
    }

    /// Mark a cell as a placed-block obstruction (foreign nets can't
    /// route through it). Used by the pipeline to seed the map from
    /// the placement.
    pub fn mark_blocked(&mut self, p: Pos3) {
        self.cells.insert(
            p,
            CellAttrs {
                base_passable: false,
                vertical: VerticalCost::Blocked,
            },
        );
    }

    /// Mark a cell as a slab (upward-only vertical transmission).
    pub fn mark_slab(&mut self, p: Pos3) {
        self.cells.insert(
            p,
            CellAttrs {
                base_passable: true,
                vertical: VerticalCost::UpOnly,
            },
        );
    }

    /// Mark a cell as glass (orthogonal nets cross without coupling).
    pub fn mark_glass(&mut self, p: Pos3) {
        self.cells.insert(
            p,
            CellAttrs {
                base_passable: true,
                vertical: VerticalCost::GlassCross,
            },
        );
    }

    /// Look up the attrs at `p` (returns the default if unset).
    pub fn attrs_at(&self, p: Pos3) -> CellAttrs {
        self.cells.get(&p).copied().unwrap_or(CellAttrs {
            base_passable: true,
            vertical: VerticalCost::Symmetric,
        })
    }

    /// Tunables for the A* edge-cost function.
    pub fn cost_for_edge(
        &self,
        from: Pos3,
        prev_dir: Option<Direction>,
        dir: Direction,
        to: Pos3,
        net: NetTag,
        cfg: &EdgeCostConfig,
    ) -> u32 {
        let to_attrs = self.attrs_at(to);

        // Hard block: foreign nets cannot traverse Blocked cells —
        // except pin terminals, which mark routable cells the placer
        // could not avoid stamping on (anchor pins typically sit on
        // top of the placer's support stones).
        if !to_attrs.base_passable && !self.pins.contains(&to) {
            if let Some(owner) = self.assigned.get(&to) {
                if *owner != net {
                    return u32::MAX;
                }
            } else {
                return u32::MAX;
            }
        }

        // Foreign net may not enter a cell already owned by another
        // net's committed dust this iteration. Pin terminals are
        // exempt (their owners are unknown until commit; a shared pin
        // cell is benign — both nets converge there). Without this
        // block, PathFinder's overuse-detection cannot be resolved by
        // history-cost: A* would happily share cells with a foreign
        // net since the edge cost is the same as for an empty cell.
        if let Some(owner) = self.assigned.get(&to) {
            if *owner != net && !self.pins.contains(&to) {
                return u32::MAX;
            }
        }

        // Adjacency shadow: a cell next to another net's committed dust
        // would short to that dust in real MC, so we forbid the edge.
        // Cells owned by `net` itself are fine — we never shadow our own.
        // Pin terminals are exempt: stubs isolate their own outputs, so
        // foreign nets are allowed to enter a pin even if it lies in
        // another net's shadow halo.
        if !self.pins.contains(&to) {
            if let Some(owner) = self.shadows.get(&to) {
                if *owner != net && self.assigned.get(&to) != Some(&net) {
                    return u32::MAX;
                }
            }
        }

        // FR-V18: slab UpOnly forbids descent through it.
        if matches!(dir, Direction::Down) {
            let from_attrs = self.attrs_at(from);
            if matches!(from_attrs.vertical, VerticalCost::UpOnly) {
                return u32::MAX;
            }
        }

        let base = match dir {
            Direction::Up | Direction::Down => cfg.level_change_base,
            _ => cfg.straight_base,
        };
        let turn = match (prev_dir, dir) {
            (Some(p), d) if p == d => 0,
            (Some(_), _) => cfg.turn_penalty,
            (None, _) => 0,
        };
        let vertical = match dir {
            Direction::Up => cfg.up_penalty,
            Direction::Down => cfg.down_penalty,
            _ => 0,
        };
        let present = u32::from(self.present_cost.get(&to).copied().unwrap_or(0));
        let history = u32::from(self.history_cost.get(&to).copied().unwrap_or(0));
        base + turn + vertical + present + history
    }

    /// Bump the congestion-history of `cells` by `delta`.
    pub fn bump_history(&mut self, cells: impl IntoIterator<Item = Pos3>, delta: u16) {
        for p in cells {
            let entry = self.history_cost.entry(p).or_insert(0);
            *entry = entry.saturating_add(delta);
        }
    }

    /// Bump the per-iteration congestion of `cells` by `delta`.
    pub fn bump_present(&mut self, cells: impl IntoIterator<Item = Pos3>, delta: u16) {
        for p in cells {
            let entry = self.present_cost.entry(p).or_insert(0);
            *entry = entry.saturating_add(delta);
        }
    }

    /// Clear per-iteration state (`assigned`, `present_cost`, `shadows`)
    /// ahead of the next PathFinder iteration. `history_cost` is
    /// preserved — that's the whole point of negotiated congestion.
    pub fn clear_iteration_state(&mut self) {
        self.assigned.clear();
        self.present_cost.clear();
        self.shadows.clear();
    }
}

/// Tunables for [`CostMap::cost_for_edge`].
#[derive(Debug, Clone, Copy)]
pub struct EdgeCostConfig {
    /// Base cost for a lateral straight step (north/south/east/west).
    pub straight_base: u32,
    /// Base cost for a vertical step (up/down).
    pub level_change_base: u32,
    /// Extra cost when changing direction.
    pub turn_penalty: u32,
    /// Extra cost for stepping downward.
    pub down_penalty: u32,
    /// Extra cost for stepping upward (requires slab).
    pub up_penalty: u32,
}

impl EdgeCostConfig {
    /// Default values from `specs/002-v2-analog-scale/data-model.md`.
    pub const DEFAULT: Self = Self {
        straight_base: 1,
        level_change_base: 3,
        turn_penalty: 4,
        down_penalty: 6,
        up_penalty: 10,
    };
}
