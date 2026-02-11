//! HTML parsing and DOM tree building.

mod builder;
mod parser;
mod tree;
mod types;

pub use parser::HtmlParser;
pub use tree::DomTree;
pub use types::{DomUpdate, NodeData};
