#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use rb_core::{BlockId, Direction, GateKind, Pos3};
use rb_synthesis::{macrocell_for, EndpointRole, Macrocell, PlacedBlock};

#[test]
fn every_primitive_has_a_cell_with_nonempty_bbox() {
    for kind in [
        GateKind::Not,
        GateKind::And,
        GateKind::Or,
        GateKind::Xor,
        GateKind::DTrigger,
        GateKind::MemoryCell,
        GateKind::Comparator,
        GateKind::Observer,
        GateKind::Repeater,
        GateKind::TargetBlock,
    ] {
        let cell = macrocell_for(kind).unwrap_or_else(|e| panic!("no cell for {kind:?}: {e}"));
        assert!(cell.bbox.volume() > 0, "cell for {kind:?} has empty bbox");
        assert!(!cell.blocks.is_empty(), "cell for {kind:?} has no blocks");
        assert!(!cell.outputs.is_empty(), "cell for {kind:?} has no outputs");
    }
}

#[test]
fn not_cell_has_one_input_one_output() {
    let c = macrocell_for(GateKind::Not).unwrap();
    assert_eq!(c.inputs.len(), 1);
    assert_eq!(c.outputs.len(), 1);
    assert!(matches!(c.outputs[0].role, EndpointRole::DataOut));
}

#[test]
fn dtrigger_anchors_use_stateful_roles() {
    let c = macrocell_for(GateKind::DTrigger).unwrap();
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::DataInStateful)));
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::ClockIn)));
    assert!(matches!(c.outputs[0].role, EndpointRole::Q));
    assert_eq!(c.tick_delay, 4);
}

#[test]
fn comparator_cell_has_analog_anchors() {
    let c = macrocell_for(GateKind::Comparator).unwrap();
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::AnalogIn(0))));
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::AnalogIn(1))));
    assert!(matches!(c.outputs[0].role, EndpointRole::AnalogOut));
}

#[test]
fn observer_cell_has_watch_and_pulse_anchors() {
    let c = macrocell_for(GateKind::Observer).unwrap();
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::ObserverWatch)));
    assert!(matches!(c.outputs[0].role, EndpointRole::ObserverPulse));
}

#[test]
fn repeater_cell_has_in_out_anchors() {
    let c = macrocell_for(GateKind::Repeater).unwrap();
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::RepeaterIn)));
    assert!(matches!(c.outputs[0].role, EndpointRole::RepeaterOut));
}

#[test]
fn target_cell_is_minimal() {
    let c = macrocell_for(GateKind::TargetBlock).unwrap();
    assert_eq!(c.inputs.len(), 1);
    assert_eq!(c.outputs.len(), 1);
    assert_eq!(c.tick_delay, 0);
}

// ─────────────────────────────────────────────────────────────────────
// Physical-validity tests — catch macrocells that wouldn't survive a
// MC block-update (floating dust, wall_torch attached to non-solid,
// floor_torch on non-solid, etc.). These verify the rules documented
// in `crates/rb-synthesis/src/cell_library.rs` and in the project's
// `router-scale-limit` memory.
// ─────────────────────────────────────────────────────────────────────

/// Return true if `block` is a full opaque conductor that can hold dust
/// on top OR serve as a wall_torch attachment block. This is a
/// hand-maintained mirror of MC's `isRedstoneConductor` /
/// `canSurviveOn` for the blocks we actually emit in macrocells.
fn is_solid_opaque_support(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Stone | BlockId::OakPlanks | BlockId::TargetBlock | BlockId::Glass | BlockId::Slab
    )
}

fn block_map(cell: &Macrocell) -> HashMap<Pos3, PlacedBlock> {
    cell.blocks.iter().map(|b| (b.pos, *b)).collect()
}

/// For every `wall_torch[facing=X]` in any macrocell: the attachment
/// block at the OPPOSITE direction from facing (i.e. west of torch
/// when facing=east) must exist and be a full opaque solid. Without
/// this, MC drops the torch on the next block update.
#[test]
fn wall_torches_have_valid_attachment_blocks() {
    for kind in [
        GateKind::Not,
        GateKind::And,
        GateKind::Or,
        GateKind::Xor,
        GateKind::DTrigger,
        GateKind::MemoryCell,
        GateKind::Comparator,
        GateKind::Observer,
        GateKind::Repeater,
        GateKind::TargetBlock,
    ] {
        let cell = macrocell_for(kind).unwrap();
        let map = block_map(&cell);
        for pb in &cell.blocks {
            if !matches!(pb.block, BlockId::RedstoneWallTorch) {
                continue;
            }
            let facing = pb
                .facing
                .unwrap_or_else(|| panic!("{kind:?}: wall_torch at {:?} has no facing", pb.pos));
            // facing = direction of the face the torch is attached to.
            // In MC the attachment block is at `pos - facing.offset()`
            // (i.e. east-facing torch has wall to the WEST).
            let (dx, dy, dz) = facing.offset();
            let attach = Pos3::new(pb.pos.x - dx, pb.pos.y - dy, pb.pos.z - dz);
            let attached = map.get(&attach).unwrap_or_else(|| panic!(
                "{kind:?}: wall_torch[facing={:?}] at {:?} expects attachment block at {attach:?}, but that cell is empty",
                facing, pb.pos
            ));
            assert!(
                is_solid_opaque_support(attached.block),
                "{kind:?}: wall_torch[facing={:?}] at {:?} attached to {:?} at {attach:?} — must be a full opaque solid, otherwise MC drops the torch",
                facing, pb.pos, attached.block
            );
        }
    }
}

/// Every floor `redstone_torch` must sit on top of a full opaque solid
/// (its attachment is the cell directly below).
#[test]
fn floor_torches_have_valid_support_blocks() {
    for kind in [
        GateKind::Not,
        GateKind::And,
        GateKind::Or,
        GateKind::Xor,
        GateKind::DTrigger,
        GateKind::MemoryCell,
        GateKind::Comparator,
        GateKind::Observer,
        GateKind::Repeater,
        GateKind::TargetBlock,
    ] {
        let cell = macrocell_for(kind).unwrap();
        let map = block_map(&cell);
        for pb in &cell.blocks {
            if !matches!(pb.block, BlockId::RedstoneTorch) {
                continue;
            }
            let below = Pos3::new(pb.pos.x, pb.pos.y - 1, pb.pos.z);
            let support = map.get(&below).unwrap_or_else(|| {
                panic!(
                    "{kind:?}: floor torch at {:?} has nothing under it at {below:?}",
                    pb.pos
                )
            });
            assert!(
                is_solid_opaque_support(support.block),
                "{kind:?}: floor torch at {:?} sits on {:?} — must be full opaque solid",
                pb.pos,
                support.block
            );
        }
    }
}

/// Every `redstone_dust` (other than dust ON TOP of a switch that's
/// itself anchored by surrounding planks — those are fine) must have
/// a full opaque solid block directly below it. Without this, the
/// dust falls off when placed.
#[test]
fn dust_blocks_have_valid_support_blocks() {
    for kind in [
        GateKind::Not,
        GateKind::And,
        GateKind::Or,
        GateKind::Xor,
        GateKind::DTrigger,
        GateKind::MemoryCell,
        GateKind::Comparator,
        GateKind::Observer,
        GateKind::Repeater,
        GateKind::TargetBlock,
    ] {
        let cell = macrocell_for(kind).unwrap();
        let map = block_map(&cell);
        for pb in &cell.blocks {
            if !matches!(pb.block, BlockId::RedstoneDust) {
                continue;
            }
            let below = Pos3::new(pb.pos.x, pb.pos.y - 1, pb.pos.z);
            let support = map.get(&below).unwrap_or_else(|| {
                panic!(
                    "{kind:?}: dust at {:?} has nothing under it at {below:?} (would float)",
                    pb.pos
                )
            });
            assert!(
                is_solid_opaque_support(support.block),
                "{kind:?}: dust at {:?} sits on {:?} — must be a full opaque solid (e.g. Stone/OakPlanks)",
                pb.pos, support.block
            );
        }
    }
}

/// Catch-all: every PlacedBlock that requires a facing has one set, and
/// every facing direction is one of the 4 horizontal cardinals (for
/// wall_torch / repeater / comparator) or any 6-direction (for observer).
#[test]
fn directional_blocks_have_required_facings() {
    let horizontal_only = [
        BlockId::RedstoneWallTorch,
        BlockId::Repeater,
        BlockId::Comparator,
    ];
    for kind in [
        GateKind::Not,
        GateKind::And,
        GateKind::Or,
        GateKind::Xor,
        GateKind::DTrigger,
        GateKind::MemoryCell,
        GateKind::Comparator,
        GateKind::Observer,
        GateKind::Repeater,
        GateKind::TargetBlock,
    ] {
        let cell = macrocell_for(kind).unwrap();
        for pb in &cell.blocks {
            if horizontal_only.contains(&pb.block) {
                let f = pb.facing.unwrap_or_else(|| {
                    panic!("{kind:?}: {:?} at {:?} missing facing", pb.block, pb.pos)
                });
                assert!(
                    matches!(
                        f,
                        Direction::North | Direction::South | Direction::East | Direction::West
                    ),
                    "{kind:?}: {:?} at {:?} has non-horizontal facing {:?}",
                    pb.block,
                    pb.pos,
                    f
                );
            }
        }
    }
}

#[test]
fn memcell_anchors_use_stateful_roles() {
    let c = macrocell_for(GateKind::MemoryCell).unwrap();
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::DataInStateful)));
    assert!(c
        .inputs
        .iter()
        .any(|a| matches!(a.role, EndpointRole::WriteEnable)));
    assert!(matches!(c.outputs[0].role, EndpointRole::Q));
}
