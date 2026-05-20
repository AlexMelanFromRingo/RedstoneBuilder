//! Memory-budget enforcement at stage boundaries (FR-V09 / FR-V10).
//!
//! Samples resident-set-size via the `memory-stats` crate at every
//! stage-boundary call to [`BudgetGuard::check`]. On overrun, returns
//! [`MemoryError::CapExceeded`] which the pipeline maps to exit code 8.

use miette::Diagnostic;
use thiserror::Error;

/// Stage-boundary RAM-cap guard.
#[derive(Debug, Clone, Copy)]
pub struct BudgetGuard {
    cap_bytes: u64,
    stage_label: &'static str,
}

impl BudgetGuard {
    /// Construct a guard from a cap in mebibytes and a stage label
    /// (used by the diagnostic).
    pub fn new(cap_mb: u32, stage_label: &'static str) -> Self {
        Self {
            cap_bytes: u64::from(cap_mb).saturating_mul(1024 * 1024),
            stage_label,
        }
    }

    /// Return the current resident-set-size in bytes, or 0 if the
    /// platform sampler is unavailable (treated as "unknown — pass").
    pub fn current_rss_bytes() -> u64 {
        memory_stats::memory_stats()
            .map(|s| s.physical_mem as u64)
            .unwrap_or(0)
    }

    /// Sample the process RSS; fail if it exceeds the cap.
    pub fn check(&self) -> Result<u64, MemoryError> {
        let used = Self::current_rss_bytes();
        if used > self.cap_bytes {
            return Err(MemoryError::CapExceeded {
                cap: self.cap_bytes,
                actual: used,
                stage: self.stage_label,
            });
        }
        Ok(used)
    }
}

/// Memory-budget overrun (FR-V10). Maps to CLI exit code 8.
#[derive(Debug, Error, Diagnostic)]
pub enum MemoryError {
    /// The compiler's actual peak RSS would exceed `--max-ram`.
    #[error("memory cap exceeded at stage `{stage}`: used {actual} bytes, cap {cap} bytes")]
    #[diagnostic(
        code(rb_synthesis::memory_cap),
        help("re-run with a higher `--max-ram MB` value")
    )]
    CapExceeded {
        /// Active cap in bytes.
        cap: u64,
        /// Actual RSS at the moment of check, in bytes.
        actual: u64,
        /// Stage label of the BudgetGuard that fired.
        stage: &'static str,
    },
}
