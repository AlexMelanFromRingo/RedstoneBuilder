#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use rb_core::GateKind;
use rb_parser::{parse, validate, SemanticError};

fn read_fixture(name: &str) -> (String, std::path::PathBuf) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let src = std::fs::read_to_string(&path).expect("read fixture");
    (src, path)
}

#[test]
fn half_adder_parses_to_expected_ast() {
    let (src, path) = read_fixture("half_adder.hdl");
    let module = parse(&src, &path).expect("parse half_adder.hdl");

    assert_eq!(module.name.as_str(), "half_adder");
    assert_eq!(module.ports.len(), 4);
    assert_eq!(module.ports[0].name.as_str(), "a");
    assert_eq!(module.ports[3].name.as_str(), "carry");
    assert_eq!(module.wires.len(), 0);
    assert_eq!(module.instances.len(), 2);
    assert!(matches!(module.instances[0].kind, GateKind::Xor));
    assert!(matches!(module.instances[1].kind, GateKind::And));

    validate(&module, &src).expect("half_adder validates clean");
}

#[test]
fn syntax_error_reports_line_and_column() {
    let (src, path) = read_fixture("syntax_error.hdl");
    let err = parse(&src, &path).expect_err("syntax_error.hdl must fail to parse");

    let report = miette::Report::new(err);
    let s = format!("{report:?}");
    assert!(
        s.contains("syntax error") || s.contains("rb_parser::syntax"),
        "expected syntax-error diagnostic, got: {s}"
    );
}

#[test]
fn undeclared_net_is_reported() {
    let (src, path) = read_fixture("undeclared.hdl");
    let module = parse(&src, &path).expect("parses fine — error is semantic");
    let errors = validate(&module, &src).expect_err("should fail semantic validation");

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SemanticError::UndeclaredNet { name, .. } if name == "c")),
        "expected UndeclaredNet for 'c', got: {errors:#?}"
    );
}

#[test]
fn undriven_output_is_reported() {
    let (src, path) = read_fixture("undriven_output.hdl");
    let module = parse(&src, &path).expect("parses fine");
    let errors = validate(&module, &src).expect_err("should fail validation");

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SemanticError::UndrivenOutput { name, .. } if name == "y")),
        "expected UndrivenOutput for 'y', got: {errors:#?}"
    );
}

#[test]
fn duplicate_identifier_is_reported() {
    let src = "module dup(input a, input a, output y);\n  not g(.A(a), .Y(y));\nendmodule\n";
    let path = std::path::PathBuf::from("inline.hdl");
    let module = parse(src, &path).expect("parses");
    let errors = validate(&module, src).expect_err("duplicate `a` must fail");

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SemanticError::DuplicateIdent { name, .. } if name == "a")),
        "expected DuplicateIdent for 'a', got: {errors:#?}"
    );
}

#[test]
fn dff_demo_parses_with_clock_attached() {
    let (src, path) = read_fixture("dff.hdl");
    let module = parse(&src, &path).expect("parse dff.hdl");
    assert_eq!(module.name.as_str(), "dff_demo");
    assert_eq!(module.instances.len(), 1);

    let ff = &module.instances[0];
    assert!(matches!(ff.kind, GateKind::DTrigger));
    assert_eq!(ff.inst_name.as_str(), "ff");
    let clock = ff.clock.as_ref().expect("clock attached");
    assert!(matches!(clock.edge, rb_parser::ast::Edge::Posedge));
    assert_eq!(clock.clock_net.as_str(), "clk");

    validate(&module, &src).expect("dff_demo validates clean");
}

#[test]
fn memcell_demo_parses_with_clock_attached() {
    let (src, path) = read_fixture("memcell.hdl");
    let module = parse(&src, &path).expect("parse memcell.hdl");

    let m = &module.instances[0];
    assert!(matches!(m.kind, GateKind::MemoryCell));
    let clock = m.clock.as_ref().expect("clock attached");
    assert!(matches!(clock.edge, rb_parser::ast::Edge::Posedge));
    assert_eq!(clock.clock_net.as_str(), "clk");

    validate(&module, &src).expect("mem_demo validates clean");
}

#[test]
fn analog_add_parses_with_comparator_subtract_and_analog_ports() {
    use rb_core::SignalKind;
    let (src, path) = read_fixture("analog_add.hdl");
    let module = parse(&src, &path).expect("parse analog_add.hdl");

    assert_eq!(module.name.as_str(), "analog_add");
    assert_eq!(module.ports.len(), 3);
    assert!(matches!(module.ports[0].kind, SignalKind::AnalogStrength));
    assert!(matches!(module.ports[1].kind, SignalKind::AnalogStrength));
    assert!(matches!(module.ports[2].kind, SignalKind::AnalogStrength));

    assert_eq!(module.instances.len(), 1);
    let c = &module.instances[0];
    assert_eq!(c.inst_name.as_str(), "c1");
    assert!(matches!(c.kind, GateKind::Comparator));
    assert!(matches!(
        c.compare_mode,
        Some(rb_parser::ast::CompareMode::Subtract)
    ));

    validate(&module, &src).expect("analog_add validates clean");
}

#[test]
fn monostable_parses_with_observer_and_repeater() {
    let (src, path) = read_fixture("monostable.hdl");
    let module = parse(&src, &path).expect("parse monostable.hdl");

    assert_eq!(module.instances.len(), 2);
    let obs = &module.instances[0];
    assert_eq!(obs.inst_name.as_str(), "o");
    assert!(matches!(obs.kind, GateKind::Observer));

    let rep = &module.instances[1];
    assert_eq!(rep.inst_name.as_str(), "r");
    assert!(matches!(rep.kind, GateKind::Repeater));
    assert_eq!(rep.repeater_delay, Some(1));

    validate(&module, &src).expect("monostable validates clean");
}

#[test]
fn analog_port_into_boolean_gate_is_rejected() {
    let (src, path) = read_fixture("signal_kind_mismatch.hdl");
    let module = parse(&src, &path).expect("parses; error is semantic");
    let errors = validate(&module, &src).expect_err("should fail semantic validation");

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SemanticError::SignalKindMismatch { name, .. } if name == "a")),
        "expected SignalKindMismatch for net 'a', got: {errors:#?}"
    );
}

#[test]
fn repeater_delay_out_of_range_is_rejected() {
    let src =
        "module bad(input a, output y);\n  repeater r(.IN(a), .OUT(y), .DELAY(7));\nendmodule\n";
    let path = std::path::PathBuf::from("bad_delay.hdl");
    let module = parse(src, &path).expect("parses");
    let errors = validate(&module, src).expect_err("delay 7 must fail");

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, SemanticError::BadDelay { actual, .. } if *actual == 7)),
        "expected BadDelay for delay=7, got: {errors:#?}"
    );
}

#[test]
fn wire_decls_with_multiple_names_expand() {
    let src = r#"
        module multi(input a, input b, output y);
            wire w1, w2, w3;
            and g1(.A(a), .B(b), .Y(y));
        endmodule
    "#;
    let path = std::path::PathBuf::from("inline.hdl");
    let module = parse(src, &path).expect("parses");
    assert_eq!(module.wires.len(), 3);
    assert_eq!(module.wires[0].name.as_str(), "w1");
    assert_eq!(module.wires[1].name.as_str(), "w2");
    assert_eq!(module.wires[2].name.as_str(), "w3");
}

#[test]
fn bus_ports_and_wires_desugar_into_per_bit_nets() {
    let (src, path) = read_fixture("bus_decl.hdl");
    let module = parse(&src, &path).expect("parse bus_decl.hdl");

    // `input [1:0] a`, `input [1:0] b`, `output [1:0] y` → 6 scalar ports.
    assert_eq!(module.ports.len(), 6, "ports: {:?}", module.ports);
    let port_names: Vec<&str> = module.ports.iter().map(|p| p.name.as_str()).collect();
    assert!(port_names.contains(&"a[0]"));
    assert!(port_names.contains(&"a[1]"));
    assert!(port_names.contains(&"y[0]"));
    assert!(port_names.contains(&"y[1]"));

    // Bit-indexed connections resolve to flat scalar net names.
    let g0 = &module.instances[0];
    let a_conn = g0
        .connections
        .iter()
        .find(|c| c.port.as_str().eq_ignore_ascii_case("A"))
        .expect("g0 has .A");
    assert_eq!(a_conn.net.as_str(), "a[0]");

    validate(&module, &src).expect("bus_decl.hdl validates clean");
}

#[test]
fn bus_wire_decl_expands_to_width() {
    let src = r#"
        module busw(input x, output y);
            wire [3:0] bus;
            and g(.A(x), .B(x), .Y(y));
        endmodule
    "#;
    let path = std::path::PathBuf::from("busw.hdl");
    let module = parse(src, &path).expect("parses");
    // `wire [3:0] bus;` → bus[0]..bus[3].
    let bus_nets: Vec<&str> = module
        .wires
        .iter()
        .map(|w| w.name.as_str())
        .filter(|n| n.starts_with("bus["))
        .collect();
    assert_eq!(bus_nets.len(), 4, "got {bus_nets:?}");
}

#[test]
fn hierarchical_design_elaborates_to_flat_top() {
    let (src, path) = read_fixture("hier_adder.hdl");
    let module = parse(&src, &path).expect("parse + elaborate hier_adder.hdl");

    // Top is adder2; both full_adder instances inlined.
    assert_eq!(module.name.as_str(), "adder2");
    assert!(
        module.mod_instances.is_empty(),
        "elaboration must leave no sub-module instances"
    );
    // 2 full_adders × 5 gates each = 10 flattened gates.
    assert_eq!(
        module.instances.len(),
        10,
        "gates: {}",
        module.instances.len()
    );

    // Sub-module internal nets are prefixed with the instance name.
    let gate_names: Vec<&str> = module
        .instances
        .iter()
        .map(|g| g.inst_name.as_str())
        .collect();
    assert!(gate_names.contains(&"fa0.x1"), "names: {gate_names:?}");
    assert!(gate_names.contains(&"fa1.o1"), "names: {gate_names:?}");

    // The carry net `c1` declared in adder2 binds fa0.cout → fa1.cin.
    let fa0_o1 = module
        .instances
        .iter()
        .find(|g| g.inst_name.as_str() == "fa0.o1")
        .expect("fa0.o1 exists");
    let y = fa0_o1
        .connections
        .iter()
        .find(|c| c.port.as_str().eq_ignore_ascii_case("Y"))
        .expect("fa0.o1 has .Y");
    assert_eq!(y.net.as_str(), "c1", "fa0 cout must bind to parent net c1");

    validate(&module, &src).expect("elaborated hier_adder validates clean");
}

#[test]
fn unknown_submodule_is_an_elaboration_error() {
    let src = r#"
        module top(input a, output y);
            missing_mod u(.in(a), .out(y));
        endmodule
    "#;
    let path = std::path::PathBuf::from("bad_hier.hdl");
    let err = parse(src, &path).expect_err("unknown submodule must fail");
    let report = miette::Report::new(err);
    let s = format!("{report:?}");
    assert!(
        s.contains("elaboration") || s.contains("unknown module"),
        "expected elaboration error, got: {s}"
    );
}

#[test]
fn recursive_instantiation_is_rejected() {
    let src = r#"
        module loop_a(input a, output y);
            loop_a inner(.a(a), .y(y));
        endmodule
    "#;
    let path = std::path::PathBuf::from("recursive.hdl");
    let err = parse(src, &path).expect_err("recursion must fail");
    let report = miette::Report::new(err);
    let s = format!("{report:?}");
    assert!(
        s.contains("recursive") || s.contains("elaboration"),
        "expected recursion error, got: {s}"
    );
}
