//! NBT writer perf benchmark: serialize a densely-populated 256³ cube.
//! Built via `cargo bench --no-run` in CI; full runs are
//! manual/nightly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{criterion_group, criterion_main, Criterion};
use rb_core::{Bbox3, BlockId, Pos3};
use rb_nbt::{block_state_for, build_root, encode_to_bytes, BlockGrid};

fn build_dense_grid(side: i32) -> BlockGrid {
    let bounds = Bbox3::from_corners(Pos3::ORIGIN, Pos3::new(side - 1, side - 1, side - 1));
    let mut grid = BlockGrid::empty(bounds);
    // Populate every 8th cell so the palette gets a handful of variants
    // and the packed-long encoder gets a realistic distribution.
    for y in (0..side).step_by(8) {
        for z in (0..side).step_by(4) {
            for x in (0..side).step_by(2) {
                let block = match ((x + y + z) / 16) % 5 {
                    0 => BlockId::RedstoneDust,
                    1 => BlockId::RedstoneTorch,
                    2 => BlockId::Stone,
                    3 => BlockId::OakPlanks,
                    _ => BlockId::Repeater,
                };
                grid.insert(Pos3::new(x, y, z), block_state_for(block, None));
            }
        }
    }
    grid
}

fn encode_bench(c: &mut Criterion) {
    // 64³ instead of 256³ — bench focus is throughput per cell, not
    // memory pressure of a single huge run. Bumping to 256³ keeps the
    // same hot loops; gate against SC-006 perf via the bigger run when
    // we're profiling.
    let grid = build_dense_grid(64);
    let root = build_root(&grid, "bench", "perf");

    c.bench_function("encode_64_cube", |b| {
        b.iter(|| {
            let bytes = encode_to_bytes(&root).expect("encode");
            std::hint::black_box(bytes);
        })
    });
}

criterion_group!(benches, encode_bench);
criterion_main!(benches);
