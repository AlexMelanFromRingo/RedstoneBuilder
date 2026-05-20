//! Error types for stub loading.

use std::path::PathBuf;

use thiserror::Error;

/// Anything that can go wrong while loading a stub library file.
#[derive(Debug, Error)]
pub enum StubError {
    /// Filesystem I/O failed (file missing, unreadable, etc.).
    #[error("I/O error reading {path}: {source}")]
    Io {
        /// Path we tried to read.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },
    /// File was found and decompressed but the NBT structure is
    /// invalid or doesn't match the Sponge / Litematica schema.
    #[error("malformed NBT in {path}: {message}")]
    BadNbt {
        /// Path that contained the bad NBT.
        path: PathBuf,
        /// Human-readable detail.
        message: String,
    },
    /// A stub schematic lacks the convention markers we require.
    /// Includes both missing-glass and missing-name-sign cases.
    #[error("{path} is not a valid stub: {message}")]
    NotAStub {
        /// Path that failed validation.
        path: PathBuf,
        /// What we expected and didn't find.
        message: String,
    },
    /// A stub's port-pin sign had a malformed text label.
    #[error("port sign at {pos:?} in {path} has unparseable text {text:?}")]
    BadPortSign {
        /// Schematic containing the offending sign.
        path: PathBuf,
        /// World position of the sign block.
        pos: (i32, i32, i32),
        /// Raw first line of the sign.
        text: String,
    },
}
