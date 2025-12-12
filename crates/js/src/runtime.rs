//! Engine-agnostic JavaScript runtime prelude.
//!
//! This module exports a small JavaScript snippet that establishes minimal
//! globals and browser-like conveniences used by Valor during tests.

/// JavaScript source for the runtime prelude.
///
/// Engines should evaluate this once per context before running page scripts.
///
/// Note: keep this idempotent; guards and hidden markers prevent double work.
pub const RUNTIME_PRELUDE: &str = include_str!("runtime_prelude.js");
