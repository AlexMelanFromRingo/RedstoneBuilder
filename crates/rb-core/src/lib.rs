//! Shared types for the RedstoneBuilder workspace.
//!
//! See `specs/001-hdl-compiler-cli/data-model.md` for the role of these
//! types in the compiler pipeline.

#![deny(missing_docs)]

pub mod block;
pub mod gate;
pub mod pos;
pub mod signal;
pub mod span;
pub mod timing;

pub use block::BlockId;
pub use gate::GateKind;
pub use pos::{Bbox3, Direction, Pos3};
pub use signal::{Signal, SignalKind};
pub use span::SourceSpan;
pub use timing::Tick;
