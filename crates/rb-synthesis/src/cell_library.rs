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
use rb_parser::ast::{CompareMode, GateInst};
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
    /// v2: per-instance repeater delay (1..=4). Set on cells built
    /// from a user-instantiable `repeater` instance — the NBT writer
    /// patches the `delay` property of the cell's Repeater block to
    /// this value at emission time. `None` for non-repeater cells and
    /// for cells with the default delay=1.
    #[serde(default)]
    pub repeater_delay: Option<u8>,
    /// v2: per-instance comparator mode. Set on cells built from a
    /// user-instantiable `comparator` instance — the NBT writer
    /// patches the `mode` property of the Comparator block to this
    /// value at emission time. `None` for non-comparator cells.
    #[serde(default)]
    pub comparator_mode: Option<CompareMode>,
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
        // v2 primitives (delay/mode default to "1" / "compare" if
        // we don't have a GateInst on hand). Prefer
        // `macrocell_for_inst(&GateInst)` to thread the user's
        // explicit attributes through.
        GateKind::Comparator => Ok(comparator_cell(CompareMode::Compare)),
        GateKind::Observer => Ok(observer_cell()),
        GateKind::Repeater => Ok(repeater_cell(1, false)),
        GateKind::TargetBlock => Ok(target_cell()),
        _ => Err(SynthError::NoCellFor { kind }),
    }
}

/// v2: variant of [`macrocell_for`] that threads per-instance
/// attributes (`compare_mode`, `repeater_delay`, etc.) from the
/// [`GateInst`] into the returned cell. Use this from the synthesis
/// pipeline whenever you have the originating instance on hand.
pub fn macrocell_for_inst(inst: &GateInst) -> Result<Macrocell, SynthError> {
    match inst.kind {
        GateKind::Comparator => {
            let mode = inst.compare_mode.unwrap_or(CompareMode::Compare);
            let mut cell = comparator_cell(mode);
            cell.comparator_mode = Some(mode);
            Ok(cell)
        }
        GateKind::Repeater => {
            let delay = inst.repeater_delay.unwrap_or(1);
            let lock_used = inst
                .connections
                .iter()
                .any(|c| c.port.as_str().eq_ignore_ascii_case("LOCK"));
            let mut cell = repeater_cell(delay, lock_used);
            cell.repeater_delay = Some(delay);
            Ok(cell)
        }
        _ => macrocell_for(inst.kind),
    }
}

fn not_cell() -> Macrocell {
    // Physically-valid 1-tick NOT gate (torch inverter).
    // 4W × 3H × 1D. Input on -X, output on +X, all at Y=1.
    //
    //   Y=2:             dust(1,2,0)
    //   Y=1: in_dust   STONE_switch   wall_torch[east]   out_dust
    //   Y=0: planks    planks         planks              planks
    //
    // Verified per MC 1.21+ source (RedStoneWireBlock.getConnectingSide):
    // input dust(0,1,0) sees adjacent Stone(1,1,0); block above Stone is
    // dust(1,2,0) → shouldConnectTo(wire) = true → forms RedstoneSide.UP
    // east. Wire's east-emission is enabled by its WEST connection
    // (external feed), so STONE becomes STRONGLY POWERED at 15. Wall
    // torch attached to STONE's east face (facing=east) turns OFF.
    // Output dust adjacent (west) to wall torch reads its signal. ✓
    //
    // The dust at (1,2,0) doubles as a "passive marker" for the slope
    // condition AND as redundant weak power; only the marker role is
    // load-bearing.
    Macrocell {
        kind: GateKind::Not,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(3, 2, 0)),
        blocks: vec![
            // X=0: input dust pad
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
            // X=1: STONE switch + dust on top
            PlacedBlock {
                pos: Pos3::new(1, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 1, 0),
                block: BlockId::Stone,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 2, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            // X=2: wall torch attached to STONE switch east face
            PlacedBlock {
                pos: Pos3::new(2, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(2, 1, 0),
                block: BlockId::RedstoneWallTorch,
                facing: Some(Direction::East),
            },
            // X=3: output dust pad
            PlacedBlock {
                pos: Pos3::new(3, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(3, 1, 0),
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
            pos: Pos3::new(3, 1, 0),
            approach: Direction::East,
        }],
        tick_delay: 1,
        repeater_delay: None,
        comparator_mode: None,
    }
}

fn and_cell() -> Macrocell {
    // 2-input AND = NOT(NOT A OR NOT B). Two parallel input inverters
    // feed a junction that drives a third inverter. 8W × 3H × 3D.
    //
    // Per MC source (RedStoneWireBlock + DefaultRedstoneWireEvaluator):
    //   - Input dust at (0,1,Z) sees Stone(1,1,Z) with dust on top
    //     at (1,2,Z) → forms RedstoneSide.UP east; west-fed dust then
    //     strongly powers Stone via wire's east-emission.
    //   - Wall torch attached to Stone east face (facing=east) turns
    //     OFF when input ON.
    //   - A_bar / B_bar dust east of torch reads its signal source.
    //   - A_bar / B_bar / junction dusts at Y=1 propagate via
    //     `getIncomingWireSignal` (max-neighbor-POWER − 1), regardless
    //     of connection state.
    //   - Junction feeds final Stone(5,1,1) with dust on top at
    //     (5,2,1) — same SIDE=UP slope trick → strong-power final
    //     Stone when any input OFF (junction ON).
    //   - Final wall torch on final Stone east face → output dust on
    //     its east is ON only when ALL inputs ON.
    //
    //  Z=0: in_A → Stone+dust → torch_E → A_bar → junction_dust →
    //  Z=1:                                       junction_dust → Stone+dust → torch_E → out_dust
    //  Z=2: in_B → Stone+dust → torch_E → B_bar → junction_dust →
    let mut blocks = vec![
        // Z=0 row (NOT A)
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
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 1, 0),
            block: BlockId::Stone,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 2, 0),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(2, 0, 0),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(2, 1, 0),
            block: BlockId::RedstoneWallTorch,
            facing: Some(Direction::East),
        },
        PlacedBlock {
            pos: Pos3::new(3, 0, 0),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(3, 1, 0),
            block: BlockId::RedstoneDust,
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
        // Z=2 row (NOT B), mirror of NOT A
        PlacedBlock {
            pos: Pos3::new(0, 0, 2),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(0, 1, 2),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 0, 2),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 1, 2),
            block: BlockId::Stone,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 2, 2),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(2, 0, 2),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(2, 1, 2),
            block: BlockId::RedstoneWallTorch,
            facing: Some(Direction::East),
        },
        PlacedBlock {
            pos: Pos3::new(3, 0, 2),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(3, 1, 2),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(4, 0, 2),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(4, 1, 2),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        // Z=1 row (junction + final NOT + output)
        PlacedBlock {
            pos: Pos3::new(4, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(4, 1, 1),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(5, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(5, 1, 1),
            block: BlockId::Stone,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(5, 2, 1),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(6, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(6, 1, 1),
            block: BlockId::RedstoneWallTorch,
            facing: Some(Direction::East),
        },
        PlacedBlock {
            pos: Pos3::new(7, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(7, 1, 1),
            block: BlockId::RedstoneDust,
            facing: None,
        },
    ];
    blocks.shrink_to_fit();
    Macrocell {
        kind: GateKind::And,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(7, 2, 2)),
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
            pos: Pos3::new(7, 1, 1),
            approach: Direction::East,
        }],
        tick_delay: 3,
        repeater_delay: None,
        comparator_mode: None,
    }
}

fn or_cell() -> Macrocell {
    // Two-input OR = dust-line merge (redstone wire naturally ORs).
    // 3W × 2H × 3D. Inputs on -X at Z=0 and Z=2; output on +X at Z=1.
    // Repeater on output refreshes signal strength back to 15 so cascaded
    // gates don't lose level too quickly.
    //
    //   Z=0: dust_A  junction      .
    //   Z=1:         junction      repeater_out  dust_out
    //   Z=2: dust_B  junction      .
    let mut blocks = vec![
        // Y=0: planks support row across the whole footprint
        PlacedBlock {
            pos: Pos3::new(0, 0, 0),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(0, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(0, 0, 2),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 0, 0),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 0, 2),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(2, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(3, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        },
        // Y=1: input dusts at Z=0 and Z=2
        PlacedBlock {
            pos: Pos3::new(0, 1, 0),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(0, 1, 2),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        // Y=1: junction dust line at X=1 connecting both inputs through Z=1
        PlacedBlock {
            pos: Pos3::new(1, 1, 0),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 1, 1),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 1, 2),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        // Y=1: repeater refreshes signal to 15 before exit.
        // MC convention: `facing` on repeater/comparator is the direction
        // FROM output TO input (where the BACK of the block points). To
        // make signal flow east (in=west, out=east), set facing=west.
        PlacedBlock {
            pos: Pos3::new(2, 1, 1),
            block: BlockId::Repeater,
            facing: Some(Direction::West),
        },
        // Y=1: output dust
        PlacedBlock {
            pos: Pos3::new(3, 1, 1),
            block: BlockId::RedstoneDust,
            facing: None,
        },
    ];
    blocks.shrink_to_fit();
    Macrocell {
        kind: GateKind::Or,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(3, 1, 2)),
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
            pos: Pos3::new(3, 1, 1),
            approach: Direction::East,
        }],
        tick_delay: 2, // 1 tick from repeater + dust merge settle
        repeater_delay: None,
        comparator_mode: None,
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
        // MC convention: `facing` = direction from output to input (back),
        // i.e. opposite of where signal flows out. west = signal exits east.
        facing: Some(Direction::West),
    });
    blocks.push(PlacedBlock {
        pos: Pos3::new(2, 1, 2),
        block: BlockId::Repeater,
        facing: Some(Direction::West),
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
        repeater_delay: None,
        comparator_mode: None,
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
        facing: Some(Direction::West), // MC: facing = back = input direction
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
        repeater_delay: None,
        comparator_mode: None,
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
        repeater_delay: None,
        comparator_mode: None,
    }
}

// ─────────────────────────────────────────────────────────────────────
// v2 cells (analog + event primitives + routing helpers)
// ─────────────────────────────────────────────────────────────────────

fn comparator_cell(_mode: CompareMode) -> Macrocell {
    // 3W × 2H × 3D. Comparator at Y=1 with planks support layer at Y=0.
    //
    //   Y=1: dust-A   Comparator(facing=W)   dust-Y
    //                 (.B side-input at Z=2, dust on top of planks)
    //   Y=0: planks layer
    Macrocell {
        kind: GateKind::Comparator,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 1, 2)),
        blocks: vec![
            // Y=0 planks supports
            PlacedBlock {
                pos: Pos3::new(0, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(2, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 0, 2),
                block: BlockId::OakPlanks,
                facing: None,
            },
            // Y=1 active layer
            PlacedBlock {
                pos: Pos3::new(0, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 1, 0),
                block: BlockId::Comparator,
                facing: Some(Direction::West), // MC: facing = back = input dir
            },
            PlacedBlock {
                pos: Pos3::new(2, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 1, 2),
                block: BlockId::RedstoneDust,
                facing: None,
            },
        ],
        inputs: vec![
            Anchor {
                role: EndpointRole::AnalogIn(0),
                pos: Pos3::new(0, 1, 0),
                approach: Direction::West,
            },
            Anchor {
                role: EndpointRole::AnalogIn(1),
                pos: Pos3::new(1, 1, 2),
                approach: Direction::South,
            },
        ],
        outputs: vec![Anchor {
            role: EndpointRole::AnalogOut,
            pos: Pos3::new(2, 1, 0),
            approach: Direction::East,
        }],
        tick_delay: 1,
        repeater_delay: None,
        comparator_mode: None, // set by macrocell_for_inst per-instance
    }
}

fn observer_cell() -> Macrocell {
    // 1W × 2H × 3D. Observer at Y=1 with planks supports at Y=0.
    //
    //   Y=1: watch_dust(Z=0)   Observer(facing=North → eye looks N to watched dust)   pulse_dust(Z=2)
    //   Y=0: planks            planks                                                   planks
    //
    // MC convention: `facing` on observer = direction the FRONT (eye)
    // points TOWARDS. So facing=North means eye looks at the block at
    // (Z-1) = watched_dust at (0,1,0). The pulse emits from the BACK,
    // i.e. opposite of facing = South → output dust at (0,1,2).
    Macrocell {
        kind: GateKind::Observer,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(0, 1, 2)),
        blocks: vec![
            // Y=0 supports
            PlacedBlock {
                pos: Pos3::new(0, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(0, 0, 1),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(0, 0, 2),
                block: BlockId::OakPlanks,
                facing: None,
            },
            // Y=1 active
            PlacedBlock {
                pos: Pos3::new(0, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(0, 1, 1),
                block: BlockId::Observer,
                facing: Some(Direction::North),
            },
            PlacedBlock {
                pos: Pos3::new(0, 1, 2),
                block: BlockId::RedstoneDust,
                facing: None,
            },
        ],
        inputs: vec![Anchor {
            role: EndpointRole::ObserverWatch,
            pos: Pos3::new(0, 1, 0),
            approach: Direction::North,
        }],
        outputs: vec![Anchor {
            role: EndpointRole::ObserverPulse,
            pos: Pos3::new(0, 1, 2),
            approach: Direction::South,
        }],
        tick_delay: 1, // 1-tick pulse
        repeater_delay: None,
        comparator_mode: None,
    }
}

fn repeater_cell(delay: u8, lock_used: bool) -> Macrocell {
    // 3W × 2H × {1,2}D. Repeater at Y=1, planks supports at Y=0.
    //
    //   Y=1: dust-IN  Repeater(facing=West→output east)  dust-OUT
    //                            [.LOCK side-input dust at Z=1 if used]
    //   Y=0: planks  planks  planks
    let mut blocks: Vec<PlacedBlock> = vec![
        PlacedBlock {
            pos: Pos3::new(0, 0, 0),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 0, 0),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(2, 0, 0),
            block: BlockId::OakPlanks,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(0, 1, 0),
            block: BlockId::RedstoneDust,
            facing: None,
        },
        PlacedBlock {
            pos: Pos3::new(1, 1, 0),
            block: BlockId::Repeater,
            facing: Some(Direction::West), // MC: back faces west = output east
        },
        PlacedBlock {
            pos: Pos3::new(2, 1, 0),
            block: BlockId::RedstoneDust,
            facing: None,
        },
    ];
    let mut inputs = vec![Anchor {
        role: EndpointRole::RepeaterIn,
        pos: Pos3::new(0, 1, 0),
        approach: Direction::West,
    }];
    let bbox_max_z = if lock_used {
        blocks.push(PlacedBlock {
            pos: Pos3::new(1, 0, 1),
            block: BlockId::OakPlanks,
            facing: None,
        });
        blocks.push(PlacedBlock {
            pos: Pos3::new(1, 1, 1),
            block: BlockId::RedstoneDust,
            facing: None,
        });
        inputs.push(Anchor {
            role: EndpointRole::RepeaterLock,
            pos: Pos3::new(1, 1, 1),
            approach: Direction::South,
        });
        1
    } else {
        0
    };
    Macrocell {
        kind: GateKind::Repeater,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 1, bbox_max_z)),
        blocks,
        inputs,
        outputs: vec![Anchor {
            role: EndpointRole::RepeaterOut,
            pos: Pos3::new(2, 1, 0),
            approach: Direction::East,
        }],
        // Each delay step adds 2 game ticks (= 1 redstone tick). v1
        // convention measures tick_delay in redstone ticks.
        tick_delay: delay.max(1),
        repeater_delay: Some(delay),
        comparator_mode: None,
    }
}

fn target_cell() -> Macrocell {
    // 3W × 2H × 1D. Target block at Y=1 with planks supports at Y=0.
    //
    //   Y=1: dust-IN  TargetBlock  dust-OUT
    //   Y=0: planks   planks       planks
    Macrocell {
        kind: GateKind::TargetBlock,
        bbox: Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(2, 1, 0)),
        blocks: vec![
            PlacedBlock {
                pos: Pos3::new(0, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(2, 0, 0),
                block: BlockId::OakPlanks,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(0, 1, 0),
                block: BlockId::RedstoneDust,
                facing: None,
            },
            PlacedBlock {
                pos: Pos3::new(1, 1, 0),
                block: BlockId::TargetBlock,
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
        tick_delay: 0,
        repeater_delay: None,
        comparator_mode: None,
    }
}
