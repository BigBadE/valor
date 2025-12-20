//! Tailwind-inspired utility class generator

use super::theme::ColorPalette;

pub struct TailwindUtilities;

impl TailwindUtilities {
    /// Generate Tailwind-inspired utility classes
    pub fn generate(colors: &ColorPalette) -> String {
        let mut css = String::new();

        // Flexbox utilities
        css.push_str(Self::flexbox());

        // Spacing utilities (padding, margin)
        css.push_str(&Self::spacing());

        // Typography utilities
        css.push_str(&Self::typography());

        // Color utilities
        css.push_str(&Self::colors(colors));

        // Layout utilities
        css.push_str(&Self::layout());

        // Border utilities
        css.push_str(&Self::borders());

        // Shadow utilities
        css.push_str(&Self::shadows());

        // Effects utilities
        css.push_str(&Self::effects());

        css
    }

    fn flexbox() -> &'static str {
        r#"
/* Flexbox */
.flex { display: flex; }
.inline-flex { display: inline-flex; }
.flex-row { flex-direction: row; }
.flex-col { flex-direction: column; }
.flex-wrap { flex-wrap: wrap; }
.flex-nowrap { flex-wrap: nowrap; }
.items-start { align-items: flex-start; }
.items-center { align-items: center; }
.items-end { align-items: flex-end; }
.items-stretch { align-items: stretch; }
.justify-start { justify-content: flex-start; }
.justify-center { justify-content: center; }
.justify-end { justify-content: flex-end; }
.justify-between { justify-content: space-between; }
.justify-around { justify-content: space-around; }
.gap-1 { gap: 4px; }
.gap-2 { gap: 8px; }
.gap-3 { gap: 12px; }
.gap-4 { gap: 16px; }
.gap-5 { gap: 20px; }
.gap-6 { gap: 24px; }
.gap-8 { gap: 32px; }
.gap-10 { gap: 40px; }
"#
    }

    fn spacing() -> String {
        let mut css = String::new();

        // Padding
        for (name, val) in [
            ("0", "0"),
            ("1", "4px"),
            ("2", "8px"),
            ("3", "12px"),
            ("4", "16px"),
            ("5", "20px"),
            ("6", "24px"),
            ("8", "32px"),
            ("10", "40px"),
            ("12", "48px"),
        ] {
            css.push_str(&format!(".p-{} {{ padding: {}; }}\n", name, val));
            css.push_str(&format!(
                ".px-{} {{ padding-left: {}; padding-right: {}; }}\n",
                name, val, val
            ));
            css.push_str(&format!(
                ".py-{} {{ padding-top: {}; padding-bottom: {}; }}\n",
                name, val, val
            ));
            css.push_str(&format!(".pt-{} {{ padding-top: {}; }}\n", name, val));
            css.push_str(&format!(".pr-{} {{ padding-right: {}; }}\n", name, val));
            css.push_str(&format!(".pb-{} {{ padding-bottom: {}; }}\n", name, val));
            css.push_str(&format!(".pl-{} {{ padding-left: {}; }}\n", name, val));
        }

        // Margin
        for (name, val) in [
            ("0", "0"),
            ("1", "4px"),
            ("2", "8px"),
            ("3", "12px"),
            ("4", "16px"),
            ("5", "20px"),
            ("6", "24px"),
            ("8", "32px"),
            ("10", "40px"),
            ("12", "48px"),
        ] {
            css.push_str(&format!(".m-{} {{ margin: {}; }}\n", name, val));
            css.push_str(&format!(
                ".mx-{} {{ margin-left: {}; margin-right: {}; }}\n",
                name, val, val
            ));
            css.push_str(&format!(
                ".my-{} {{ margin-top: {}; margin-bottom: {}; }}\n",
                name, val, val
            ));
            css.push_str(&format!(".mt-{} {{ margin-top: {}; }}\n", name, val));
            css.push_str(&format!(".mr-{} {{ margin-right: {}; }}\n", name, val));
            css.push_str(&format!(".mb-{} {{ margin-bottom: {}; }}\n", name, val));
            css.push_str(&format!(".ml-{} {{ margin-left: {}; }}\n", name, val));
        }

        css
    }

    fn typography() -> &'static str {
        r#"
/* Typography */
.text-xs { font-size: 12px; line-height: 16px; }
.text-sm { font-size: 14px; line-height: 20px; }
.text-base { font-size: 16px; line-height: 24px; }
.text-lg { font-size: 18px; line-height: 28px; }
.text-xl { font-size: 20px; line-height: 28px; }
.text-2xl { font-size: 24px; line-height: 32px; }
.text-3xl { font-size: 30px; line-height: 36px; }
.text-4xl { font-size: 36px; line-height: 40px; }
.text-5xl { font-size: 48px; line-height: 1; }
.text-6xl { font-size: 60px; line-height: 1; }
.text-7xl { font-size: 72px; line-height: 1; }
.font-thin { font-weight: 100; }
.font-light { font-weight: 300; }
.font-normal { font-weight: 400; }
.font-medium { font-weight: 500; }
.font-semibold { font-weight: 600; }
.font-bold { font-weight: 700; }
.font-black { font-weight: 900; }
.text-left { text-align: left; }
.text-center { text-align: center; }
.text-right { text-align: right; }
.text-justify { text-align: justify; }
"#
    }

    fn colors(palette: &ColorPalette) -> String {
        format!(
            r#"
/* Colors */
.text-primary {{ color: {}; }}
.text-secondary {{ color: {}; }}
.text-white {{ color: #ffffff; }}
.text-black {{ color: #000000; }}
.bg-primary {{ background-color: {}; }}
.bg-secondary {{ background-color: {}; }}
.bg-white {{ background-color: #ffffff; }}
.bg-black {{ background-color: #000000; }}
.bg-transparent {{ background-color: transparent; }}
.bg-gradient {{ background: {}; }}
"#,
            palette.text, palette.secondary, palette.primary, palette.secondary, palette.background
        )
    }

    fn layout() -> &'static str {
        r#"
/* Layout */
.block { display: block; }
.inline-block { display: inline-block; }
.inline { display: inline; }
.hidden { display: none; }
.w-full { width: 100%; }
.h-full { height: 100%; }
.min-h-screen { min-height: 100vh; }
.max-w-xs { max-width: 320px; }
.max-w-sm { max-width: 384px; }
.max-w-md { max-width: 448px; }
.max-w-lg { max-width: 512px; }
.max-w-xl { max-width: 576px; }
.max-w-2xl { max-width: 672px; }
"#
    }

    fn borders() -> &'static str {
        r#"
/* Borders */
.border { border-width: 1px; border-style: solid; }
.border-0 { border-width: 0; }
.border-2 { border-width: 2px; }
.border-4 { border-width: 4px; }
.rounded-none { border-radius: 0; }
.rounded-sm { border-radius: 2px; }
.rounded { border-radius: 4px; }
.rounded-md { border-radius: 6px; }
.rounded-lg { border-radius: 8px; }
.rounded-xl { border-radius: 12px; }
.rounded-2xl { border-radius: 16px; }
.rounded-full { border-radius: 9999px; }
"#
    }

    fn shadows() -> &'static str {
        r#"
/* Shadows */
.shadow-none { box-shadow: none; }
.shadow-sm { box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05); }
.shadow { box-shadow: 0 1px 3px 0 rgba(0, 0, 0, 0.1), 0 1px 2px 0 rgba(0, 0, 0, 0.06); }
.shadow-md { box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06); }
.shadow-lg { box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -2px rgba(0, 0, 0, 0.05); }
.shadow-xl { box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.1), 0 10px 10px -5px rgba(0, 0, 0, 0.04); }
.text-shadow { text-shadow: 2px 2px 4px rgba(0,0,0,0.3); }
"#
    }

    fn effects() -> &'static str {
        r#"
/* Effects */
.opacity-0 { opacity: 0; }
.opacity-50 { opacity: 0.5; }
.opacity-75 { opacity: 0.75; }
.opacity-90 { opacity: 0.9; }
.opacity-100 { opacity: 1; }
.cursor-pointer { cursor: pointer; }
.cursor-default { cursor: default; }
.transition { transition: all 0.15s ease-in-out; }
.transition-transform { transition: transform 0.15s ease-in-out; }
.transition-colors { transition: color 0.15s, background-color 0.15s; }

/* Hover effects */
.hover\:shadow-lg:hover { box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -2px rgba(0, 0, 0, 0.05); }
.hover\:transform-up:hover { transform: translateY(-2px); }
.hover\:bg-gray:hover { background-color: #f0f0f0; }
"#
    }
}
