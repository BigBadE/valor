use crate::state::Layer;
use core::mem::take;
use renderer::display_list::{DisplayItem, DisplayList, Scissor, StackingContextBoundary};
use renderer::renderer::DrawText;

/// A batch of text items with an optional scissor rectangle.
pub type TextBatch = (Option<Scissor>, Vec<DrawText>);

/// Flush current text batch if non-empty.
fn flush_batch(out: &mut Vec<TextBatch>, scissor: Option<Scissor>, texts: &mut Vec<DrawText>) {
    if !texts.is_empty() {
        out.push((scissor, take(texts)));
    }
}

/// Batch text items from a display list with scissor rectangles.
pub fn batch_texts_with_scissor(
    display_list: &DisplayList,
    framebuffer_w: u32,
    framebuffer_h: u32,
) -> Vec<TextBatch> {
    let mut out: Vec<TextBatch> = Vec::new();
    let mut stack: Vec<Scissor> = Vec::new();
    let mut current_scissor: Option<Scissor> = None;
    let mut current_texts: Vec<DrawText> = Vec::new();
    let mut sc_stack_is_opacity: Vec<bool> = Vec::new();
    let mut opacity_depth: usize = 0;
    for item in &display_list.items {
        match item {
            DisplayItem::BeginClip {
                x,
                y,
                width,
                height,
            } => {
                flush_batch(&mut out, current_scissor, &mut current_texts);
                let new_scissor =
                    rect_to_scissor((framebuffer_w, framebuffer_h), *x, *y, *width, *height);
                let effective = current_scissor.map_or(new_scissor, |scissor| {
                    intersect_scissor(scissor, new_scissor)
                });
                stack.push(new_scissor);
                current_scissor = Some(effective);
            }
            DisplayItem::EndClip => {
                flush_batch(&mut out, current_scissor, &mut current_texts);
                stack.pop();
                current_scissor = stack.iter().copied().reduce(intersect_scissor);
            }
            DisplayItem::BeginStackingContext { boundary } => {
                let is_opacity =
                    matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0);
                sc_stack_is_opacity.push(is_opacity);
                if is_opacity {
                    opacity_depth += 1;
                }
            }
            DisplayItem::EndStackingContext => {
                if sc_stack_is_opacity.pop().unwrap_or(false) && opacity_depth > 0 {
                    opacity_depth -= 1;
                }
            }
            DisplayItem::Text { .. } => {
                if opacity_depth == 0
                    && let Some(text_item) = map_text_item(item)
                {
                    current_texts.push(text_item);
                }
            }
            DisplayItem::Rect { .. }
            | DisplayItem::Border { .. }
            | DisplayItem::BoxShadow { .. }
            | DisplayItem::Image { .. }
            | DisplayItem::LinearGradient { .. }
            | DisplayItem::RadialGradient { .. } => {}
        }
    }
    flush_batch(&mut out, current_scissor, &mut current_texts);
    out
}

/// Process a single display list layer and extract text batches with scissor rects.
fn batch_single_layer(
    display_list: &DisplayList,
    framebuffer_w: u32,
    framebuffer_h: u32,
    out: &mut Vec<TextBatch>,
) {
    let mut stack: Vec<Scissor> = Vec::new();
    let mut current_scissor: Option<Scissor> = None;
    let mut current_texts: Vec<DrawText> = Vec::new();
    let mut sc_stack_is_opacity: Vec<bool> = Vec::new();
    let mut opacity_depth: usize = 0;

    for item in &display_list.items {
        match item {
            DisplayItem::BeginClip {
                x,
                y,
                width,
                height,
            } => {
                flush_batch(out, current_scissor, &mut current_texts);
                let new_scissor =
                    rect_to_scissor((framebuffer_w, framebuffer_h), *x, *y, *width, *height);
                let effective = current_scissor.map_or(new_scissor, |scissor| {
                    intersect_scissor(scissor, new_scissor)
                });
                stack.push(new_scissor);
                current_scissor = Some(effective);
            }
            DisplayItem::EndClip => {
                flush_batch(out, current_scissor, &mut current_texts);
                stack.pop();
                current_scissor = stack.iter().copied().reduce(intersect_scissor);
            }
            DisplayItem::BeginStackingContext { boundary } => {
                let is_opacity =
                    matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0);
                sc_stack_is_opacity.push(is_opacity);
                if is_opacity {
                    opacity_depth += 1;
                }
            }
            DisplayItem::EndStackingContext => {
                if sc_stack_is_opacity.pop().unwrap_or(false) && opacity_depth > 0 {
                    opacity_depth -= 1;
                }
            }
            DisplayItem::Text { .. } => {
                if opacity_depth == 0
                    && let Some(text_item) = map_text_item(item)
                {
                    current_texts.push(text_item);
                }
            }
            DisplayItem::Rect { .. }
            | DisplayItem::Border { .. }
            | DisplayItem::BoxShadow { .. }
            | DisplayItem::Image { .. }
            | DisplayItem::LinearGradient { .. }
            | DisplayItem::RadialGradient { .. } => {}
        }
    }
    if !current_texts.is_empty() {
        out.push((current_scissor, current_texts));
    }
}

/// Batch text items from multiple layers with scissor rectangles.
pub fn batch_layer_texts_with_scissor(
    layers: &[Layer],
    framebuffer_w: u32,
    framebuffer_h: u32,
) -> Vec<TextBatch> {
    let mut out: Vec<TextBatch> = Vec::new();
    for layer in layers {
        let display_list = match layer {
            Layer::Content(list) | Layer::Chrome(list) => list,
            Layer::Background => continue,
        };
        batch_single_layer(display_list, framebuffer_w, framebuffer_h, &mut out);
    }
    out
}

/// Map a display item to a text draw command if it's a text item.
pub fn map_text_item(item: &DisplayItem) -> Option<DrawText> {
    if let DisplayItem::Text {
        x,
        y,
        text,
        color,
        font_size,
        font_weight,
        font_family,
        line_height,
        bounds,
    } = item
    {
        return Some(DrawText {
            x: *x,
            y: *y,
            text: text.clone(),
            color: *color,
            font_size: *font_size,
            font_weight: *font_weight,
            font_family: font_family.clone(),
            line_height: *line_height,
            bounds: *bounds,
        });
    }
    None
}

/// Convert a rectangle to scissor coordinates, clamping to framebuffer bounds.
fn rect_to_scissor(framebuffer: (u32, u32), x: f32, y: f32, width: f32, height: f32) -> Scissor {
    let framebuffer_width = framebuffer.0.max(1);
    let framebuffer_height = framebuffer.1.max(1);

    let positive_x = x.max(0.0);
    let positive_y = y.max(0.0);
    let start_horizontal = positive_x.floor().min(u32::MAX as f32);
    let start_vertical = positive_y.floor().min(u32::MAX as f32);

    let end_horizontal = (positive_x + width.max(0.0)).ceil().min(u32::MAX as f32);
    let end_vertical = (positive_y + height.max(0.0)).ceil().min(u32::MAX as f32);

    let scissor_x = (start_horizontal as u32).min(framebuffer_width);
    let scissor_y = (start_vertical as u32).min(framebuffer_height);
    let end_x = (end_horizontal as u32).min(framebuffer_width);
    let end_y = (end_vertical as u32).min(framebuffer_height);

    let final_width = end_x.saturating_sub(scissor_x);
    let final_height = end_y.saturating_sub(scissor_y);

    (scissor_x, scissor_y, final_width, final_height)
}

/// Compute the intersection of two scissor rectangles.
fn intersect_scissor(rect_a: Scissor, rect_b: Scissor) -> Scissor {
    let (a_x, a_y, a_width, a_height) = rect_a;
    let (b_x, b_y, b_width, b_height) = rect_b;
    let x_min = a_x.max(b_x);
    let y_min = a_y.max(b_y);
    let x_max = (a_x + a_width).min(b_x + b_width);
    let y_max = (a_y + a_height).min(b_y + b_height);
    let width = x_max.saturating_sub(x_min);
    let height = y_max.saturating_sub(y_min);
    (x_min, y_min, width, height)
}
