//! Shared helpers for the examples. Not a crate module.
//!
//! A directory under `examples/` is not an example target, so this compiles only into the
//! examples that declare `mod support;`.

// This module is compiled once per example, so a method one example uses is dead code in
// the other. The alternative is a separate copy of the catalogue loader in each.
#[allow(dead_code)]
pub mod catalogue;
