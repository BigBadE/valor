#![allow(
    clippy::missing_docs_in_private_items,
    reason = "Internal implementation details don't need public documentation"
)]
#![allow(
    clippy::missing_inline_in_public_items,
    reason = "Inlining decisions left to compiler for this crate"
)]
#![allow(
    clippy::min_ident_chars,
    reason = "Short variable names acceptable in parsing context"
)]
#![allow(
    clippy::expect_used,
    reason = "Expect used in controlled parsing scenarios"
)]

extern crate alloc;

pub mod dom;
pub mod parser;
