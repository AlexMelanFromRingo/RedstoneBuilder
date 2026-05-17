//! Macro-cell library for the combinational primitives (US1).
//!
//! Each cell is defined in a *local* coordinate frame where +X is the
//! cell's "output direction", +Y is up, +Z is into the build. The placer
//! rotates and translates the local-frame block list into world
//! coordinates.
//!
//! **Important caveat**: these block patterns are good-faith
//! redstone-engineering approximations. Each one parses, types,
//! places, and routes cleanly — but in-game functional verification
//! against MC Java Edition 26.1 vanilla redstone semantics is a
//! Polish-phase task (T078). If a cell turns out to be wrong in-game,
//! only this file needs changing.

use rb_core::{Bbox3, BlockId, Direction, GateKind, Pos3};
use serde::Serialize;

use crate::error::SynthError;
use crate::netlist::EndpointRole;

/// One block placed inside a macro-cell (in cell-local coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlacedBlock {
    /// Position in cell-local coords.
    pub pos: Pos3,
    /// Which Minecraft primitive to put here.
    pub block: BlockId,
    /// `facing` block-state property, if the block needs one.
    pub facing: Option<Direction>,
}

/// One I/O attachment point for a macro-cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Anchor {
    /// Which port this anchor represents on the gate.
    pub role: EndpointRole,
    /// Position in cell-local coords where the router connects to it.
    pub pos: Pos3,
    /// Direction the router must approach from.
    pub approach: Direction,
}

/// A complete macro-cell: bbox, block layout, anchors, and timing.
#[derive(Debug, Clone, Serialize)]
pub struct Macrocell {
    /// Which primitive this cell implements.
    pub kind: GateKind,
    /// Cell-local bounding box. `min` is always `(0,0,0)`.
    pub bbox: Bbox3,
    /// Blocks that make up the cell (in cell-local coords).
    pub blocks: Vec<PlacedBlock>,
    /// Input anchors.
    pub inputs: Vec<Anchor>,
    /// Output anchors.
    pub outputs: Vec<Anchor>,
    /// Settle delay, in redstone ticks, from any input change to a
    /// stable output. (For US2 stateful elements this is the
    /// input-to-output propagation; clock-edge behavior is separate.)
    pub tick_delay: u8,
}

/// Look up the macro-cell for a [`GateKind`]. Returns
/// [`SynthError::NoCellFor`] for primitives that v1 doesn't synthesize
/// (US2 / Post-MVP).
pub fn macrocell_for(kind: GateKind) -> Result<Macrocell, SynthError> {
    match kind {
        GateKind::Not => Ok(not_cell()),
        GateKind::And => Ok(and_cell()),
        GateKind::Or => Ok(or_cell()),
        GateKind::Xor => Ok(xor_cell()),
        GateKind::DTrigger => Ok(dtrigger_cell()),
        GateKind::MemoryCell => Ok(memcell_cell()),
        _ => Err(SynthError::NoCellFor { kind }),
    }
}

fn not_cell() -> Macrocell {
    // 3 × 2 × 1 (W × H × D).
    // y=1:     |        | wall_torch(W) |        |
    // y=0:     | dust-in| stone-support | dust-out|
    Macrocell {
        kind: GateKind::Not,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 1, 0)),
        blocks: vec![
            PlacedBlock {
                pos: Pos3::new(0, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(0, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 0, 0),
                block: BlockId::Stone,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 1, 0),
                block: BlockId::RedstoneWallTorch,
                facing: Some(Direction::West),
            },
            PlacedBlock {
                pos: Pos3::new(2, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(2, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
        ],
        inputs: vec![Anchor {
            role: EndpointRole::DataIn(0),
            pos: Pos3::new(0, 1, 0),
            approach: Direction::West,
        }],
        outputs: vec![Anchor {
            role: EndpointRole::DataOut,
            pos: Pos3::new(2, 1, 0),
            approach: Direction::East,
        }],
        tick_delay: 1,
    }
}

fn and_cell() -> Macrocell {
    // 3-wide footprint: two NOTs feeding a NOR (torch tower).
    // 5 × 3 × 1.  Layout (y up, z=0 plane):
    //
    //   y=2:    .            .         dust-out
    //   y=1:    torch_up     torch_up  .
    //   y=0: dust-A solid|dust-B solid|stone
    //
    Macrocell {
        kind: GateKind::And,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(4, 2, 0)),
        blocks: vec![
            PlacedBlock {
                pos: Pos3::new(0, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(0, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 0, 0),
                block: BlockId::Stone,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 1, 0),
                block: BlockId::RedstoneWallTorch,
                facing: Some(Direction::East),
            },
            PlacedBlock {
                pos: Pos3::new(2, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(2, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(3, 0, 0),
                block: BlockId::Stone,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(3, 1, 0),
                block: BlockId::RedstoneTorch,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(3, 2, 0),
                block: BlockId::Stone,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(4, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(4, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
        ],
        inputs: vec![
            Anchor {
                role: EndpointRole::DataIn(0),
                pos: Pos3::new(0, 1, 0),
                approach: Direction::West,
            },
            Anchor {
                role: EndpointRole::DataIn(1),
                pos: Pos3::new(2, 1, 0),
                approach: Direction::South,
            },
        ],
        outputs: vec![Anchor {
            role: EndpointRole::DataOut,
            pos: Pos3::new(4, 1, 0),
            approach: Direction::East,
        }],
        tick_delay: 3,
    }
}

fn or_cell() -> Macrocell {
    // Two dust paths merging into a repeater. 3 × 1 × 2.
    Macrocell {
        kind: GateKind::Or,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 0, 1)),
        blocks: vec![
            PlacedBlock {
                pos: Pos3::new(0, 0, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(0, 0, 1),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 0, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 0, 1),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(2, 0, 0),
                block: BlockId::Repeater,
                facing: Some(Direction::East),
            },
        ],
        inputs: vec![
            Anchor {
                role: EndpointRole::DataIn(0),
                pos: Pos3::new(0, 0, 0),
                approach: Direction::West,
            },
            Anchor {
                role: EndpointRole::DataIn(1),
                pos: Pos3::new(0, 0, 1),
                approach: Direction::West,
            },
        ],
        outputs: vec![Anchor {
            role: EndpointRole::DataOut,
            pos: Pos3::new(2, 0, 0),
            approach: Direction::East,
        }],
        tick_delay: 1,
    }
}

fn dtrigger_cell() -> Macrocell {
    // Edge-triggered D flip-flop (good-faith approximation):
    // a clock-edge pulse detector gates a latch holding D.
    // Bbox 5×2×3.  Anchors:
    //   D   at (0, 1, 0), approach from west
    //   CLK at (0, 1, 2), approach from west
    //   Q   at (4, 1, 1), approach from east
    let mut blocks: Vec<PlacedBlock> = Vec::new();
    for x in 0..5 {
        for z in 0..3 {
            blocks.push(PlacedBlock {
                pos: Pos3::new(x, 0, z),
                block: BlockId::OakPlanks,
                facing: None,
            });
        }
    }
    blocks.push(PlacedBlock {
        pos: Pos3::new(0, 1, 0),
        block: BlockId::RedstoneDust,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(0, 1, 2),
        block: BlockId::RedstoneDust,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(2, 1, 0),
        block: BlockId::Repeater,
        facing: Some(Direction::East),
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(2, 1, 2),
        block: BlockId::Repeater,
        facing: Some(Direction::East),
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(3, 1, 1),
        block: BlockId::RedstoneTorch,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(4, 1, 1),
        block: BlockId::RedstoneDust,
        facing: None,
    });

    Macrocell {
        kind: GateKind::DTrigger,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(4, 1, 2)),
        blocks,
        inputs: vec![
            Anchor {
                role: EndpointRole::DataInStateful,
                pos: Pos3::new(0, 1, 0),
                approach: Direction::West,
            },
            Anchor {
                role: EndpointRole::ClockIn,
                pos: Pos3::new(0, 1, 2),
                approach: Direction::West,
            },
        ],
        outputs: vec![Anchor {
            role: EndpointRole::Q,
            pos: Pos3::new(4, 1, 1),
            approach: Direction::East,
        }],
        tick_delay: 4,
    }
}

fn memcell_cell() -> Macrocell {
    // 1-bit memory: SR latch where set = DATA & WRITE, reset = !DATA & WRITE.
    // Bbox 4×2×3.  Anchors:
    //   DATA  at (0, 1, 0), approach from west
    //   WRITE at (0, 1, 2), approach from west
    //   Q     at (3, 1, 1), approach from east
    let mut blocks: Vec<PlacedBlock> = Vec::new();
    for x in 0..4 {
        for z in 0..3 {
            blocks.push(PlacedBlock {
                pos: Pos3::new(x, 0, z),
                block: BlockId::OakPlanks,
                facing: None,
            });
        }
    }
    blocks.push(PlacedBlock {
        pos: Pos3::new(0, 1, 0),
        block: BlockId::RedstoneDust,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(0, 1, 2),
        block: BlockId::RedstoneDust,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(1, 1, 1),
        block: BlockId::Comparator,
        facing: Some(Direction::East),
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(2, 1, 1),
        block: BlockId::RedstoneDust,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(3, 1, 1),
        block: BlockId::RedstoneDust,
        facing: None,
    });

    Macrocell {
        kind: GateKind::MemoryCell,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(3, 1, 2)),
        blocks,
        inputs: vec![
            Anchor {
                role: EndpointRole::DataInStateful,
                pos: Pos3::new(0, 1, 0),
                approach: Direction::West,
            },
            Anchor {
                role: EndpointRole::WriteEnable,
                pos: Pos3::new(0, 1, 2),
                approach: Direction::West,
            },
        ],
        outputs: vec![Anchor {
            role: EndpointRole::Q,
            pos: Pos3::new(3, 1, 1),
            approach: Direction::East,
        }],
        tick_delay: 2,
    }
}

fn xor_cell() -> Macrocell {
    // Classic redstone XOR: A·!B + !A·B realised with 2 NOTs + 2 ANDs + 1 OR.
    // For v1 we record a coarse 5 × 3 × 3 bounding box and a
    // representative scaffold; the precise block-by-block layout is a
    // Polish-phase tuning task.
    let mut blocks: Vec<PlacedBlock> = Vec::new();
    for x in 0..5 {
        for z in 0..3 {
            blocks.push(PlacedBlock {
                pos: Pos3::new(x, 0, z),
                block: BlockId::OakPlanks,
                facing: None,
            });
        }
    }
    blocks.push(PlacedBlock {
        pos: Pos3::new(0, 1, 0),
        block: BlockId::RedstoneDust,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(0, 1, 2),
        block: BlockId::RedstoneDust,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(2, 1, 1),
        block: BlockId::RedstoneTorch,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(2, 2, 1),
        block: BlockId::Stone,
        facing: None,
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(4, 1, 1),
        block: BlockId::RedstoneDust,
        facing: None,
    });

    Macrocell {
        kind: GateKind::Xor,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(4, 2, 2)),
        blocks,
        inputs: vec![
            Anchor {
                role: EndpointRole::DataIn(0),
                pos: Pos3::new(0, 1, 0),
                approach: Direction::West,
            },
            Anchor {
                role: EndpointRole::DataIn(1),
                pos: Pos3::new(0, 1, 2),
                approach: Direction::West,
            },
        ],
        outputs: vec![Anchor {
            role: EndpointRole::DataOut,
            pos: Pos3::new(4, 1, 1),
            approach: Direction::East,
        }],
        tick_delay: 3,
    }
}
