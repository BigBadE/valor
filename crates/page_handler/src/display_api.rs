use crate::state::HtmlPage;
use anyhow::Error;
use wgpu_renderer::{DisplayList, DrawRect, DrawText};

impl HtmlPage {
    /// Build a simple display list of rectangles from the current layout geometry and styles.
    pub fn display_list_snapshot(&mut self) -> Result<Vec<DrawRect>, Error> {
        self.layouter_try_update_sync()?;
        let rects = self.layouter_geometry_mut();
        let snapshot = self.layouter_snapshot_mut();
        Ok(self.display_builder().build_rect_list(&rects, &snapshot))
    }

    /// Build a retained display list combining rectangles, text, and overlays.
    pub fn display_list_retained_snapshot(&mut self) -> Result<DisplayList, Error> {
        // Ensure mirrors are up-to-date before snapshotting for rendering.
        self.layouter_try_update_sync()?;
        // Guarantee at least one layout pass so geometry isn't empty when taking a snapshot.
        self.ensure_layout_now();
        let rects = self.layouter_geometry_mut();
        let snapshot = self.layouter_snapshot_mut();
        // Use layouter's view for styles to avoid recomputation here
        let computed_map = self.layouter_computed_styles();
        let robust_styles = computed_map.clone();
        let inputs = crate::display::RetainedInputs {
            rects,
            snapshot,
            computed_map: computed_map.clone(),
            computed_fallback: computed_map.clone(),
            computed_robust: Some(robust_styles),
            selection_overlay: self.selection_overlay(),
            focused_node: self.focused_node(),
            hud_enabled: self.hud_enabled(),
            spillover_deferred: self.frame_spillover_deferred(),
            last_style_restyled_nodes: self.last_style_restyled_nodes(),
        };
        Ok(self.display_builder().build_retained(inputs))
    }

    pub fn text_list_snapshot(&mut self) -> Result<Vec<DrawText>, Error> {
        // Drain updates for consistency
        self.layouter_try_update_sync()?;
        // Gather geometry and snapshot for finding text nodes
        let rects = self.layouter_geometry_mut();
        let snapshot = self.layouter_snapshot_mut();
        let computed_map = self.layouter_computed_styles();
        Ok(self
            .display_builder()
            .build_text_list(&rects, &snapshot, &computed_map))
    }

    /// Return an approximate background color for the page by inspecting the body's
    /// computed background-color; fall back to html, then white.
    pub fn background_rgba(&mut self) -> [f32; 4] {
        // Drain mirrors to have latest layout and styles
        let _ = self.layouter_try_update_sync();
        let computed = self.layouter_computed_styles();
        let snapshot = self.layouter_snapshot();
        // Find body, then html
        let mut body_key: Option<js::NodeKey> = None;
        let mut html_key: Option<js::NodeKey> = None;
        for (key, kind, _children) in snapshot.iter() {
            if let layouter::LayoutNodeKind::Block { tag } = kind {
                if tag.eq_ignore_ascii_case("body") {
                    body_key = Some(*key);
                }
                if tag.eq_ignore_ascii_case("html") {
                    html_key = Some(*key);
                }
            }
        }
        let pick = body_key.or(html_key);
        if let Some(k) = pick
            && let Some(cs) = computed.get(&k)
        {
            let c = cs.background_color;
            // If transparent (alpha == 0), default to white canvas background like browsers.
            if c.alpha == 0 {
                return [1.0, 1.0, 1.0, 1.0];
            }
            // Force opaque alpha for canvas clear; background painting of elements can carry alpha separately.
            return [
                c.red as f32 / 255.0,
                c.green as f32 / 255.0,
                c.blue as f32 / 255.0,
                1.0,
            ];
        }
        // Fallback: white background
        [1.0, 1.0, 1.0, 1.0]
    }

    /// Hit-test screen coordinates against current layout boxes and return the topmost NodeKey.
    pub fn hit_test(&mut self, x: i32, y: i32) -> Option<js::NodeKey> {
        if self.layouter_try_update_sync().is_err() {
            return None;
        }
        self.layouter_hit_test(x, y)
    }
}
