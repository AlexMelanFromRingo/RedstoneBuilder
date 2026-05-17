//! Errors produced by the NBT writer.

use miette::Diagnostic;
use thiserror::Error;

/// Errors raised by [`crate::write_litematic`] and the underlying
/// encoder.
#[derive(Debug, Error, Diagnostic)]
pub enum NbtError {
    /// Filesystem / writer I/O failure.
    #[error("io error writing schematic: {0}")]
    #[diagnostic(code(rb_nbt::io))]
    Io(#[from] std::io::Error),

    /// `fastnbt` failed to serialize the root compound.
    #[error("nbt serialization error: {0}")]
    #[diagnostic(code(rb_nbt::serialize))]
    Nbt(String),

    /// The palette grew past the bit-width budget v1 supports.
    /// (Each palette index must fit into `MAX_BITS_PER_INDEX = 32` bits;
    /// real-world designs use far fewer.)
    #[error("palette overflow: {count} unique block states exceeds v1 limit of {max}")]
    #[diagnostic(code(rb_nbt::palette_overflow))]
    PaletteOverflow {
        /// Distinct block states the writer was asked to encode.
        count: usize,
        /// Hard upper bound for v1.
        max: usize,
    },
}

impl From<fastnbt::error::Error> for NbtError {
    fn from(err: fastnbt::error::Error) -> Self {
        NbtError::Nbt(err.to_string())
    }
}
