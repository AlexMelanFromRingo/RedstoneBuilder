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
