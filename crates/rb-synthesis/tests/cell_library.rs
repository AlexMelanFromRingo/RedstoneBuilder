#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rb_core::GateKind;
use rb_synthesis::{macrocell_for, EndpointRole};

#[test]
fn every_primitive_has_a_cell_with_nonempty_bbox() {
    for kind in [
        GateKind::Not,
        GateKind::And,
        GateKind::Or,
        GateKind::Xor,
        GateKind::DTrigger,
        GateKind::MemoryCell,
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
