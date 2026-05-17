//! HDL parser for RedstoneBuilder.
//!
//! See `specs/001-hdl-compiler-cli/contracts/ast.md` and
//! `contracts/hdl-grammar.md` for the contracts this crate implements.

#![deny(missing_docs)]

pub mod ast;
pub mod error;
mod parse;
mod validate;

pub use error::{ParseError, SemanticError};
pub use parse::parse;
pub use validate::validate;
