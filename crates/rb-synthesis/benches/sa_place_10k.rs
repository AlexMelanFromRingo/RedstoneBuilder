//! v2: simulated-annealing placer perf benchmark on a 10 000-gate
//! synthetic design (SC-V03 / SC-V04 perf gates).
//!
//! Built via `cargo bench --no-run` in CI; full runs are
//! manual / nightly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use rb_parser::parse;
use rb_synthesis::{build_netlist, place_simulated_annealing, PlaceConfig, SaConfig};

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

    let mut group = c.benchmark_group("sa_place_10k");
    group.sample_size(10);
    group.bench_function("place_simulated_annealing_10k_chain", |b| {
        b.iter(|| {
            let _ = place_simulated_annealing(&netlist, &SaConfig::DEFAULT, &PlaceConfig::DEFAULT);
        })
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
