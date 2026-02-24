//! Default User-Agent stylesheet per the HTML specification.
//!
//! These rules are loaded before any author CSS so they have the lowest
//! source-order priority in the cascade.

/// UA stylesheet CSS text. Uses type selectors (specificity 0,0,1) so any
/// author rule with equal or higher specificity will override these defaults.
pub const UA_CSS: &str = "\
html, body, div, p, h1, h2, h3, h4, h5, h6, \
ul, ol, li, dl, dt, dd, blockquote, pre, form, fieldset, legend, \
section, article, aside, header, footer, main, nav, address, \
figure, figcaption, details, summary, dialog, hr, button, textarea { \
    display: block; \
} \
head, meta, title, link, style, script, base, template, noscript { \
    display: none; \
} \
body { \
    margin: 8px; \
} \
h1 { font-weight: 700; font-size: 2em; margin-top: 0.67em; margin-bottom: 0.67em; } \
h2 { font-weight: 700; font-size: 1.5em; margin-top: 0.83em; margin-bottom: 0.83em; } \
h3 { font-weight: 700; font-size: 1.17em; margin-top: 1em; margin-bottom: 1em; } \
h4 { font-weight: 700; font-size: 1em; margin-top: 1.33em; margin-bottom: 1.33em; } \
h5 { font-weight: 700; font-size: 0.83em; margin-top: 1.67em; margin-bottom: 1.67em; } \
h6 { font-weight: 700; font-size: 0.67em; margin-top: 2.33em; margin-bottom: 2.33em; } \
p { margin-top: 1em; margin-bottom: 1em; } \
table { display: table; } \
thead { display: table-header-group; } \
tbody { display: table-row-group; } \
tfoot { display: table-footer-group; } \
tr { display: table-row; } \
th, td { display: table-cell; } \
span, a, em, strong, b, i, u, s, small, big, sub, sup, \
abbr, cite, code, kbd, samp, var, q, mark, label { \
    display: inline; \
} \
img, br, input, select { display: inline; } \
";
