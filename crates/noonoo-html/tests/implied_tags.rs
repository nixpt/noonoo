//! HTML5's implied end tags and implied table sections in the tree builder.

use noonoo_dom::{Document, NodeData, NodeId};

/// The element tree under `<body>`-level content as compact markup:
/// `p(#a) div(#b)`, children in brackets, text as `"..."`.
fn shape(html: &str) -> String {
    let doc = noonoo_html::parse(html);
    let root = doc.root();
    children(&doc, root)
}

fn children(doc: &Document, node: NodeId) -> String {
    doc.tree()
        .children(node)
        .iter()
        .filter_map(|&child| match doc.node(child)? {
            NodeData::Element(element) => {
                let inner = children(doc, child);
                Some(if inner.is_empty() {
                    element.name.clone()
                } else {
                    format!("{}[{inner}]", element.name)
                })
            }
            NodeData::Text(text) if !text.trim().is_empty() => Some(format!("{:?}", text.trim())),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_block_start_tag_closes_an_open_paragraph() {
    assert_eq!(shape("<p>a<p>b"), r#"p["a"] p["b"]"#);
    assert_eq!(shape("<p>a<div>b</div>c"), r#"p["a"] div["b"] "c""#);
    assert_eq!(shape("<p>a<ul><li>b</ul>"), r#"p["a"] ul[li["b"]]"#);
    assert_eq!(
        shape("<p>a<table><tr><td>b</table>"),
        r#"p["a"] table[tbody[tr[td["b"]]]]"#
    );
    assert_eq!(shape("<p>a<h2>b</h2>"), r#"p["a"] h2["b"]"#);
    // Phrasing content stays inside.
    assert_eq!(
        shape("<p>a<b>b</b><span>c</span>"),
        r#"p["a" b["b"] span["c"]]"#
    );
}

#[test]
fn a_paragraph_outside_a_buttons_scope_stays_open() {
    assert_eq!(
        shape("<p>a<button><div>b</div></button>"),
        r#"p["a" button[div["b"]]]"#
    );
    assert_eq!(
        shape("<button><p>a<div>b</div></button>"),
        r#"button[p["a"] div["b"]]"#
    );
    assert_eq!(
        shape("<p>a<table><tr><td><div>b</div></table>"),
        r#"p["a"] table[tbody[tr[td[div["b"]]]]]"#
    );
}

#[test]
fn a_stray_paragraph_end_tag_makes_an_empty_paragraph() {
    assert_eq!(shape("<div>a</p>b</div>"), r#"div["a" p "b"]"#);
}

#[test]
fn list_items_and_definitions_close_their_open_sibling() {
    assert_eq!(shape("<ul><li>a<li>b</ul>"), r#"ul[li["a"] li["b"]]"#);
    assert_eq!(
        shape("<ul><li>a<ul><li>b</ul><li>c</ul>"),
        r#"ul[li["a" ul[li["b"]]] li["c"]]"#
    );
    assert_eq!(
        shape("<ul><li>a<div><li>b</div></ul>"),
        r#"ul[li["a" div] li["b"]]"#,
        "a div doesn't stop the search"
    );
    assert_eq!(
        shape("<dl><dt>a<dd>b<dt>c<dd>d</dl>"),
        r#"dl[dt["a"] dd["b"] dt["c"] dd["d"]]"#
    );
    assert_eq!(shape("<ul><li><p>a<li>b</ul>"), r#"ul[li[p["a"]] li["b"]]"#);
}

#[test]
fn options_and_headings_close_their_open_sibling() {
    assert_eq!(
        shape("<select><option>a<option>b<optgroup><option>c<optgroup><option>d</select>"),
        r#"select[option["a"] option["b"] optgroup[option["c"]] optgroup[option["d"]]]"#
    );
    assert_eq!(shape("<h1>a<h2>b</h2>"), r#"h1["a"] h2["b"]"#);
    // Inside a select, a block doesn't reach out to close the paragraph.
    assert_eq!(
        shape("<p>a<select><option>b<div>c</div></select>"),
        r#"p["a" select[option["b" div["c"]]]]"#
    );
}

#[test]
fn rows_and_cells_get_their_implied_sections() {
    assert_eq!(
        shape("<table><tr><td>a</td></tr></table>"),
        "table[tbody[tr[td[\"a\"]]]]"
    );
    assert_eq!(
        shape("<table><td>a<td>b</table>"),
        r#"table[tbody[tr[td["a"] td["b"]]]]"#
    );
    assert_eq!(
        shape("<table><thead><tr><th>h<tbody><td>a</table>"),
        r#"table[thead[tr[th["h"]]] tbody[tr[td["a"]]]]"#
    );
    assert_eq!(
        shape("<table><caption>c</caption><tr><td>a</table>"),
        r#"table[caption["c"] tbody[tr[td["a"]]]]"#
    );
}
