//! Font system initialization and management.

use glyphon::{Attrs, Family, FontSystem, fontdb};
use std::sync::{Arc, Mutex, PoisonError};

type FontSystemOption = Option<Arc<Mutex<FontSystem>>>;

/// Global font system for text measurement.
/// This is lazy-initialized on first use and reused throughout the application.
static FONT_SYSTEM: Mutex<FontSystemOption> = Mutex::new(None);

/// Get or initialize the global font system.
pub fn get_font_system() -> Arc<Mutex<FontSystem>> {
    let mut guard = FONT_SYSTEM.lock().unwrap_or_else(PoisonError::into_inner);

    if let Some(ref font_sys) = *guard {
        Arc::clone(font_sys)
    } else {
        let mut font_system = FontSystem::new();

        // Load system fonts
        font_system.db_mut().load_system_fonts();

        // Set generic font families to match Chrome's defaults on each platform
        #[cfg(target_os = "windows")]
        {
            // Chrome uses these fonts as defaults on Windows
            font_system.db_mut().set_monospace_family("Consolas");
            font_system.db_mut().set_sans_serif_family("Arial");
            font_system.db_mut().set_serif_family("Times New Roman");
        }

        #[cfg(target_os = "macos")]
        {
            // Chrome uses these fonts as defaults on macOS
            font_system.db_mut().set_monospace_family("Menlo");
            font_system.db_mut().set_sans_serif_family("Helvetica");
            font_system.db_mut().set_serif_family("Times");
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // Chrome uses these fonts as defaults on Linux
            // DejaVu fonts are widely available and match Chrome's behavior
            font_system
                .db_mut()
                .set_monospace_family("DejaVu Sans Mono");
            font_system.db_mut().set_sans_serif_family("DejaVu Sans");
            font_system.db_mut().set_serif_family("DejaVu Serif");
        }

        let arc = Arc::new(Mutex::new(font_system));
        *guard = Some(Arc::clone(&arc));
        arc
    }
}

/// Map a font family name to a glyphon Family enum.
/// Maps generic families to match Chrome's default font choices on each platform.
///
/// This function ensures consistent font selection between layout (measurement) and rendering.
/// We explicitly use `Family::Name` with the specific font we configured in fontdb to ensure
/// consistent metrics. Using the generic `Family::SansSerif` etc. can sometimes pick different
/// fonts depending on fontdb's internal logic.
pub fn map_font_family(font_name: &str) -> Family<'_> {
    match font_name.to_lowercase().as_str() {
        "sans-serif" => {
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
                Family::Name("DejaVu Sans")
            }
        }
        "serif" => {
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
                Family::Name("DejaVu Serif")
            }
        }
        "monospace" => {
            #[cfg(target_os = "windows")]
            {
                Family::Name("Consolas")
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
        _ => Family::Name(font_name),
    }
}

/// Font metrics from actual font file.
#[derive(Debug, Clone, Copy)]
pub struct FontMetricsData {
    /// Ascent in font units (normalized to 0-1 range).
    pub ascent: f32,
    /// Descent in font units (normalized to 0-1 range, positive value).
    pub descent: f32,
    /// Leading (line-gap) in font units (normalized to 0-1 range).
    /// This is the recommended additional spacing between lines.
    pub leading: f32,
}

/// Get font metrics (ascent + descent + leading) from actual font file.
/// This directly accesses font metrics without shaping, matching what Chromium does.
///
/// Returns `None` if no font matches are found or if the font fails to load.
pub fn get_font_metrics(font_sys: &mut FontSystem, attrs: &Attrs<'_>) -> Option<FontMetricsData> {
    use fontdb::Weight;

    // Get font matches for the given attributes
    let font_matches = font_sys.get_font_matches(attrs);

    // Get the first font match (the default font for these attributes)
    let first_match = font_matches.first()?;

    // Convert weight u16 to fontdb::Weight
    let weight = Weight(first_match.font_weight);

    // Get the actual Font object
    let font = font_sys.get_font(first_match.id, weight)?;

    // Get font metrics directly from the font (NO SHAPING!)
    let metrics = font.metrics();
    let units_per_em = f32::from(metrics.units_per_em);

    // On Windows, Chrome uses OS/2 winAscent + winDescent for line-height calculation.
    // On macOS/Linux, it uses hhea ascent + descent + leading.
    // We need to match this platform-specific behavior for correct text layout.
    #[cfg(target_os = "windows")]
    let (ascent, descent, leading) = {
        if let Some((win_ascent, win_descent)) = font.os2_metrics() {
            // Use OS/2 table metrics (what Chrome uses on Windows)
            (win_ascent, win_descent, 0.0)
        } else {
            // Fallback to hhea metrics if OS/2 table is missing
            let ascent = metrics.ascent / units_per_em;
            let descent = -metrics.descent / units_per_em;
            let leading = metrics.leading / units_per_em;
            (ascent, descent, leading)
        }
    };

    #[cfg(not(target_os = "windows"))]
    let (ascent, descent, leading) = {
        let ascent = metrics.ascent / units_per_em;
        let descent = -metrics.descent / units_per_em; // Note: descent is negative in font metrics
        let leading = metrics.leading / units_per_em; // Line-gap for "normal" line-height
        (ascent, descent, leading)
    };

    Some(FontMetricsData {
        ascent,
        descent,
        leading,
    })
}
