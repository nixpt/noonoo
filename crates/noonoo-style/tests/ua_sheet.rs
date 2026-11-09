//! The user-agent sheet's defaults, after HTML's rendering section: block
//! display, the vertical rhythm of margins, list indents and preformatted
//! text, before any author CSS.

use noonoo_css::Stylesheet;
use noonoo_dom::Document;
use noonoo_style::{
    ComputedStyle, Dimension, Display, FontStyle, Styles, TextAlign, WhiteSpace, compute,
};

/// A standards-mode page: quirks mode drops some default margins.
fn styled(html: &str) -> (Document, Styles) {
    let doc = noonoo_html::parse(&format!("<!doctype html>{html}"));
    let styles = compute(&doc, &[]);
    (doc, styles)
}

fn first<'s>(doc: &Document, styles: &'s Styles, tag: &str) -> &'s ComputedStyle {
    styles.get(doc.get_elements_by_tag_name(tag)[0]).unwrap()
}

/// `(top, right, bottom, left)` margins in px; `auto` as `NaN`.
fn margins(style: &ComputedStyle) -> (f32, f32, f32, f32) {
    let px = |d: &Dimension| match d {
        Dimension::Px(px) => *px,
        _ => f32::NAN,
    };
    (
        px(&style.margin.top),
        px(&style.margin.right),
        px(&style.margin.bottom),
        px(&style.margin.left),
    )
}

#[test]
fn paragraphs_and_blocks_get_one_line_of_margin() {
    let (doc, styles) = styled(
        "<p>a</p><blockquote>b</blockquote><figure>c</figure><dl><dt>d<dd>e</dl><pre>f</pre>",
    );
    assert_eq!(margins(first(&doc, &styles, "p")), (16.0, 0.0, 16.0, 0.0));
    assert_eq!(
        margins(first(&doc, &styles, "blockquote")),
        (16.0, 40.0, 16.0, 40.0)
    );
    assert_eq!(
        margins(first(&doc, &styles, "figure")),
        (16.0, 40.0, 16.0, 40.0)
    );
    assert_eq!(margins(first(&doc, &styles, "dl")), (16.0, 0.0, 16.0, 0.0));
    assert_eq!(margins(first(&doc, &styles, "dd")), (0.0, 0.0, 0.0, 40.0));
    assert_eq!(margins(first(&doc, &styles, "pre")), (16.0, 0.0, 16.0, 0.0));
}

#[test]
fn heading_margins_scale_with_their_size() {
    let (doc, styles) = styled("<h1>a</h1><h2>b</h2><h3>c</h3><h4>d</h4><h5>e</h5><h6>f</h6>");
    // `0.67em` of 32px, `0.83em` of 24px, ... as in browsers.
    let expected = [
        ("h1", 0.67 * 32.0),
        ("h2", 0.83 * 24.0),
        ("h3", 1.0 * 1.17 * 16.0),
        ("h4", 1.33 * 16.0),
        ("h5", 1.67 * 0.83 * 16.0),
        ("h6", 2.33 * 0.67 * 16.0),
    ];
    for (tag, margin) in expected {
        let (top, _, bottom, _) = margins(first(&doc, &styles, tag));
        assert!((top - margin).abs() < 0.01, "{tag}: {top} vs {margin}");
        assert_eq!(top, bottom, "{tag}");
    }
}

#[test]
fn lists_indent_and_nested_lists_lose_their_vertical_margin() {
    let (doc, styles) = styled("<ul><li>a<ol><li>b</ol></ul><menu><li>c</menu>");
    let ul = first(&doc, &styles, "ul");
    assert_eq!(margins(ul), (16.0, 0.0, 16.0, 0.0));
    assert_eq!(ul.padding.left, noonoo_style::Length::Px(40.0));
    let ol = first(&doc, &styles, "ol");
    assert_eq!(margins(ol), (0.0, 0.0, 0.0, 0.0));
    assert_eq!(ol.padding.left, noonoo_style::Length::Px(40.0));
    assert_eq!(first(&doc, &styles, "menu").display, Display::Block);
}

#[test]
fn the_rest_of_htmls_block_elements_are_blocks() {
    let (doc, styles) = styled(
        "<center>a</center><details><summary>s</summary>b</details><address>c</address>\
         <hgroup>d</hgroup><legend>e</legend><search>f</search><dir>g</dir><optgroup>h</optgroup>",
    );
    for tag in [
        "center", "details", "summary", "address", "hgroup", "legend", "search", "dir",
    ] {
        assert_eq!(first(&doc, &styles, tag).display, Display::Block, "{tag}");
    }
    // `center` centres its blocks too, as `-webkit-center` does.
    assert_eq!(
        first(&doc, &styles, "center").text_align,
        TextAlign::WebkitCenter
    );
    assert_eq!(
        first(&doc, &styles, "address").font_style,
        FontStyle::Italic
    );
}

#[test]
fn preformatted_text_keeps_its_white_space() {
    let (doc, styles) = styled("<pre>a</pre><textarea>b</textarea><p>c</p>");
    assert_eq!(first(&doc, &styles, "pre").white_space, WhiteSpace::Pre);
    assert_eq!(
        first(&doc, &styles, "textarea").white_space,
        WhiteSpace::PreWrap
    );
    assert_eq!(first(&doc, &styles, "p").white_space, WhiteSpace::Normal);
}

#[test]
fn table_headers_are_bold_and_centred() {
    let (doc, styles) = styled("<table><tr><th>h</th><td>d</td></tr></table>");
    let th = first(&doc, &styles, "th");
    assert_eq!(th.font_weight, 700);
    assert_eq!(th.text_align, TextAlign::Center);
    assert_eq!(first(&doc, &styles, "td").font_weight, 400);
}

#[test]
fn the_hidden_attribute_hides() {
    let (doc, styles) = styled("<div hidden>a</div><p>b</p>");
    assert_eq!(first(&doc, &styles, "div").display, Display::None);
}

#[test]
fn author_css_still_wins() {
    let doc = noonoo_html::parse("<p>a</p><ul><li>b</ul>");
    let sheet = Stylesheet::parse("p, ul { margin: 0; padding: 0 }").unwrap();
    let styles = compute(&doc, &[sheet]);
    assert_eq!(margins(first(&doc, &styles, "p")), (0.0, 0.0, 0.0, 0.0));
    assert_eq!(
        first(&doc, &styles, "ul").padding.left,
        noonoo_style::Length::Px(0.0)
    );
}

#[test]
fn the_webkit_centre_keywords_parse() {
    let doc = noonoo_html::parse("<div id=a>a</div><div id=b>b</div>");
    let sheet =
        Stylesheet::parse("#a { text-align: -webkit-center } #b { text-align: -moz-center }")
            .unwrap();
    let styles = compute(&doc, &[sheet]);
    for id in ["a", "b"] {
        let node = doc.get_element_by_id(id).unwrap();
        assert_eq!(
            styles.get(node).unwrap().text_align,
            TextAlign::WebkitCenter,
            "{id}"
        );
    }
}

#[test]
fn text_fields_have_an_inset_border_and_padding() {
    let (doc, styles) = styled(
        r#"<input id="a"><input id="b" type="search"><textarea id="t"></textarea><input id="c" type="checkbox">"#,
    );
    let by_id = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    // A textarea's box is Chromium's: 2px of padding in a 1px border.
    let textarea = by_id("t");
    assert_eq!(textarea.padding.top, noonoo_style::Length::Px(2.0));
    assert_eq!(textarea.padding.left, noonoo_style::Length::Px(2.0));
    assert_eq!(textarea.border_width.top, 1.0);
    assert_eq!(textarea.border_style.top, noonoo_style::BorderStyle::Solid);
    assert_eq!(textarea.font_family, ["monospace"]);
    // ... and it scrolls its text rather than spilling it.
    assert_eq!(textarea.overflow_y, noonoo_style::Overflow::Auto);
    for id in ["a", "b"] {
        let style = by_id(id);
        assert_eq!(style.padding.top, noonoo_style::Length::Px(1.0), "{id}");
        assert_eq!(style.padding.left, noonoo_style::Length::Px(2.0), "{id}");
        assert_eq!(style.border_width.top, 2.0, "{id}");
        assert_eq!(
            style.border_style.top,
            noonoo_style::BorderStyle::Inset,
            "{id}"
        );
    }
    assert_eq!(by_id("c").border_width.top, 0.0);
}

#[test]
fn form_controls_use_the_small_control_font() {
    let doc = noonoo_html::parse(
        "<!doctype html><div><input id=i><textarea id=t></textarea><select id=s></select><button id=b>x</button></div>",
    );
    let sheet = Stylesheet::parse(
        "div { font-size: 30px; font-weight: bold; font-style: italic; line-height: 3 }",
    )
    .unwrap();
    let styles = compute(&doc, &[sheet]);
    for id in ["i", "t", "s", "b"] {
        let style = styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
        // `font: -webkit-small-control`: 13.33px, and the shorthand resets
        // what the page set on the control's ancestors.
        assert!((style.font_size - 40.0 / 3.0).abs() < 0.01, "{id}");
        assert_eq!(style.font_weight, 400, "{id}");
        assert_eq!(style.font_style, FontStyle::Normal, "{id}");
        assert_eq!(style.line_height, None, "{id}");
    }
}

#[test]
fn buttons_checkboxes_and_radios_have_browser_boxes() {
    let (doc, styles) = styled(
        r#"<button id=b>x</button><input id=s type=submit><input id=c type=checkbox><input id=r type=RADIO>"#,
    );
    let by_id = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    for id in ["b", "s"] {
        let style = by_id(id);
        assert_eq!(style.padding.top, noonoo_style::Length::Px(1.0), "{id}");
        assert_eq!(style.padding.left, noonoo_style::Length::Px(6.0), "{id}");
        assert_eq!(style.border_width.left, 2.0, "{id}");
        assert_eq!(
            style.border_style.left,
            noonoo_style::BorderStyle::Outset,
            "{id}"
        );
    }
    let px = noonoo_style::Dimension::Px;
    for (id, expected) in [("c", (3.0, 3.0, 3.0, 4.0)), ("r", (3.0, 3.0, 0.0, 5.0))] {
        let style = by_id(id);
        assert_eq!(
            (style.width.clone(), style.height.clone()),
            (px(13.0), px(13.0)),
            "{id}"
        );
        assert_eq!(margins(style), expected, "{id}");
    }
}

#[test]
fn hidden_inputs_are_not_displayed() {
    let doc = noonoo_html::parse(r#"<!doctype html><input id=h type=HIDDEN>"#);
    // As in browsers, `!important`: a page's `input { display: block }`
    // doesn't show it.
    let sheet = Stylesheet::parse("input { display: block }").unwrap();
    let styles = compute(&doc, &[sheet]);
    let style = styles.get(doc.get_element_by_id("h").unwrap()).unwrap();
    assert_eq!(style.display, Display::None);
}

#[test]
fn a_rule_is_a_grey_inset_line() {
    let (doc, styles) = styled("<p>a</p><hr><p>b</p>");
    let hr = first(&doc, &styles, "hr");
    // Chromium's: a 1px inset border all round, half a line of margin.
    assert_eq!(hr.border_width.top, 1.0);
    assert_eq!(hr.border_width.bottom, 1.0);
    assert_eq!(hr.border_style.top, noonoo_style::BorderStyle::Inset);
    let (top, _, bottom, _) = margins(hr);
    assert_eq!((top, bottom), (8.0, 8.0));
    assert!(hr.margin.left == Dimension::Auto && hr.margin.right == Dimension::Auto);
    assert_eq!(hr.overflow_y, noonoo_style::Overflow::Hidden);
}
