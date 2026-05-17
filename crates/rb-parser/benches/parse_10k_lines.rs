//! Parser perf benchmark: ~10 000-line HDL file. Built via
//! `cargo bench --no-run` in CI; full runs are manual/nightly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use rb_parser::parse;

fn synth_long_chain(n: usize) -> String {
    let mut src = String::with_capacity(n * 60);
    src.push_str("module chain(input a, output q);\n");
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
        src.push_str(&format!("    not g{i}(.A({from}), .Y(w{i}));\n"));
    }
    src.push_str(&format!("    not gq(.A(w{}), .Y(q));\n", n - 1));
    src.push_str("endmodule\n");
    src
}

fn parse_bench(c: &mut Criterion) {
    let src = synth_long_chain(3_300); // ~10 000 lines including wire decl
    let path = PathBuf::from("bench.hdl");
    c.bench_function("parse_10k_lines", |b| {
        b.iter(|| {
            let m = parse(&src, &path).expect("parse");
            std::hint::black_box(m);
        })
    });
}

criterion_group!(benches, parse_bench);
criterion_main!(benches);
