//! The user-agent stylesheet.
//!
//! A small default stylesheet so that a bare document has sensible structure
//! before any author CSS: block vs inline display, `head` and friends hidden,
//! and default heading sizes. It is applied at the lowest origin, so any author
//! rule overrides it.

/// The built-in user-agent stylesheet.
pub const UA_STYLESHEET: &str = r#"
html, body, div, p, section, article, header, footer, main, nav, aside,
figure, figcaption, blockquote, pre, form, fieldset, ul, ol, dl, dt, dd,
h1, h2, h3, h4, h5, h6, hr, table, thead, tbody, tfoot, tr { display: block }
li { display: list-item }
head, title, meta, link, style, script, base, template { display: none }
span, a, em, strong, b, i, u, small, code, kbd, samp, var, sub, sup, label,
abbr, cite, q, mark { display: inline }
img, input, button, select, textarea, video, canvas, progress { display: inline-block }
body { margin: 8px }
h1 { font-size: 2em; font-weight: bold }
h2 { font-size: 1.5em; font-weight: bold }
h3 { font-size: 1.17em; font-weight: bold }
h4 { font-size: 1em; font-weight: bold }
h5 { font-size: 0.83em; font-weight: bold }
h6 { font-size: 0.67em; font-weight: bold }
strong, b { font-weight: bold }
em, i { font-style: italic }
pre, code { font-family: monospace }
a { color: #0000ee; text-decoration: underline }
"#;
