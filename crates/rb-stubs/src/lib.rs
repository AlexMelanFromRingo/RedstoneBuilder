//! Hand-built schematic primitives ("stubs") used as the gate library
//! for RedstoneBuilder v3's compose-from-stubs architecture.
//!
//! A stub is a small `.schem` or `.litematic` file (built by hand in
//! Minecraft, saved via WorldEdit / Litematica) that implements one
//! redstone primitive — NOT, AND, OR, NAND, NOR, XOR, DFF, etc. The
//! compiler instantiates stubs, places them via SA, and runs wires
//! between their I/O pins via the bussing router.
//!
//! Per the convention pioneered by `andrewsmike/redhdl`:
//!
//! - **2 glass blocks** mark opposing corners of the stub's
//!   axis-aligned bounding box. Any glass variant (plain, stained,
//!   tinted) counts. Corners may lie on any of the 4 main diagonals.
//! - **A schematic-name sign** sits adjacent (any of 6 faces) to one
//!   of the glass corners. Its first text line is the stub name.
//! - **Port-pin signs** elsewhere in the schematic have first lines
//!   `input <name>[<index>]` or `output <name>[<index>]`. Index is
//!   optional for 1-bit ports.
//! - The **core circuit** lives between the glass corners; the
//!   loader strips the glass markers from the materialised output.
//!
//! See [`Stub::load_schem`] for the entry point.

#![deny(missing_docs)]

pub mod error;
pub mod loader;
pub mod stub;

pub use error::StubError;
pub use loader::{load_stub_from_schem, load_stub_library, stub_from_sponge, LoadOptions};
pub use stub::{PortDir, Stub, StubLibrary, StubPort};
