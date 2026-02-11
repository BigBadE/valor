//! Node styler context for formula resolution.
//!
//! Provides an implementation of `StylerAccess` from `rewrite_core`
//! that queries the DOM tree on demand — no pre-computed relationships.

use crate::Styler;
use crate::value_resolver::resolve_length;
use lightningcss::properties::{Property, PropertyId};
use rewrite_core::{MultiRelationship, NodeId, SingleRelationship, StylerAccess, Subpixel};

/// A styler scoped to a specific node, providing CSS property access
/// and tree navigation for formula resolution.
///
/// Constructs related stylers on demand by navigating the DOM tree.
/// No pre-computed relationships — unlimited traversal depth.
pub struct NodeStylerContext<'st> {
    styler: &'st Styler,
    node: NodeId,
    vw: u32,
    vh: u32,
}

impl<'st> NodeStylerContext<'st> {
    /// Create a new node styler context.
    pub fn new(styler: &'st Styler, node: NodeId, vw: u32, vh: u32) -> Self {
        Self {
            styler,
            node,
            vw,
            vh,
        }
    }

    /// Query a raw CSS property for the current node.
    pub fn get_css_property(&self, prop_id: &PropertyId<'static>) -> Option<&Property<'static>> {
        self.styler.get_raw_property(self.node, prop_id)
    }

    /// Get the node ID.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Extend the lifetime to `'static` for use with the formula system.
    ///
    /// # Safety
    /// This is safe because:
    /// - `NodeStylerContext` only holds a borrowed `&Styler` and a `NodeId`
    /// - The formula system never stores `NodeStylerContext` values — it only
    ///   calls methods on them transiently during resolution
    /// - The caller must ensure the `Styler` reference outlives the resolve context
    pub fn into_static(self) -> NodeStylerContext<'static> {
        // SAFETY: NodeStylerContext<'st> and NodeStylerContext<'static> have identical
        // layout. The reference is only used for transient reads during formula
        // resolution, and the caller ensures the Styler outlives the resolve context.
        unsafe { std::mem::transmute(self) }
    }
}

impl StylerAccess for NodeStylerContext<'_> {
    fn get_property(&self, prop_id: &PropertyId<'static>) -> Option<Subpixel> {
        let prop = self.styler.get_raw_property(self.node, prop_id)?;
        property_to_subpixel(prop, self)
    }

    fn node_id(&self) -> NodeId {
        self.node
    }

    fn related(&self, rel: SingleRelationship) -> Self {
        match rel {
            SingleRelationship::Self_ => {
                NodeStylerContext::new(self.styler, self.node, self.vw, self.vh)
            }
            SingleRelationship::Parent => {
                let parent_id = self.styler.tree().parent(self.node).unwrap_or(NodeId(0));
                NodeStylerContext::new(self.styler, parent_id, self.vw, self.vh)
            }
        }
    }

    fn related_iter(&self, rel: MultiRelationship) -> Vec<Self> {
        let tree = self.styler.tree();
        match rel {
            MultiRelationship::Children => tree
                .children(self.node)
                .map(|id| NodeStylerContext::new(self.styler, id, self.vw, self.vh))
                .collect(),
            MultiRelationship::PrevSiblings => tree
                .prev_siblings(self.node)
                .map(|id| NodeStylerContext::new(self.styler, id, self.vw, self.vh))
                .collect(),
            MultiRelationship::NextSiblings => tree
                .next_siblings(self.node)
                .map(|id| NodeStylerContext::new(self.styler, id, self.vw, self.vh))
                .collect(),
            MultiRelationship::Siblings => tree
                .prev_siblings(self.node)
                .chain(tree.next_siblings(self.node))
                .map(|id| NodeStylerContext::new(self.styler, id, self.vw, self.vh))
                .collect(),
        }
    }

    fn sibling_index(&self) -> usize {
        self.styler.tree().sibling_index(self.node)
    }

    fn viewport_width(&self) -> u32 {
        self.vw
    }

    fn viewport_height(&self) -> u32 {
        self.vh
    }
}

/// Extract a length value from a CSS property and resolve it to pixels.
fn property_to_subpixel(prop: &Property<'static>, styler: &impl StylerAccess) -> Option<Subpixel> {
    use lightningcss::properties::Property::*;
    use lightningcss::properties::size::Size;
    use lightningcss::values::length::LengthPercentageOrAuto;

    match prop {
        Width(size) | Height(size) => match size {
            Size::LengthPercentage(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            _ => None,
        },
        MinWidth(size) | MinHeight(size) => match size {
            Size::LengthPercentage(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            _ => None,
        },
        MaxWidth(size) | MaxHeight(size) => match size {
            lightningcss::properties::size::MaxSize::LengthPercentage(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            _ => None,
        },
        MarginTop(lpa) | MarginBottom(lpa) | MarginLeft(lpa) | MarginRight(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            LengthPercentageOrAuto::Auto => None,
        },
        PaddingTop(lp) | PaddingBottom(lp) | PaddingLeft(lp) | PaddingRight(lp) => match lp {
            LengthPercentageOrAuto::LengthPercentage(lpp) => match lpp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            LengthPercentageOrAuto::Auto => None,
        },
        BorderTopWidth(width)
        | BorderBottomWidth(width)
        | BorderLeftWidth(width)
        | BorderRightWidth(width) => match width {
            lightningcss::properties::border::BorderSideWidth::Length(len) => match len {
                lightningcss::values::length::Length::Value(lv) => Some(resolve_length(lv, styler)),
                lightningcss::values::length::Length::Calc(_) => None,
            },
            _ => None,
        },
        Top(lpa) | Bottom(lpa) | Left(lpa) | Right(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            LengthPercentageOrAuto::Auto => None,
        },
        FontSize(fs) => match fs {
            lightningcss::properties::font::FontSize::Length(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            _ => None,
        },
        LineHeight(lh) => match lh {
            lightningcss::properties::font::LineHeight::Length(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length(len, styler))
                }
                _ => None,
            },
            lightningcss::properties::font::LineHeight::Number(n) => {
                let font_size = styler.get_property(&PropertyId::FontSize).unwrap_or(16);
                Some((*n * font_size as f32) as Subpixel)
            }
            _ => None,
        },
        _ => None,
    }
}
