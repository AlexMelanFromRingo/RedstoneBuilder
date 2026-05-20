//! v2: A* + PathFinder routing perf benchmark on a 10 000-gate
//! synthetic design (SC-V03 perf gate).
//!
//! Built via `cargo bench --no-run` in CI. Full runs (which actually
//! route 10k nets) are manual / nightly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use rb_parser::parse;
use rb_synthesis::{
    build_netlist, place, route_pathfinder, CostMap, NetTag, PathFinderConfig, PlaceConfig,
};

fn synth_long_chain(n: usize) -> String {
    let mut src = String::from("module chain(input a, output q);\n    wire ");
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
        src.push_str(&format!("    not g{i}(.A({from}), .Y(w{i}));\n"));
    }
    src.push_str(&format!("    not gq(.A(w{}), .Y(q));\n", n - 1));
    src.push_str("endmodule\n");
    src
}

fn bench(c: &mut Criterion) {
    let src = synth_long_chain(10_000);
    let path = PathBuf::from("stress10k.hdl");
    let module = parse(&src, &path).expect("parse 10k");
    let netlist = build_netlist(&module).expect("netlist 10k");
    let placement = place(&netlist, &PlaceConfig::DEFAULT).expect("place 10k");

    let mut routes: Vec<(NetTag, rb_core::Pos3, rb_core::Pos3)> = Vec::new();
    for win in placement.cells.windows(2) {
        let s = &win[0];
        let d = &win[1];
        let so = s.macro_cell.outputs[0];
        let di = d.macro_cell.inputs[0];
        let sp = rb_core::Pos3::new(
            s.origin.x + so.pos.x,
            s.origin.y + so.pos.y,
            s.origin.z + so.pos.z,
        );
        let dp = rb_core::Pos3::new(
            d.origin.x + di.pos.x,
            d.origin.y + di.pos.y,
            d.origin.z + di.pos.z,
        );
        routes.push((NetTag(s.node.index() as u32), sp, dp));
    }

    let bounds = rb_core::Bbox3::from_corners(
        rb_core::Pos3::new(-4096, 0, -4096),
        rb_core::Pos3::new(4096, 8, 4096),
    );

    let mut group = c.benchmark_group("pathfinder_10k");
    group.sample_size(10); // expensive — fewer samples
    group.bench_function("route_pathfinder_10k_chain", |b| {
        b.iter(|| {
            let mut cost = CostMap::new(bounds);
            let _ = route_pathfinder(&mut cost, &routes, &PathFinderConfig::DEFAULT);
        })
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
