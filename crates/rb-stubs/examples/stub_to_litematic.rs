//! Smoke test: load a single stub by name, write it out as a
//! `.litematic` file readable by Litematica 0.27.x on MC 26.1.
//!
//! This is the v3 minimum-viable pipeline:
//!
//! ```text
//!   stub_lib/core/<name>.schem  --rb-stubs--> Stub
//!                                  │
//!                                  ▼
//!                          rb-nbt::BlockGrid
//!                                  │
//!                                  ▼
//!                   write_litematic → .litematic (v7)
//! ```
//!
//! Usage:
//!     cargo run -p rb-stubs --example stub_to_litematic -- \
//!         kan_cc_adder_8b out/v3_kan_cc_adder.litematic

use std::env;
use std::path::PathBuf;

use rb_core::Bbox3;
use rb_nbt::{write_litematic, BlockGrid};
use rb_stubs::{load_stub_library, StubLibrary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let stub_name = args
        .next()
        .ok_or("usage: stub_to_litematic <name> <out.litematic>")?;
    let out_path = args
        .next()
        .ok_or("usage: stub_to_litematic <name> <out.litematic>")?;

    let stub_lib_path = workspace_root().join("stub_lib").join("core");
    eprintln!("Loading stubs from {}...", stub_lib_path.display());
    let lib: StubLibrary = load_stub_library(&stub_lib_path)?;
    eprintln!(
        "Loaded {} stub(s): {:?}",
        lib.len(),
        lib.names().iter().take(8).collect::<Vec<_>>()
    );

    let stub = lib
        .get(&stub_name)
        .ok_or_else(|| format!("stub {stub_name:?} not in library ({:?})", lib.names()))?;

    eprintln!(
        "Stub {:?}: bbox {:?}, {} blocks, {} ports",
        stub.name,
        stub.bbox,
        stub.blocks.len(),
        stub.ports.len()
    );
    for port in &stub.ports {
        eprintln!(
            "  port {}: dir={:?} width={} pin_start={:?} step={:?}",
            port.name, port.dir, port.width, port.pin_start, port.pin_step
        );
    }

    // Translate stub blocks → BlockGrid. Stub coords are already
    // local; we just remap to 0-origin (in case the stub's bbox
    // doesn't start at 0,0,0).
    let bbox = stub.bbox;
    let bounds = Bbox3::from_corners(
        rb_core::Pos3::ORIGIN,
        rb_core::Pos3::new(
            bbox.max.x - bbox.min.x,
            bbox.max.y - bbox.min.y,
            bbox.max.z - bbox.min.z,
        ),
    );
    let mut grid = BlockGrid::empty(bounds);
    for (pos, block) in &stub.blocks {
        let p = rb_core::Pos3::new(pos.x - bbox.min.x, pos.y - bbox.min.y, pos.z - bbox.min.z);
        if bounds.contains(p) {
            grid.insert(p, block.clone());
        }
    }

    let out = PathBuf::from(out_path);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    write_litematic(
        &out,
        &grid,
        stub.name.as_str(),
        &format!("RedstoneBuilder v3 export of stub {:?}", stub.name),
    )?;
    eprintln!("Wrote {}", out.display());
    Ok(())
}

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // workspace root
    p
}
