//! Node styler context for formula resolution.
//!
//! Provides an implementation of `StylerAccess` from `rewrite_core`
//! that queries the Database (cascade + inheritance) on demand.

use crate::Styler;
use crate::value_resolver::{resolve_length, resolve_length_for_font_size};
use lightningcss::properties::display::{Display, DisplayInside, DisplayOutside};
use lightningcss::properties::{Property, PropertyId};
use lightningcss::vendor_prefix::VendorPrefix;
use rewrite_core::{
    Database, MultiRelationship, NodeId, SingleRelationship, StylerAccess, Subpixel,
    TextMeasurement,
};
use rewrite_html::NodeData;
use std::sync::Arc;

/// A context scoped to a specific node, providing CSS property access
/// and tree navigation for formula resolution.
///
/// Uses `Arc` for shared ownership of `Styler` and `Database`, so the
/// context is `'static` and can be freely boxed as `dyn StylerAccess`.
///
/// Property queries go through `Database::get_property()` which
/// resolves the full cascade with inheritance for inherited properties.
pub struct NodeStylerContext {
    styler: Arc<Styler>,
    db: Arc<Database>,
    node: NodeId,
    vw: u32,
    vh: u32,
}

impl NodeStylerContext {
    /// Create a new node styler context.
    pub fn new(styler: Arc<Styler>, db: Arc<Database>, node: NodeId, vw: u32, vh: u32) -> Self {
        Self {
            styler,
            db,
            node,
            vw,
            vh,
        }
    }

    /// Get the node ID.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Determine whether this text node is at the start/end of its
    /// containing block for Phase II whitespace trimming.
    ///
    /// Phase II trims collapsible spaces at line boundaries. Without
    /// full inline layout, we approximate: trim only when the text
    /// node is the sole visible content in its parent (no inline
    /// siblings share the line). When other inline content is present,
    /// boundary spaces act as inter-element separators.
    fn text_block_boundary(&self) -> (bool, bool) {
        let tree = self.styler.tree();

        let has_prev_content = tree
            .prev_siblings(self.node)
            .any(|sib| match tree.get_node(sib) {
                Some(NodeData::Element { .. }) => true,
                Some(NodeData::Text(t)) => !t.trim().is_empty(),
                _ => false,
            });

        let has_next_content = tree
            .next_siblings(self.node)
            .any(|sib| match tree.get_node(sib) {
                Some(NodeData::Element { .. }) => true,
                Some(NodeData::Text(t)) => !t.trim().is_empty(),
                _ => false,
            });

        // Only trim when the text is the sole visible content in the
        // block — no adjacent inline siblings share the line.
        let sole_content = !has_prev_content && !has_next_content;
        (sole_content, sole_content)
    }
}

impl StylerAccess for NodeStylerContext {
    fn get_property(&self, prop_id: &PropertyId<'static>) -> Option<Subpixel> {
        let prop = self.db.get_property(self.node, prop_id.clone())?;
        property_to_subpixel(&prop, self)
    }

    fn get_css_property(&self, prop_id: &PropertyId<'static>) -> Option<Property<'static>> {
        self.db.get_property(self.node, prop_id.clone())
    }

    fn node_id(&self) -> NodeId {
        self.node
    }

    fn related(&self, rel: SingleRelationship) -> Box<dyn StylerAccess> {
        match rel {
            SingleRelationship::Self_ => Box::new(NodeStylerContext::new(
                self.styler.clone(),
                self.db.clone(),
                self.node,
                self.vw,
                self.vh,
            )),
            SingleRelationship::Parent => {
                let parent_id = self.styler.tree().parent(self.node).unwrap_or(NodeId(0));
                Box::new(NodeStylerContext::new(
                    self.styler.clone(),
                    self.db.clone(),
                    parent_id,
                    self.vw,
                    self.vh,
                ))
            }
            SingleRelationship::PrevSibling => {
                // Find the closest previous element sibling (skip text nodes).
                let tree = self.styler.tree();
                let prev_id = tree
                    .prev_siblings(self.node)
                    .find(|&id| {
                        matches!(
                            tree.get_node(id),
                            Some(rewrite_html::NodeData::Element { .. })
                        )
                    })
                    .unwrap_or(self.node);
                Box::new(NodeStylerContext::new(
                    self.styler.clone(),
                    self.db.clone(),
                    prev_id,
                    self.vw,
                    self.vh,
                ))
            }
            SingleRelationship::BlockContainer => {
                // Walk up ancestors to find the nearest block container
                // (an ancestor whose display is not inline).
                let tree = self.styler.tree();
                let mut current = self.node;
                while let Some(parent_id) = tree.parent(current) {
                    let display = self.db.get_property(parent_id, PropertyId::Display);
                    let is_inline = matches!(
                        display,
                        Some(Property::Display(Display::Pair(pair)))
                            if matches!(pair.outside, DisplayOutside::Inline)
                                && matches!(pair.inside, DisplayInside::Flow)
                    );
                    if !is_inline {
                        return Box::new(NodeStylerContext::new(
                            self.styler.clone(),
                            self.db.clone(),
                            parent_id,
                            self.vw,
                            self.vh,
                        ));
                    }
                    current = parent_id;
                }
                // Fallback: root node.
                Box::new(NodeStylerContext::new(
                    self.styler.clone(),
                    self.db.clone(),
                    NodeId::ROOT,
                    self.vw,
                    self.vh,
                ))
            }
        }
    }

    fn related_iter(&self, rel: MultiRelationship) -> Vec<Box<dyn StylerAccess>> {
        let tree = self.styler.tree();

        // Helper: get CSS `order` property value for a node (defaults to 0).
        let get_order = |node: NodeId| -> i32 {
            match self
                .db
                .get_property(node, PropertyId::Order(VendorPrefix::None))
            {
                Some(Property::Order(val, _)) => val,
                _ => 0,
            }
        };

        match rel {
            MultiRelationship::OrderedChildren => {
                // tree.children() returns reverse DOM order; reverse to DOM order
                // before sorting so that items with equal `order` keep DOM order.
                let mut children: Vec<NodeId> = tree.children(self.node).collect();
                children.reverse();
                children.sort_by_key(|id| get_order(*id));
                children
                    .into_iter()
                    .map(|id| {
                        Box::new(NodeStylerContext::new(
                            self.styler.clone(),
                            self.db.clone(),
                            id,
                            self.vw,
                            self.vh,
                        )) as Box<dyn StylerAccess>
                    })
                    .collect()
            }
            MultiRelationship::OrderedPrevSiblings => {
                // Get parent, then all siblings sorted by order.
                // "Previous" = all siblings that appear before this node in sorted order.
                let parent = tree.parent(self.node).unwrap_or(self.node);
                let mut siblings: Vec<NodeId> = tree.children(parent).collect();
                siblings.reverse(); // reverse DOM order → DOM order
                siblings.sort_by_key(|id| get_order(*id));
                // Find this node's position and return everything before it.
                let pos = siblings.iter().position(|id| *id == self.node).unwrap_or(0);
                siblings[..pos]
                    .iter()
                    .map(|id| {
                        Box::new(NodeStylerContext::new(
                            self.styler.clone(),
                            self.db.clone(),
                            *id,
                            self.vw,
                            self.vh,
                        )) as Box<dyn StylerAccess>
                    })
                    .collect()
            }
            _ => {
                let iter: Box<dyn Iterator<Item = NodeId> + '_> = match rel {
                    MultiRelationship::Children => Box::new(tree.children(self.node)),
                    MultiRelationship::PrevSiblings => Box::new(tree.prev_siblings(self.node)),
                    MultiRelationship::NextSiblings => Box::new(tree.next_siblings(self.node)),
                    MultiRelationship::Siblings => Box::new(
                        tree.prev_siblings(self.node)
                            .chain(tree.next_siblings(self.node)),
                    ),
                    MultiRelationship::OrderedChildren | MultiRelationship::OrderedPrevSiblings => {
                        unreachable!()
                    }
                };
                iter.map(|id| {
                    Box::new(NodeStylerContext::new(
                        self.styler.clone(),
                        self.db.clone(),
                        id,
                        self.vw,
                        self.vh,
                    )) as Box<dyn StylerAccess>
                })
                .collect()
            }
        }
    }

    fn viewport_width(&self) -> u32 {
        self.vw
    }

    fn viewport_height(&self) -> u32 {
        self.vh
    }

    fn root(&self) -> Box<dyn StylerAccess> {
        let tree = self.styler.tree();
        let root_element = tree
            .children(NodeId::ROOT)
            .find(|child| matches!(tree.nodes[child.0 as usize], NodeData::Element { .. }))
            .unwrap_or(NodeId::ROOT);
        Box::new(NodeStylerContext::new(
            self.styler.clone(),
            self.db.clone(),
            root_element,
            self.vw,
            self.vh,
        ))
    }

    fn is_intrinsic(&self) -> bool {
        self.styler.tree().text_content(self.node).is_some()
    }

    fn text_content(&self) -> Option<String> {
        let text = self.styler.tree().text_content(self.node)?;
        if text.trim().is_empty() {
            return None;
        }
        // CSS Text 3 §4.1.1: collapse whitespace.
        let (at_start, at_end) = self.text_block_boundary();
        let collapsed = rewrite_text::collapse_whitespace(text, at_start, at_end);
        if collapsed.is_empty() {
            return None;
        }
        Some(collapsed)
    }

    fn measure_text(&self, text: &str, max_width: Option<f32>) -> Option<TextMeasurement> {
        let font_family = self.db.get_property(self.node, PropertyId::FontFamily);
        let font_weight = self.db.get_property(self.node, PropertyId::FontWeight);
        let font_style = self.db.get_property(self.node, PropertyId::FontStyle);
        let font_size_prop = self.db.get_property(self.node, PropertyId::FontSize);

        let font_size = font_size_prop
            .as_ref()
            .and_then(|prop| property_to_subpixel(prop, self))
            .unwrap_or(Subpixel::from_px(16))
            .to_f32();

        let attrs = rewrite_text::build_attrs(
            font_family.as_ref(),
            font_weight.as_ref(),
            font_style.as_ref(),
        );

        let font_sys = rewrite_text::get_font_system();
        let mut font_sys_guard = font_sys.lock().unwrap_or_else(|err| err.into_inner());

        if let Some(max_w) = max_width {
            let wrapped = rewrite_text::measure_text_wrapped(
                &mut font_sys_guard,
                text,
                &attrs,
                font_size,
                max_w,
            );
            Some(TextMeasurement {
                width: wrapped.max_line_width,
                height: wrapped.total_height,
                ascent: wrapped.ascent,
                descent: wrapped.descent,
            })
        } else {
            let metrics = rewrite_text::measure_text(&mut font_sys_guard, text, &attrs, font_size);
            Some(TextMeasurement {
                width: metrics.width,
                height: metrics.height,
                ascent: metrics.ascent,
                descent: metrics.descent,
            })
        }
    }
}

/// Resolve a percentage against the containing block's width.
///
/// Per CSS 2.2 §10.2, §10.3, §10.5: percentages on width, margin, and
/// padding resolve against the containing block's width (even vertical
/// margins/padding use the containing block's WIDTH, not height).
fn resolve_percentage_width(pct: f32, styler: &dyn StylerAccess) -> Subpixel {
    let cb = styler.related(SingleRelationship::BlockContainer);
    let cb_width = cb
        .get_property(&PropertyId::Width)
        .unwrap_or_else(|| Subpixel::from_px(styler.viewport_width() as i32));
    Subpixel::from_f32(cb_width.to_f32() * pct)
}

/// Resolve a percentage against the containing block's height.
///
/// Per CSS 2.2 §10.5: height percentages resolve against the containing
/// block's height. If the containing block's height is auto, the
/// percentage is treated as auto (returns None).
fn resolve_percentage_height(pct: f32, styler: &dyn StylerAccess) -> Option<Subpixel> {
    let cb = styler.related(SingleRelationship::BlockContainer);
    let cb_height = cb.get_property(&PropertyId::Height)?;
    Some(Subpixel::from_f32(cb_height.to_f32() * pct))
}

/// Resolve a `DimensionPercentage` that resolves percentages against width.
fn resolve_dim_pct_width(
    lp: &lightningcss::values::percentage::DimensionPercentage<
        lightningcss::values::length::LengthValue,
    >,
    styler: &dyn StylerAccess,
) -> Option<Subpixel> {
    use lightningcss::values::percentage::DimensionPercentage::*;
    match lp {
        Dimension(len) => Some(resolve_length(len, styler)),
        Percentage(pct) => Some(resolve_percentage_width(pct.0, styler)),
        Calc(_) => None,
    }
}

/// Resolve a `DimensionPercentage` that resolves percentages against height.
fn resolve_dim_pct_height(
    lp: &lightningcss::values::percentage::DimensionPercentage<
        lightningcss::values::length::LengthValue,
    >,
    styler: &dyn StylerAccess,
) -> Option<Subpixel> {
    use lightningcss::values::percentage::DimensionPercentage::*;
    match lp {
        Dimension(len) => Some(resolve_length(len, styler)),
        Percentage(pct) => resolve_percentage_height(pct.0, styler),
        Calc(_) => None,
    }
}

/// Extract a length value from a CSS property and resolve it to pixels.
fn property_to_subpixel(prop: &Property<'static>, styler: &dyn StylerAccess) -> Option<Subpixel> {
    use lightningcss::properties::Property::*;
    use lightningcss::properties::size::Size;
    use lightningcss::values::length::LengthPercentageOrAuto;

    match prop {
        Width(size) | MinWidth(size) => match size {
            Size::LengthPercentage(lp) => resolve_dim_pct_width(lp, styler),
            _ => None,
        },
        Height(size) | MinHeight(size) => match size {
            Size::LengthPercentage(lp) => resolve_dim_pct_height(lp, styler),
            _ => None,
        },
        MaxWidth(size) => match size {
            lightningcss::properties::size::MaxSize::LengthPercentage(lp) => {
                resolve_dim_pct_width(lp, styler)
            }
            _ => None,
        },
        MaxHeight(size) => match size {
            lightningcss::properties::size::MaxSize::LengthPercentage(lp) => {
                resolve_dim_pct_height(lp, styler)
            }
            _ => None,
        },
        // CSS 2.2 §10.3.3, §10.5.3: margin/padding percentages resolve
        // against the containing block's WIDTH, even for vertical sides.
        MarginTop(lpa) | MarginBottom(lpa) | MarginLeft(lpa) | MarginRight(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => resolve_dim_pct_width(lp, styler),
            LengthPercentageOrAuto::Auto => None,
        },
        PaddingTop(lp) | PaddingBottom(lp) | PaddingLeft(lp) | PaddingRight(lp) => match lp {
            LengthPercentageOrAuto::LengthPercentage(lpp) => resolve_dim_pct_width(lpp, styler),
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
        Top(lpa) | Bottom(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => resolve_dim_pct_height(lp, styler),
            LengthPercentageOrAuto::Auto => None,
        },
        Left(lpa) | Right(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => resolve_dim_pct_width(lp, styler),
            LengthPercentageOrAuto::Auto => None,
        },
        FontSize(fs) => match fs {
            lightningcss::properties::font::FontSize::Length(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    // em/lh on font-size refer to the inherited (parent's) value.
                    Some(resolve_length_for_font_size(len, styler))
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
                let font_size = styler
                    .get_property(&PropertyId::FontSize)
                    .unwrap_or(Subpixel::from_px(16));
                Some(Subpixel::from_f32(*n * font_size.to_f32()))
            }
            _ => None,
        },
        // Flex properties: grow/shrink are plain numbers encoded as Subpixel
        // for formula arithmetic. FlexBasis is a length/percentage/auto.
        FlexGrow(val, _) => Some(Subpixel::from_f32(*val)),
        FlexShrink(val, _) => Some(Subpixel::from_f32(*val)),
        FlexBasis(basis, _) => match basis {
            LengthPercentageOrAuto::LengthPercentage(lp) => resolve_dim_pct_width(lp, styler),
            LengthPercentageOrAuto::Auto => None,
        },
        // Gap properties (CSS Box Alignment §8)
        RowGap(gap) | ColumnGap(gap) => match gap {
            lightningcss::properties::align::GapValue::LengthPercentage(lp) => {
                resolve_dim_pct_width(lp, styler)
            }
            lightningcss::properties::align::GapValue::Normal => None,
        },
        _ => None,
    }
}
