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
            // On Linux, explicitly set font families to match Chrome's defaults.
            // Liberation Serif is metrically compatible with Times New Roman.
            font_system.db_mut().set_serif_family("Liberation Serif");

            // For sans-serif and monospace, we explicitly map them in map_font_family()
            // to avoid fontconfig's incorrect resolution
        }

        let arc = Arc::new(Mutex::new(font_system));
        *guard = Some(Arc::clone(&arc));
        arc
    }
}

/// Map a font family name to a glyphon Family enum.
/// Maps CSS generic font families to cosmic-text Family enum.
///
/// This uses the generic Family variants (SansSerif, Serif, Monospace) to allow fontconfig
/// to resolve fonts on Linux, matching Chrome's behavior. On Linux, fontconfig typically maps:
/// - sans-serif/system-ui → Noto Sans (or other configured sans-serif font)
/// - serif → Liberation Serif or Noto Serif
/// - monospace → DejaVu Sans Mono or Noto Sans Mono
///
/// This ensures we use the same fonts that Chrome uses via fontconfig.
///
/// This is the single source of truth for font family mapping used by both
/// layout (css_text) and rendering (wgpu_backend).
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
        "serif" => {
            // Use Family::Serif to let fontconfig resolve the actual font
            // On Linux, this typically resolves to Liberation Serif or Noto Serif
            // On Windows, this resolves to Times New Roman
            // On macOS, this resolves to Times
            Family::Serif
        }
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
                // CRITICAL: Use explicit font name instead of Family::Monospace.
                // Family::Monospace causes fontconfig to resolve incorrectly,
                // matching to random fonts (DejaVu Serif, Noto Sans Gujarati UI, etc.)
                // DejaVu Sans Mono has correct weight matching: 400→400, 600→700, 700→700
                Family::Name("DejaVu Sans Mono")
            }
        }
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        // For custom font names, we can't return 'static since we'd need to own the string
        // Fall back to serif (the default) for now
        // TODO: Support custom font names by using a different approach
        _ => Family::Serif,
    }
}

/// Get the default font family when no font-family is specified.
///
/// This is the single source of truth for the default font used by both
/// layout (css_text) and rendering (wgpu_backend).
pub fn get_default_font_family() -> Family<'static> {
    // Default to serif to match Chrome's behavior on Linux
    // Chrome on Linux defaults to Times New Roman (Liberation Serif)
    // Use explicit font names instead of Family::Serif to ensure cosmic-text loads the right font
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
        // Use Liberation Serif to match Chrome's default on Linux
        // Note: Liberation Serif on this system has a corrupted 'A' glyph (0.875px too wide)
        // but it's still the closest match overall since Chrome uses Liberation Serif
        Family::Name("Liberation Serif")
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

    // Font matching complete - metrics will be extracted below
    let _ = font_sys.db().face(first_match.id);

    // Get font metrics directly from the font (NO SHAPING!)
    let metrics = font.metrics();
    let units_per_em = f32::from(metrics.units_per_em);

    // Chrome uses different font metric tables depending on the platform:
    // - Windows: OS/2 winAscent + winDescent (no line gap)
    // - Linux: OS/2 typo metrics if USE_TYPO_METRICS flag is set, otherwise hhea metrics
    // - macOS: hhea ascent + descent + leading
    // We need to match this platform-specific behavior for correct text layout.
    #[cfg(target_os = "windows")]
    let (ascent, descent, leading) = {
        // Chrome on Windows uses OS/2 typo metrics when USE_TYPO_METRICS flag is set,
        // otherwise falls back to win metrics (no line gap).
        // This matches Skia's behavior in SkFontHost_FreeType.cpp.
        if let Some((typo_ascent, typo_descent, typo_line_gap, use_typo_metrics)) =
            font.os2_typo_metrics()
        {
            if use_typo_metrics {
                // Use OS/2 typo metrics with line gap
                (typo_ascent, typo_descent, typo_line_gap)
            } else if let Some((win_ascent, win_descent)) = font.os2_metrics() {
                // Use OS/2 win metrics (no line gap) - traditional Windows behavior
                (win_ascent, win_descent, 0.0)
            } else {
                // Fallback to hhea metrics if OS/2 table is missing
                let ascent = metrics.ascent / units_per_em;
                let descent = -metrics.descent / units_per_em;
                let leading = metrics.leading / units_per_em;
                (ascent, descent, leading)
            }
        } else if let Some((win_ascent, win_descent)) = font.os2_metrics() {
            // Use OS/2 win metrics (no line gap) if typo metrics not available
            (win_ascent, win_descent, 0.0)
        } else {
            // Fallback to hhea metrics if OS/2 table is missing
            let ascent = metrics.ascent / units_per_em;
            let descent = -metrics.descent / units_per_em;
            let leading = metrics.leading / units_per_em;
            (ascent, descent, leading)
        }
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let (ascent, descent, leading) = {
        // Chrome on Linux uses OS/2 typo metrics when USE_TYPO_METRICS flag is set,
        // otherwise falls back to hhea metrics.
        if let Some((typo_ascent, typo_descent, typo_line_gap, use_typo_metrics)) =
            font.os2_typo_metrics()
        {
            if use_typo_metrics {
                // Use OS/2 typo metrics with line gap (matches Chrome/Skia on Linux)
                (typo_ascent, typo_descent, typo_line_gap)
            } else {
                // When USE_TYPO_METRICS is not set, Chrome uses hhea metrics
                // This matches: ascent = face->ascender, descent = face->descender,
                // leading = face->height + (face->descender - face->ascender)
                // where face->height = ascender - descender + lineGap
                let ascent = metrics.ascent / units_per_em;
                let descent = -metrics.descent / units_per_em;
                let leading = metrics.leading / units_per_em;
                (ascent, descent, leading)
            }
        } else {
            // Fallback to hhea metrics if OS/2 table is missing
            let ascent = metrics.ascent / units_per_em;
            let descent = -metrics.descent / units_per_em;
            let leading = metrics.leading / units_per_em;
            (ascent, descent, leading)
        }
    };

    #[cfg(target_os = "macos")]
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
