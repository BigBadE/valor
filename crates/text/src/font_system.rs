//! Global font system initialization and font family mapping.

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};
use std::sync::{Arc, Mutex, PoisonError};

type FontSystemOption = Option<Arc<Mutex<FontSystem>>>;

/// Global font system singleton — lazy-initialized on first use.
static FONT_SYSTEM: Mutex<FontSystemOption> = Mutex::new(None);

/// Get or initialize the global font system.
pub fn get_font_system() -> Arc<Mutex<FontSystem>> {
    let mut guard = FONT_SYSTEM.lock().unwrap_or_else(PoisonError::into_inner);

    if let Some(ref font_sys) = *guard {
        Arc::clone(font_sys)
    } else {
        let mut font_system = FontSystem::new();

        font_system.db_mut().load_system_fonts();

        // Set generic font families to match Chrome's defaults per platform.
        #[cfg(target_os = "windows")]
        {
            font_system.db_mut().set_monospace_family("Consolas");
            font_system.db_mut().set_sans_serif_family("Arial");
            font_system.db_mut().set_serif_family("Times New Roman");
        }

        #[cfg(target_os = "macos")]
        {
            font_system.db_mut().set_monospace_family("Menlo");
            font_system.db_mut().set_sans_serif_family("Helvetica");
            font_system.db_mut().set_serif_family("Times");
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            font_system.db_mut().set_serif_family("Liberation Serif");
        }

        let arc = Arc::new(Mutex::new(font_system));
        *guard = Some(Arc::clone(&arc));
        arc
    }
}

/// Map a CSS font family name to a cosmic-text `Family`.
///
/// Single source of truth for font family mapping used by both
/// measurement and rendering.
pub fn map_font_family(font_name: &str) -> Family<'static> {
    match font_name.to_lowercase().as_str() {
        "system-ui" | "-apple-system" | "blinkmacsystemfont" | "sans-serif" => {
            #[cfg(target_os = "windows")]
            {
                Family::Name("Arial")
            }
            #[cfg(target_os = "macos")]
            {
                Family::Name("Helvetica")
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                Family::Name("Liberation Sans")
            }
        }
        // On Linux, Family::Monospace causes fontconfig misresolution.
        // Use an explicit font name instead.
        "monospace" => {
            #[cfg(target_os = "windows")]
            {
                Family::Name("Courier New")
            }
            #[cfg(target_os = "macos")]
            {
                Family::Name("Menlo")
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                Family::Name("DejaVu Sans Mono")
            }
        }
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        // "serif" and everything else → Serif
        _ => Family::Serif,
    }
}

/// Default font family when none is specified in CSS.
pub fn default_font_family() -> Family<'static> {
    #[cfg(target_os = "windows")]
    {
        Family::Name("Times New Roman")
    }
    #[cfg(target_os = "macos")]
    {
        Family::Name("Times")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Family::Name("Liberation Serif")
    }
}

/// Raw font metrics extracted at a specific font size.
///
/// Ascent, descent, and leading are in **pixels** (already scaled by
/// font-size and separately rounded to match Chrome's behaviour).
#[derive(Debug, Clone, Copy)]
pub struct FontMetricsData {
    /// Ascent above baseline in px (rounded).
    pub ascent: f32,
    /// Descent below baseline in px (rounded, positive).
    pub descent: f32,
    /// Line height in px (ascent + descent + leading, each rounded separately).
    pub line_height: f32,
}

/// Extract font metrics by shaping a reference character at the given
/// size.  Uses `LayoutLine::max_ascent`/`max_descent` which reflect
/// the font tables after cosmic-text has matched the font.
pub fn get_font_metrics(
    font_sys: &mut FontSystem,
    attrs: &Attrs<'_>,
    font_size: f32,
) -> Option<FontMetricsData> {
    // Use a large enough line_height so shaping is not constrained.
    let metrics = Metrics::new(font_size, font_size * 2.0);
    let mut buffer = Buffer::new(font_sys, metrics);
    buffer.set_size(font_sys, None, None);
    buffer.set_wrap(font_sys, Wrap::None);
    buffer.set_text(font_sys, "x", attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_sys, false);

    // Get the first layout line's metrics.
    let layout = buffer.line_layout(font_sys, 0)?;
    let line = layout.first()?;

    let ascent = line.max_ascent.round();
    let descent = line.max_descent.round();
    // Compute leading from the difference between cosmic-text's
    // reported line_height and ascent+descent.
    let leading = line
        .line_height_opt
        .map_or(0.0, |line_h| (line_h - ascent - descent).max(0.0).round());
    let line_height = ascent + descent + leading;

    Some(FontMetricsData {
        ascent,
        descent,
        line_height,
    })
}
