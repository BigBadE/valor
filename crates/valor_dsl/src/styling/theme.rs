//! Global theme configuration

use std::collections::HashMap;

/// Color palette for the application
#[derive(Clone, Debug)]
pub struct ColorPalette {
    pub primary: String,
    pub secondary: String,
    pub background: String,
    pub text: String,
    pub accent: String,
    pub error: String,
    pub warning: String,
    pub success: String,
    /// Custom colors
    pub custom: HashMap<String, String>,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self {
            primary: "#667eea".to_string(),
            secondary: "#764ba2".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
            accent: "#f59e0b".to_string(),
            error: "#ef4444".to_string(),
            warning: "#f59e0b".to_string(),
            success: "#10b981".to_string(),
            custom: HashMap::new(),
        }
    }
}

/// Spacing scale (in pixels)
#[derive(Clone, Debug)]
pub struct Spacing {
    pub xs: u32,
    pub sm: u32,
    pub md: u32,
    pub lg: u32,
    pub xl: u32,
    pub xxl: u32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: 4,
            sm: 8,
            md: 16,
            lg: 24,
            xl: 32,
            xxl: 48,
        }
    }
}

/// Typography configuration
#[derive(Clone, Debug)]
pub struct Typography {
    pub font_family: String,
    pub base_size: u32,
    pub line_height: f32,
    pub heading_font: String,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font_family: "system-ui, -apple-system, sans-serif".to_string(),
            base_size: 16,
            line_height: 1.5,
            heading_font: "system-ui, -apple-system, sans-serif".to_string(),
        }
    }
}

/// Global theme configuration
#[derive(Clone, Debug, Default)]
pub struct Theme {
    pub colors: ColorPalette,
    pub spacing: Spacing,
    pub typography: Typography,
}

impl Theme {
    /// Create a new theme with custom configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set custom colors
    pub fn with_colors(mut self, colors: ColorPalette) -> Self {
        self.colors = colors;
        self
    }

    /// Set custom spacing
    pub fn with_spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = spacing;
        self
    }

    /// Set custom typography
    pub fn with_typography(mut self, typography: Typography) -> Self {
        self.typography = typography;
        self
    }

    /// Generate CSS custom properties (CSS variables) from theme
    pub fn to_css(&self) -> String {
        format!(
            r#":root {{
    /* Colors */
    --color-primary: {};
    --color-secondary: {};
    --color-background: {};
    --color-text: {};
    --color-accent: {};
    --color-error: {};
    --color-warning: {};
    --color-success: {};

    /* Spacing */
    --space-xs: {}px;
    --space-sm: {}px;
    --space-md: {}px;
    --space-lg: {}px;
    --space-xl: {}px;
    --space-xxl: {}px;

    /* Typography */
    --font-family: {};
    --font-size-base: {}px;
    --line-height: {};
    --font-heading: {};
}}

body {{
    font-family: var(--font-family);
    font-size: var(--font-size-base);
    line-height: var(--line-height);
    color: var(--color-text);
    background: var(--color-background);
    margin: 0;
    padding: 0;
}}
"#,
            self.colors.primary,
            self.colors.secondary,
            self.colors.background,
            self.colors.text,
            self.colors.accent,
            self.colors.error,
            self.colors.warning,
            self.colors.success,
            self.spacing.xs,
            self.spacing.sm,
            self.spacing.md,
            self.spacing.lg,
            self.spacing.xl,
            self.spacing.xxl,
            self.typography.font_family,
            self.typography.base_size,
            self.typography.line_height,
            self.typography.heading_font,
        )
    }
}

/// Theme configuration builder
pub struct ThemeConfig;

impl ThemeConfig {
    /// Create default theme
    pub fn default_theme() -> Theme {
        Theme::default()
    }

    /// Create purple gradient theme (like reactive_counter)
    pub fn purple_gradient() -> Theme {
        Theme::new().with_colors(ColorPalette {
            primary: "#667eea".to_string(),
            secondary: "#764ba2".to_string(),
            background: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)".to_string(),
            text: "#ffffff".to_string(),
            ..Default::default()
        })
    }

    /// Create dark theme
    pub fn dark() -> Theme {
        Theme::new().with_colors(ColorPalette {
            primary: "#3b82f6".to_string(),
            secondary: "#8b5cf6".to_string(),
            background: "#1f2937".to_string(),
            text: "#f3f4f6".to_string(),
            ..Default::default()
        })
    }
}
