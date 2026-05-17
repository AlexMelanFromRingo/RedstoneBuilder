//! Miette diagnostic-renderer setup.
//!
//! Installs the `fancy` graphical handler by default; falls back to the
//! narrated handler when `NO_COLOR` is set or the output is not a TTY.
//! Idempotent: safe to call more than once (subsequent calls are no-ops).

use std::env;
use std::sync::Once;

static INSTALL: Once = Once::new();

/// Install the global `miette` diagnostic handler. Idempotent.
pub fn install() {
    INSTALL.call_once(|| {
        if env::var_os("NO_COLOR").is_some() {
            // Narrated (plain-text) handler. Errors here mean a handler
            // was already installed by another thread — safe to ignore.
            let _ = miette::set_hook(Box::new(|_| {
                Box::new(miette::NarratableReportHandler::new())
            }));
            return;
        }

        let _ = miette::set_hook(Box::new(|_| {
            Box::new(
                miette::MietteHandlerOpts::new()
                    .terminal_links(true)
                    .unicode(true)
                    .context_lines(2)
                    .build(),
            )
        }));
    });
}
