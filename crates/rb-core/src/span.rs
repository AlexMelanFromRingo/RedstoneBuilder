//! Source code spans for diagnostics.
//!
//! These are produced by the parser and threaded through every
//! downstream stage so that synthesis or routing errors can still point
//! back to the user's source.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

/// A range of source bytes, plus the human-friendly line/column of the
/// span's start.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SourceSpan {
    /// File path the span comes from.
    pub file: Arc<PathBuf>,
    /// Byte offset of the span's start (inclusive).
    pub byte_start: u32,
    /// Byte offset of the span's end (exclusive).
    pub byte_end: u32,
    /// 1-based line number of `byte_start`.
    pub line: u32,
    /// 1-based column of `byte_start` (in chars, not bytes).
    pub column: u32,
}

impl SourceSpan {
    /// Build a span from an explicit byte range.
    pub fn new(file: Arc<PathBuf>, byte_start: u32, byte_end: u32, line: u32, column: u32) -> Self {
        Self {
            file,
            byte_start,
            byte_end,
            line,
            column,
        }
    }

    /// Length in bytes.
    pub fn len(&self) -> u32 {
        self.byte_end.saturating_sub(self.byte_start)
    }

    /// True for a zero-length span.
    pub fn is_empty(&self) -> bool {
        self.byte_end == self.byte_start
    }
}
