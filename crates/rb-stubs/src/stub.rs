//! Core stub data model.

use std::collections::BTreeMap;

use rb_core::{Bbox3, Direction, Pos3};
use rb_nbt::BlockState;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Direction of a stub port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PortDir {
    /// Signal flows from the surrounding module INTO the stub.
    In,
    /// Signal flows from the stub OUTWARDS into the surrounding module.
    Out,
}

/// One named, possibly multi-bit port on a stub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StubPort {
    /// Logical name (e.g. `"a"`, `"sum"`, `"clk"`). Lowercased.
    pub name: SmolStr,
    /// Whether driven into the stub or driven out.
    pub dir: PortDir,
    /// Number of bits. 1 for scalar ports.
    pub width: u32,
    /// World position of pin index `0`. Pin `i` is at
    /// `start + step * i` for some `step` (typically axis-aligned).
    pub pin_start: Pos3,
    /// Step vector between consecutive pin positions. For 1-bit
    /// ports this is `Pos3::ORIGIN`.
    pub pin_step: Pos3,
    /// Direction the port "faces" — the direction the router must
    /// approach this pin from (towards the stub for inputs, away
    /// from the stub for outputs).
    pub facing: Direction,
}

impl StubPort {
    /// World position of the `i`-th pin (0-indexed). Panics if
    /// `i >= width`; bounds are caller-guaranteed.
    pub fn pin_pos(&self, i: u32) -> Pos3 {
        debug_assert!(i < self.width, "pin index {i} >= width {}", self.width);
        Pos3::new(
            self.pin_start.x + self.pin_step.x * (i as i32),
            self.pin_start.y + self.pin_step.y * (i as i32),
            self.pin_start.z + self.pin_step.z * (i as i32),
        )
    }
}

/// A hand-built schematic primitive.
///
/// Coordinates inside `blocks` are stub-local; the placer translates
/// them to world coords when materialising. Pins coordinates inside
/// [`StubPort`] are likewise stub-local.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stub {
    /// Stub name (read from the schematic-name sign). E.g. `"and_h8b"`.
    pub name: SmolStr,
    /// Stub-local bounding box derived from the glass corners.
    pub bbox: Bbox3,
    /// All ports keyed by name; ordering preserved by `Vec` for
    /// deterministic iteration.
    pub ports: Vec<StubPort>,
    /// Core circuit blocks (glass corner markers and port-pin signs
    /// removed). Keys are stub-local positions.
    pub blocks: BTreeMap<Pos3, BlockState>,
}

impl Stub {
    /// Find a port by name (case-insensitive). Returns `None` if
    /// no such port exists.
    pub fn port(&self, name: &str) -> Option<&StubPort> {
        self.ports
            .iter()
            .find(|p| p.name.as_str().eq_ignore_ascii_case(name))
    }

    /// Convenience: width along each axis.
    pub fn size(&self) -> (i32, i32, i32) {
        (
            self.bbox.max.x - self.bbox.min.x + 1,
            self.bbox.max.y - self.bbox.min.y + 1,
            self.bbox.max.z - self.bbox.min.z + 1,
        )
    }
}

/// A collection of loaded stubs keyed by `name`. Multiple stubs may
/// be discovered from a `stub_lib/` tree; later loads with the same
/// name shadow earlier ones (so `opt/` can override `core/`).
#[derive(Debug, Default, Clone)]
pub struct StubLibrary {
    stubs: BTreeMap<SmolStr, Stub>,
}

impl StubLibrary {
    /// Empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a stub (overwriting any prior entry with the same name).
    pub fn insert(&mut self, stub: Stub) {
        self.stubs.insert(stub.name.clone(), stub);
    }

    /// Lookup by name (case-insensitive). Returns `None` if absent.
    pub fn get(&self, name: &str) -> Option<&Stub> {
        let lc = name.to_ascii_lowercase();
        self.stubs.get(lc.as_str())
    }

    /// Iterate over all stubs (sorted by name).
    pub fn iter(&self) -> impl Iterator<Item = &Stub> {
        self.stubs.values()
    }

    /// How many stubs are loaded.
    pub fn len(&self) -> usize {
        self.stubs.len()
    }

    /// Whether the library has no stubs.
    pub fn is_empty(&self) -> bool {
        self.stubs.is_empty()
    }

    /// All stub names, sorted.
    pub fn names(&self) -> Vec<&str> {
        self.stubs.keys().map(|k| k.as_str()).collect()
    }
}
