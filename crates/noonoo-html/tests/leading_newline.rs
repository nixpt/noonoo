//! HTML's tree builder drops a newline straight after a `<pre>`,
//! `<listing>` or `<textarea>` start tag, so markup can start the text on
//! the next line.

use noonoo_dom::NodeData;

fn text_of(html: &str, tag: &str) -> String {
    let doc = noonoo_html::parse(html);
    let element = doc.get_elements_by_tag_name(tag)[0];
    doc.tree()
        .children(element)
        .iter()
        .filter_map(|&child| match doc.node(child)? {
            NodeData::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_newline_after_pre_listing_and_textarea_is_dropped() {
    assert_eq!(text_of("<pre>\nx\n</pre>", "pre"), "x\n");
    assert_eq!(text_of("<pre>\r\nx</pre>", "pre"), "x");
    assert_eq!(text_of("<listing>\nx</listing>", "listing"), "x");
    assert_eq!(text_of("<textarea>\nx</textarea>", "textarea"), "x");
}

#[test]
fn only_one_newline_is_dropped() {
    assert_eq!(text_of("<pre>\n\nx</pre>", "pre"), "\nx");
}

#[test]
fn a_newline_after_other_content_stays() {
    assert_eq!(text_of("<pre><b>a</b>\nx</pre>", "pre"), "\nx");
    assert_eq!(text_of("<div>\nx</div>", "div"), "\nx");
    assert_eq!(text_of("<pre> \nx</pre>", "pre"), " \nx");
}
