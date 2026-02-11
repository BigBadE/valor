//! Formula resolution - evaluates formulas to concrete pixel values.
//!
//! The resolver is generic over `T`, the styler type. `T` must provide:
//! - `get_property(&PropertyId) -> Option<i32>` for CSS value resolution
//! - `related(SingleRelationship) -> T` for navigating to related nodes
//! - `related_iter(MultiRelationship) -> Vec<T>` for iterating related nodes
//! - `sibling_index() -> usize` for structural queries
//!
//! These are inherent methods on `T`, not a trait.

use crate::{
    Aggregation, GenericFormula, GenericFormulaList, MultiRelationship, NodeId, Operation,
    SingleRelationship, Subpixel,
};
use lightningcss::properties::PropertyId;
use std::collections::HashMap;

/// Cache key for resolved formula values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CacheKey {
    node: NodeId,
    /// Formula pointer as identity (stable since formulas are 'static).
    /// Stored as usize to avoid generic parameter on CacheKey.
    formula_ptr: usize,
}

/// Context for formula resolution with caching.
///
/// Generic over `T`, the styler type. Uses inherent methods on `T` rather
/// than a trait, so concrete types define the interface directly.
pub struct ResolveContext<T: StylerAccess> {
    /// Cached resolved values: (node, formula) -> pixels.
    cache: HashMap<CacheKey, Subpixel>,
    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,
    /// Styler for CSS property queries.
    styler: T,
}

/// Trait for the minimal interface the resolver needs from a styler.
///
/// This keeps the resolver decoupled from any specific styler implementation
/// while avoiding `dyn` dispatch.
pub trait StylerAccess: Sized {
    /// Query a CSS property for the current node, converted to pixels.
    fn get_property(&self, prop_id: &PropertyId<'static>) -> Option<Subpixel>;

    /// Get a styler for a related node.
    /// Returns the root styler if the relationship doesn't exist (e.g., parent of root).
    fn related(&self, rel: SingleRelationship) -> Self;

    /// Get stylers for all nodes in a multi-relationship.
    fn related_iter(&self, rel: MultiRelationship) -> Vec<Self>;

    /// Get the index of this node among its siblings (0-based).
    fn sibling_index(&self) -> usize;

    /// Get the node ID this styler is scoped to.
    fn node_id(&self) -> NodeId;

    /// Get the viewport width in pixels.
    fn viewport_width(&self) -> u32;

    /// Get the viewport height in pixels.
    fn viewport_height(&self) -> u32;
}

impl<T: StylerAccess> ResolveContext<T> {
    /// Create a new resolve context.
    pub fn new(viewport_width: u32, viewport_height: u32, styler: T) -> Self {
        Self {
            cache: HashMap::new(),
            viewport_width,
            viewport_height,
            styler,
        }
    }

    /// Resolve a formula for a node, using cache if available.
    pub fn resolve(
        &mut self,
        formula: &'static GenericFormula<T>,
        node: NodeId,
    ) -> Option<Subpixel> {
        let key = CacheKey {
            node,
            formula_ptr: formula as *const GenericFormula<T> as usize,
        };

        if let Some(&cached) = self.cache.get(&key) {
            return Some(cached);
        }

        let value = self.resolve_inner(formula, node)?;
        self.cache.insert(key, value);
        Some(value)
    }

    /// Resolve a formula in the context of a different node.
    fn resolve_for_node(
        &mut self,
        formula: &'static GenericFormula<T>,
        target: T,
    ) -> Option<Subpixel> {
        let target_node = target.node_id();
        let old_styler = std::mem::replace(&mut self.styler, target);
        let value = self.resolve(formula, target_node);
        self.styler = old_styler;
        value
    }

    /// Internal resolve function.
    fn resolve_inner(
        &mut self,
        formula: &'static GenericFormula<T>,
        node: NodeId,
    ) -> Option<Subpixel> {
        match formula {
            GenericFormula::Constant(value) => Some(*value),

            GenericFormula::ViewportWidth => Some(self.viewport_width as Subpixel),

            GenericFormula::ViewportHeight => Some(self.viewport_height as Subpixel),

            GenericFormula::Op(op, a, b) => {
                let a_val = self.resolve(a, node)?;
                let b_val = self.resolve(b, node)?;
                Some(match op {
                    Operation::Add => a_val + b_val,
                    Operation::Sub => a_val - b_val,
                    Operation::Mul => a_val * b_val,
                    Operation::Div => {
                        if b_val == 0 {
                            0
                        } else {
                            a_val / b_val
                        }
                    }
                })
            }

            GenericFormula::CssValue(prop_id) => self.styler.get_property(prop_id),

            GenericFormula::CssValueOrDefault(prop_id, default) => {
                Some(self.styler.get_property(prop_id).unwrap_or(*default))
            }

            GenericFormula::Related(rel, query_fn) => {
                let target = self.styler.related(*rel);
                let result_formula = query_fn(&target)?;
                self.resolve_for_node(result_formula, target)
            }

            GenericFormula::Aggregate(agg, list) => {
                let values = self.collect_list(list)?;
                match agg {
                    Aggregation::Sum => Some(values.iter().sum()),
                    Aggregation::Max => values.into_iter().max(),
                    Aggregation::Min => values.into_iter().min(),
                    Aggregation::Average => {
                        if values.is_empty() {
                            Some(0)
                        } else {
                            Some(values.iter().sum::<Subpixel>() / values.len() as Subpixel)
                        }
                    }
                }
            }

            GenericFormula::Count(list) => {
                let rel = match list {
                    GenericFormulaList::Related(rel, _) | GenericFormulaList::Map(rel, _) => *rel,
                };
                Some(self.styler.related_iter(rel).len() as Subpixel)
            }

            GenericFormula::SiblingIndex => Some(self.styler.sibling_index() as Subpixel),
        }
    }

    /// Collect values from a formula list.
    fn collect_list(&mut self, list: &'static GenericFormulaList<T>) -> Option<Vec<Subpixel>> {
        match list {
            GenericFormulaList::Related(rel, query_fn) => {
                let targets = self.styler.related_iter(*rel);
                let mut values = Vec::with_capacity(targets.len());
                for target in targets {
                    if let Some(formula) = query_fn(&target) {
                        values.push(self.resolve_for_node(formula, target)?);
                    }
                }
                Some(values)
            }

            GenericFormulaList::Map(rel, formula) => {
                let targets = self.styler.related_iter(*rel);
                let mut values = Vec::with_capacity(targets.len());
                for target in targets {
                    values.push(self.resolve_for_node(formula, target)?);
                }
                Some(values)
            }
        }
    }
}
