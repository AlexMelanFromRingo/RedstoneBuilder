//! Routing perf benchmark — SC-006 perf gate (~1 k-gate design < 120 s).
//!
//! Built and registered via `cargo bench --no-run` in CI; full runs are
//! manual / nightly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use rb_parser::parse;
use rb_synthesis::{
    build_netlist, place, route_with_retry, Grid3D, NetTag, PlaceConfig, RouteConfig,
};

fn synth_long_chain(n: usize) -> String {
    // Build a chain of `n` NOT gates: w0 → w1 → … → w_{n-1} → out
    let mut src = String::from("module chain(input a, output q);\n");
    src.push_str("    wire ");
    for i in 0..n {
        if i > 0 {
            src.push_str(", ");
        }
        src.push_str(&format!("w{i}"));
    }
    src.push_str(";\n");
    for i in 0..n {
        let from = if i == 0 {
            "a".to_string()
        } else {
            format!("w{}", i - 1)
        };
        let to = format!("w{i}");
        src.push_str(&format!("    not g{i}(.A({from}), .Y({to}));\n"));
    }
    src.push_str(&format!("    not gq(.A(w{}), .Y(q));\n", n - 1));
    src.push_str("endmodule\n");
    src
}

fn route_pipeline_bench(c: &mut Criterion) {
    let src = synth_long_chain(1000);
    let path = PathBuf::from("bench.hdl");
    let module = parse(&src, &path).expect("parse");
    let netlist = build_netlist(&module).expect("netlist");
    let placement = place(&netlist, &PlaceConfig::DEFAULT).expect("place");

    let mut routes: Vec<(NetTag, rb_core::Pos3, rb_core::Pos3)> = Vec::new();
    // Stub: route consecutive cells' output→next cell's input
    for win in placement.cells.windows(2) {
        let src_cell = &win[0];
        let dst_cell = &win[1];
        let src_out = src_cell.macro_cell.outputs[0];
        let dst_in = dst_cell.macro_cell.inputs[0];
        let src_pos = rb_core::Pos3::new(
            src_cell.origin.x + src_out.pos.x,
            src_cell.origin.y + src_out.pos.y,
            src_cell.origin.z + src_out.pos.z,
        );
        let dst_pos = rb_core::Pos3::new(
            dst_cell.origin.x + dst_in.pos.x,
            dst_cell.origin.y + dst_in.pos.y,
            dst_cell.origin.z + dst_in.pos.z,
        );
        routes.push((NetTag(src_cell.node.index() as u32), src_pos, dst_pos));
    }

    let template = Grid3D::new();
    c.bench_function("route_with_retry_1k", |b| {
        b.iter(|| {
            let cfg = RouteConfig::DEFAULT;
            let _ = route_with_retry(&template, &routes, &cfg);
        })
    });
}

criterion_group!(benches, route_pipeline_bench);
criterion_main!(benches);
