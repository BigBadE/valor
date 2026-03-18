//! CSS property resolver for formula resolution.
//!
//! Provides an implementation of `PropertyResolver` from `rewrite_core`
//! that queries the Database (cascade + inheritance) on demand.

use crate::Styler;
use crate::value_resolver::NodeContext;
use lightningcss::properties::display::{Display, DisplayInside, DisplayOutside};
use lightningcss::properties::{Property, PropertyId};
use rewrite_core::{Database, NodeId, PropertyResolver, Subpixel, TextMeasurement};
use rewrite_html::NodeData;
use std::sync::Arc;

/// A property resolver that wraps a `Styler` and `Database`, providing
/// CSS property access and tree navigation for formula resolution.
///
/// Unlike the old `NodeStylerContext` which was scoped to a single node,
/// this resolver is shared across all nodes — callers pass `NodeId`
/// explicitly. This eliminates boxing: no `Box<dyn StylerAccess>` is
/// allocated during tree navigation.
pub struct CssPropertyResolver {
    styler: Arc<Styler>,
    db: Arc<Database>,
    vw: u32,
    vh: u32,
}

impl CssPropertyResolver {
    /// Create a new property resolver.
    pub fn new(styler: Arc<Styler>, db: Arc<Database>, vw: u32, vh: u32) -> Self {
        Self { styler, db, vw, vh }
    }

    /// Determine whether a text node is at the start/end of its
    /// containing block for Phase II whitespace trimming.
    fn text_block_boundary(&self, node: NodeId) -> (bool, bool) {
        let tree = self.styler.tree();

        let has_prev_content = tree
            .prev_siblings(node)
            .any(|sib| match tree.get_node(sib) {
                Some(NodeData::Element { .. }) => true,
                Some(NodeData::Text(t)) => !t.trim().is_empty(),
                _ => false,
            });

        let has_next_content = tree
            .next_siblings(node)
            .any(|sib| match tree.get_node(sib) {
                Some(NodeData::Element { .. }) => true,
                Some(NodeData::Text(t)) => !t.trim().is_empty(),
                _ => false,
            });

        let sole_content = !has_prev_content && !has_next_content;
        (sole_content, sole_content)
    }
}

impl PropertyResolver for CssPropertyResolver {
    fn get_property(&self, node: NodeId, prop_id: &PropertyId<'static>) -> Option<Subpixel> {
        let prop = self.db.get_property(node, prop_id.clone())?;
        property_to_subpixel(&prop, node, self)
    }

    fn get_css_property(
        &self,
        node: NodeId,
        prop_id: &PropertyId<'static>,
    ) -> Option<Property<'static>> {
        self.db.get_property(node, prop_id.clone())
    }

    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.styler.tree().parent(node)
    }

    fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.styler.tree().children(node).collect()
    }

    fn prev_siblings(&self, node: NodeId) -> Vec<NodeId> {
        self.styler.tree().prev_siblings(node).collect()
    }

    fn next_siblings(&self, node: NodeId) -> Vec<NodeId> {
        self.styler.tree().next_siblings(node).collect()
    }

    fn viewport_width(&self) -> u32 {
        self.vw
    }

    fn viewport_height(&self) -> u32 {
        self.vh
    }

    fn is_intrinsic(&self, node: NodeId) -> bool {
        self.styler.tree().text_content(node).is_some()
    }

    fn is_element(&self, node: NodeId) -> bool {
        matches!(
            self.styler.tree().get_node(node),
            Some(rewrite_html::NodeData::Element { .. })
        )
    }

    fn text_content(&self, node: NodeId) -> Option<String> {
        let text = self.styler.tree().text_content(node)?;
        if text.trim().is_empty() {
            return None;
        }
        // CSS Text 3 §4.1.1: collapse whitespace.
        let (at_start, at_end) = self.text_block_boundary(node);
        let collapsed = rewrite_text::collapse_whitespace(text, at_start, at_end);
        if collapsed.is_empty() {
            return None;
        }
        Some(collapsed)
    }

    fn measure_text(
        &self,
        node: NodeId,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> Option<TextMeasurement> {
        let font_family = self.db.get_property(node, PropertyId::FontFamily);
        let font_weight = self.db.get_property(node, PropertyId::FontWeight);
        let font_style = self.db.get_property(node, PropertyId::FontStyle);

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
fn resolve_percentage_width(pct: f32, node: NodeId, resolver: &CssPropertyResolver) -> Subpixel {
    // Walk up to find the block container.
    let cb = find_block_container(node, resolver);
    let cb_width = resolver
        .get_property(cb, &PropertyId::Width)
        .unwrap_or_else(|| Subpixel::from_px(resolver.vw as i32));
    Subpixel::from_f32(cb_width.to_f32() * pct)
}

/// Resolve a percentage against the containing block's height.
fn resolve_percentage_height(
    pct: f32,
    node: NodeId,
    resolver: &CssPropertyResolver,
) -> Option<Subpixel> {
    let cb = find_block_container(node, resolver);
    let cb_height = resolver.get_property(cb, &PropertyId::Height)?;
    Some(Subpixel::from_f32(cb_height.to_f32() * pct))
}

/// Find the nearest block container ancestor for a node.
fn find_block_container(node: NodeId, resolver: &CssPropertyResolver) -> NodeId {
    let tree = resolver.styler.tree();
    let mut current = node;
    while let Some(parent_id) = tree.parent(current) {
        let display = resolver.db.get_property(parent_id, PropertyId::Display);
        let is_inline = matches!(
            display,
            Some(Property::Display(Display::Pair(pair)))
                if matches!(pair.outside, DisplayOutside::Inline)
                    && matches!(pair.inside, DisplayInside::Flow)
        );
        if !is_inline {
            return parent_id;
        }
        current = parent_id;
    }
    NodeId::ROOT
}

/// Resolve a `DimensionPercentage` that resolves percentages against width.
fn resolve_dim_pct_width(
    lp: &lightningcss::values::percentage::DimensionPercentage<
        lightningcss::values::length::LengthValue,
    >,
    node: NodeId,
    resolver: &CssPropertyResolver,
) -> Option<Subpixel> {
    use lightningcss::values::percentage::DimensionPercentage::*;
    match lp {
        Dimension(len) => Some(resolve_length_ctx(len, node, resolver)),
        Percentage(pct) => Some(resolve_percentage_width(pct.0, node, resolver)),
        Calc(_) => None,
    }
}

/// Resolve a `DimensionPercentage` that resolves percentages against height.
fn resolve_dim_pct_height(
    lp: &lightningcss::values::percentage::DimensionPercentage<
        lightningcss::values::length::LengthValue,
    >,
    node: NodeId,
    resolver: &CssPropertyResolver,
) -> Option<Subpixel> {
    use lightningcss::values::percentage::DimensionPercentage::*;
    match lp {
        Dimension(len) => Some(resolve_length_ctx(len, node, resolver)),
        Percentage(pct) => resolve_percentage_height(pct.0, node, resolver),
        Calc(_) => None,
    }
}

/// Resolve a length value using the resolver for font-relative units.
fn resolve_length_ctx(
    value: &lightningcss::values::length::LengthValue,
    node: NodeId,
    resolver: &CssPropertyResolver,
) -> Subpixel {
    let ctx = NodeContext {
        node,
        resolver,
    };
    crate::value_resolver::resolve_length(value, &ctx)
}

/// Resolve a length value for font-size (em/lh reference parent).
fn resolve_length_for_font_size_ctx(
    value: &lightningcss::values::length::LengthValue,
    node: NodeId,
    resolver: &CssPropertyResolver,
) -> Subpixel {
    let ctx = NodeContext {
        node,
        resolver,
    };
    crate::value_resolver::resolve_length_for_font_size(value, &ctx)
}

/// Extract a length value from a CSS property and resolve it to pixels.
fn property_to_subpixel(
    prop: &Property<'static>,
    node: NodeId,
    resolver: &CssPropertyResolver,
) -> Option<Subpixel> {
    use lightningcss::properties::Property::*;
    use lightningcss::properties::size::Size;
    use lightningcss::values::length::LengthPercentageOrAuto;

    match prop {
        Width(size) | MinWidth(size) => match size {
            Size::LengthPercentage(lp) => resolve_dim_pct_width(lp, node, resolver),
            _ => None,
        },
        Height(size) | MinHeight(size) => match size {
            Size::LengthPercentage(lp) => resolve_dim_pct_height(lp, node, resolver),
            _ => None,
        },
        MaxWidth(size) => match size {
            lightningcss::properties::size::MaxSize::LengthPercentage(lp) => {
                resolve_dim_pct_width(lp, node, resolver)
            }
            _ => None,
        },
        MaxHeight(size) => match size {
            lightningcss::properties::size::MaxSize::LengthPercentage(lp) => {
                resolve_dim_pct_height(lp, node, resolver)
            }
            _ => None,
        },
        // CSS 2.2 §10.3.3, §10.5.3: margin/padding percentages resolve
        // against the containing block's WIDTH, even for vertical sides.
        MarginTop(lpa) | MarginBottom(lpa) | MarginLeft(lpa) | MarginRight(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => {
                resolve_dim_pct_width(lp, node, resolver)
            }
            LengthPercentageOrAuto::Auto => None,
        },
        PaddingTop(lp) | PaddingBottom(lp) | PaddingLeft(lp) | PaddingRight(lp) => match lp {
            LengthPercentageOrAuto::LengthPercentage(lpp) => {
                resolve_dim_pct_width(lpp, node, resolver)
            }
            LengthPercentageOrAuto::Auto => None,
        },
        BorderTopWidth(width)
        | BorderBottomWidth(width)
        | BorderLeftWidth(width)
        | BorderRightWidth(width) => match width {
            lightningcss::properties::border::BorderSideWidth::Length(len) => match len {
                lightningcss::values::length::Length::Value(lv) => {
                    Some(resolve_length_ctx(lv, node, resolver))
                }
                lightningcss::values::length::Length::Calc(_) => None,
            },
            _ => None,
        },
        Top(lpa) | Bottom(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => {
                resolve_dim_pct_height(lp, node, resolver)
            }
            LengthPercentageOrAuto::Auto => None,
        },
        Left(lpa) | Right(lpa) => match lpa {
            LengthPercentageOrAuto::LengthPercentage(lp) => {
                resolve_dim_pct_width(lp, node, resolver)
            }
            LengthPercentageOrAuto::Auto => None,
        },
        FontSize(fs) => match fs {
            lightningcss::properties::font::FontSize::Length(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length_for_font_size_ctx(len, node, resolver))
                }
                _ => None,
            },
            _ => None,
        },
        LineHeight(lh) => match lh {
            lightningcss::properties::font::LineHeight::Length(lp) => match lp {
                lightningcss::values::percentage::DimensionPercentage::Dimension(len) => {
                    Some(resolve_length_ctx(len, node, resolver))
                }
                _ => None,
            },
            lightningcss::properties::font::LineHeight::Number(n) => {
                let font_size = resolver
                    .get_property(node, &PropertyId::FontSize)
                    .unwrap_or(Subpixel::from_px(16));
                Some(Subpixel::from_f32(*n * font_size.to_f32()))
            }
            _ => None,
        },
        // Flex properties
        FlexGrow(val, _) => Some(Subpixel::from_f32(*val)),
        FlexShrink(val, _) => Some(Subpixel::from_f32(*val)),
        FlexBasis(basis, _) => match basis {
            LengthPercentageOrAuto::LengthPercentage(lp) => {
                resolve_dim_pct_width(lp, node, resolver)
            }
            LengthPercentageOrAuto::Auto => None,
        },
        // Gap properties (CSS Box Alignment §8)
        RowGap(gap) | ColumnGap(gap) => match gap {
            lightningcss::properties::align::GapValue::LengthPercentage(lp) => {
                resolve_dim_pct_width(lp, node, resolver)
            }
            lightningcss::properties::align::GapValue::Normal => None,
        },
        _ => None,
    }
}
