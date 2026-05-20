#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verify that every Sponge `.schem` stub imported into
//! `stub_lib/core/` loads cleanly into a [`Stub`] with sensible
//! name + ports.

use std::path::PathBuf;

use rb_stubs::{load_stub_from_schem, load_stub_library, LoadOptions, PortDir};

fn stub_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("stub_lib/core");
    p.push(format!("{name}.schem"));
    p
}

fn stub_lib_core() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("stub_lib/core");
    p
}

#[test]
fn adder_stub_loads_with_3_ports() {
    let stub =
        load_stub_from_schem(&stub_path("adder"), LoadOptions::default()).expect("adder load");
    assert!(stub.name.contains("adder"), "name was {:?}", stub.name);
    let port_names: Vec<&str> = stub.ports.iter().map(|p| p.name.as_str()).collect();
    assert!(
        port_names.contains(&"a"),
        "missing port 'a': {port_names:?}"
    );
    assert!(
        port_names.contains(&"b"),
        "missing port 'b': {port_names:?}"
    );
    assert!(
        port_names.contains(&"c"),
        "missing port 'c': {port_names:?}"
    );
    let c = stub.port("c").unwrap();
    assert_eq!(c.dir, PortDir::Out);
    assert_eq!(c.width, 8, "expected 8-bit output");
}

#[test]
fn not_h8b_stub_loads_with_in_and_out_ports() {
    let stub =
        load_stub_from_schem(&stub_path("not_h8b"), LoadOptions::default()).expect("not_h8b load");
    assert!(stub.name.contains("not"));
    let p_in = stub.port("in").expect("port 'in'");
    let p_out = stub.port("out").expect("port 'out'");
    assert_eq!(p_in.dir, PortDir::In);
    assert_eq!(p_out.dir, PortDir::Out);
    assert_eq!(p_in.width, 8);
    assert_eq!(p_out.width, 8);
}

#[test]
fn library_loads_all_redhdl_stubs() {
    let lib = load_stub_library(&stub_lib_core()).expect("library load");
    let names = lib.names();
    // We have 7 source files; `bitwise_and_h8b.schem` /
    // `bitwise_not_h8b.schem` share name signs ("and_h8b" /
    // "not_h8b") with the non-bitwise variants and so collapse to
    // 5 unique stubs in the library.
    let expected_substrings = ["and_h8b", "not_h8b", "adder", "xor", "diagonal_not"];
    for needle in expected_substrings {
        assert!(
            names.iter().any(|n| n.contains(needle)),
            "missing stub containing {needle:?} (lib has {names:?})"
        );
    }
    assert!(
        lib.len() >= 5,
        "expected ≥ 5 unique stubs, got {} ({names:?})",
        lib.len()
    );
}

#[test]
fn loaded_stub_blocks_strip_glass_corners_and_signs() {
    let stub =
        load_stub_from_schem(&stub_path("and_h8b"), LoadOptions::default()).expect("and_h8b load");
    // No glass should remain.
    for (pos, block) in &stub.blocks {
        assert!(
            !block.name.as_str().contains("glass"),
            "glass marker not stripped at {pos:?}: {}",
            block.name.as_str()
        );
        assert!(
            !block.name.as_str().contains("sign"),
            "sign not stripped at {pos:?}: {}",
            block.name.as_str()
        );
    }
}

#[test]
fn stub_ports_have_reasonable_pin_geometry() {
    let stub = load_stub_from_schem(&stub_path("and_h8b"), LoadOptions::default()).unwrap();
    for port in &stub.ports {
        // pin_start should be inside or very close to the stub's bbox.
        let bbox = &stub.bbox;
        let margin = 2; // tolerate 1 cell outside (signs mount on faces)
        assert!(
            port.pin_start.x >= bbox.min.x - margin && port.pin_start.x <= bbox.max.x + margin,
            "port {} pin_start.x = {} outside bbox x [{}..{}]",
            port.name,
            port.pin_start.x,
            bbox.min.x,
            bbox.max.x
        );
    }
}
