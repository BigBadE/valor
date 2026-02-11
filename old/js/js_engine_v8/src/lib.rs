//! JavaScript engine adapter using V8 backend.
//!
//! This crate provides a V8-backed implementation of the `JsEngine` trait.

mod bindings;
mod conversions;
mod engine;

pub use engine::V8Engine;
