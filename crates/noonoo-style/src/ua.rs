//! The user-agent stylesheet.
//!
//! A default stylesheet so that a bare document has sensible structure
//! before any author CSS, after the HTML standard's rendering section: block
//! vs inline display, `head` and friends (and `[hidden]`) hidden, heading
//! sizes, the `1em` margins of paragraphs, lists and quotes, list indents,
//! and preformatted white space. It is applied at the lowest origin, so any
//! author rule overrides it.

/// The built-in user-agent stylesheet.
pub const UA_STYLESHEET: &str = r#"
html, body, div, p, section, article, header, footer, main, nav, aside,
figure, figcaption, blockquote, pre, form, fieldset, ul, ol, dl, dt, dd,
h1, h2, h3, h4, h5, h6, hr, address, center, details, summary,
hgroup, legend, listing, menu, dir, plaintext, search, xmp, optgroup { display: block }
table { display: table }
thead, tbody, tfoot { display: table-row-group }
tr { display: table-row }
td, th { display: table-cell }
li { display: list-item }
head, title, meta, link, style, script, base, template, dialog { display: none }
dialog[open] { display: block }
span, a, em, strong, b, i, u, small, code, kbd, samp, var, sub, sup, label,
abbr, cite, q, mark, dfn, s, del, ins, time, bdi, bdo, data, ruby { display: inline }
img, input, button, select, textarea, video, canvas, progress { display: inline-block }
body { margin: 8px }
p, blockquote, figure, dl, pre, listing, plaintext, xmp, ul, ol, menu, dir {
  margin-top: 1em; margin-bottom: 1em }
blockquote, figure { margin-left: 40px; margin-right: 40px }
dd { margin-left: 40px }
ul, ol, menu, dir { padding-left: 40px }
ul ul, ul ol, ul menu, ul dir, ol ul, ol ol, ol menu, ol dir,
menu ul, menu ol, menu menu, menu dir, dir ul, dir ol, dir menu, dir dir {
  margin-top: 0; margin-bottom: 0 }
h1 { font-size: 2em; margin-top: 0.67em; margin-bottom: 0.67em; font-weight: bold }
h2 { font-size: 1.5em; margin-top: 0.83em; margin-bottom: 0.83em; font-weight: bold }
h3 { font-size: 1.17em; margin-top: 1em; margin-bottom: 1em; font-weight: bold }
h4 { font-size: 1em; margin-top: 1.33em; margin-bottom: 1.33em; font-weight: bold }
h5 { font-size: 0.83em; margin-top: 1.67em; margin-bottom: 1.67em; font-weight: bold }
h6 { font-size: 0.67em; margin-top: 2.33em; margin-bottom: 2.33em; font-weight: bold }
pre, listing, plaintext, xmp { white-space: pre }
textarea { white-space: pre-wrap }
center, th { text-align: center }
th { font-weight: bold }
strong, b { font-weight: bold }
em, i, cite, var, dfn, address { font-style: italic }
pre, code, kbd, samp, listing, plaintext, xmp { font-family: monospace }
a { color: #0000ee; text-decoration: underline }
[hidden] { display: none }
"#;
