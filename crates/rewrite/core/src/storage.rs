//! Formula-based layout database.
//!
//! The database stores CSS property values and executes queries to produce formulas.

use crate::{CssLayoutProperty, Keyword, NodeId, Query};
use dashmap::DashMap;
use std::sync::Arc;

/// Central database for formula-based layout computation.
#[derive(Default, Clone)]
pub struct Database {
    /// CSS layout property values (keywords)
    pub(crate) layout_properties: Arc<DashMap<(NodeId, CssLayoutProperty), Keyword>>,
}

impl Database {
    /// Create a new database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a scoped database context for a node.
    pub fn scoped_to(&self, parent: NodeId, node: NodeId) -> crate::ScopedDb {
        crate::ScopedDb {
            db: self,
            parent,
            node,
            branch_deps: smallvec::SmallVec::new(),
        }
    }

    /// Get a layout property keyword.
    pub fn get_layout_keyword(&self, node: NodeId, prop: CssLayoutProperty) -> Keyword {
        self.layout_properties
            .get(&(node, prop))
            .map(|v| *v)
            .unwrap_or(Keyword::Auto) // Default to Auto if not set
    }

    /// Set a layout property keyword.
    pub fn set_layout_property(&self, node: NodeId, prop: CssLayoutProperty, value: Keyword) {
        self.layout_properties.insert((node, prop), value);
    }

    /// Execute a query to get a formula.
    /// Given a parent and node, runs the query and returns the selected formula.
    pub fn query(&self, parent: NodeId, node: NodeId, query: Query) -> &'static crate::Formula {
        let mut scoped_db = crate::ScopedDb {
            db: self,
            parent,
            node,
            branch_deps: smallvec::SmallVec::new(),
        };
        query(&mut scoped_db)
    }
}
