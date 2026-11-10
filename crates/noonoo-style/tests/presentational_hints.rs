//! HTML's presentational attributes (the rendering section's "presentational
//! hints"): they style like author CSS that every author rule overrides.

use noonoo_css::Stylesheet;
use noonoo_style::{Dimension, compute};

fn width_of(html: &str, css: &str, id: &str) -> Dimension {
    let doc = noonoo_html::parse(html);
    let sheets: Vec<Stylesheet> = if css.is_empty() {
        Vec::new()
    } else {
        vec![Stylesheet::parse(css).unwrap()]
    };
    let styles = compute(&doc, &sheets);
    styles
        .get(doc.get_element_by_id(id).unwrap())
        .unwrap()
        .width
        .clone()
}

#[test]
fn a_width_attribute_sets_the_width_of_tables_and_cells() {
    let html = r#"<table id="t" width="85%"><tr><td id="d" width="40">a</td><th id="h" width=" 12.5 ">b</th></tr></table>"#;
    assert_eq!(width_of(html, "", "t"), Dimension::Percent(85.0));
    assert_eq!(width_of(html, "", "d"), Dimension::Px(40.0));
    assert_eq!(width_of(html, "", "h"), Dimension::Px(12.5));
}

#[test]
fn author_css_overrides_a_width_attribute() {
    let html = r#"<table id="t" width="85%"><tr><td>a</td></tr></table>"#;
    assert_eq!(
        width_of(html, "* { width: 10px }", "t"),
        Dimension::Px(10.0)
    );
}

#[test]
fn an_invalid_or_foreign_width_attribute_is_ignored() {
    let html = r#"<table id="t" width="wide"><tr><td>a</td></tr></table><div id="d" width="40"></div><table id="z" width="0"><tr><td>a</td></tr></table>"#;
    assert_eq!(width_of(html, "", "t"), Dimension::Auto);
    assert_eq!(width_of(html, "", "d"), Dimension::Auto);
    // HTML: a zero width is no hint.
    assert_eq!(width_of(html, "", "z"), Dimension::Auto);
}

fn padding_of(html: &str, css: &str, id: &str) -> noonoo_style::Length {
    let doc = noonoo_html::parse(html);
    let sheets: Vec<Stylesheet> = if css.is_empty() {
        Vec::new()
    } else {
        vec![Stylesheet::parse(css).unwrap()]
    };
    let styles = compute(&doc, &sheets);
    let style = styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    assert_eq!(style.padding.top, style.padding.left, "{id}");
    style.padding.right.clone()
}

#[test]
fn cells_have_a_pixel_of_padding_by_default() {
    let html = r#"<table><tr><td id="d">a</td><th id="h">b</th></tr></table>"#;
    assert_eq!(padding_of(html, "", "d"), noonoo_style::Length::Px(1.0));
    assert_eq!(padding_of(html, "", "h"), noonoo_style::Length::Px(1.0));
}

#[test]
fn cellpadding_pads_its_own_tables_cells() {
    let html = r#"<table cellpadding="4"><tbody><tr><td id="d">a<table><tr><td id="inner">b</td></tr></table></td></tr></tbody></table><table cellpadding="0"><tr><th id="z">c</th></tr></table>"#;
    assert_eq!(padding_of(html, "", "d"), noonoo_style::Length::Px(4.0));
    assert_eq!(padding_of(html, "", "inner"), noonoo_style::Length::Px(1.0));
    assert_eq!(padding_of(html, "", "z"), noonoo_style::Length::Px(0.0));
}

#[test]
fn author_css_overrides_cellpadding() {
    let html = r#"<table cellpadding="4"><tr><td id="d">a</td></tr></table>"#;
    assert_eq!(
        padding_of(html, "td { padding: 2px }", "d"),
        noonoo_style::Length::Px(2.0)
    );
}

#[test]
fn tables_space_their_cells_and_cellspacing_sets_it() {
    let doc = noonoo_html::parse(
        "<table id=a><tr><td>x</table><table id=b cellspacing=5><tr><td>x</table>",
    );
    let styles = noonoo_style::compute(&doc, &[]);
    let of = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    assert_eq!(of("a").border_spacing, (2.0, 2.0));
    assert_eq!(of("b").border_spacing, (5.0, 5.0));
}

#[test]
fn a_border_attribute_frames_the_table_and_its_cells() {
    use noonoo_style::BorderStyle;
    let doc = noonoo_html::parse(
        "<table id=a border=2><tr><td id=ad>x</table>\
         <table id=b border><tr><th id=bd>x</table>\
         <table id=c border=0><tr><td id=cd>x</table>\
         <table id=d border=3><tr><td id=dd>x</table>",
    );
    let sheet = Stylesheet::parse("#dd { border: none }").unwrap();
    let styles = compute(&doc, &[sheet]);
    let of = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    for (table, width) in [("a", 2.0), ("b", 1.0)] {
        let style = of(table);
        assert_eq!(style.border_width.left, width, "{table}");
        assert_eq!(style.border_width.bottom, width, "{table}");
        assert_eq!(style.border_style.top, BorderStyle::Outset, "{table}");
    }
    for cell in ["ad", "bd"] {
        assert_eq!(of(cell).border_width.top, 1.0, "{cell}");
        assert_eq!(of(cell).border_style.right, BorderStyle::Inset, "{cell}");
    }
    // `border=0` is no frame, and no cell borders either.
    assert_eq!(of("c").border_width.top, 0.0);
    assert_eq!(of("cd").border_width.top, 0.0);
    // Author CSS beats the hint.
    assert_eq!(of("dd").border_style.top, BorderStyle::None);
}

#[test]
fn align_attributes_align_text_and_tables() {
    use noonoo_style::{Float, TextAlign};
    let doc = noonoo_html::parse(
        "<div id=d align=center>a</div><div id=m align=MIDDLE>a</div>\
         <p id=p align=center>a</p><h2 id=h align=right>a</h2><div id=j align=justify>a</div>\
         <div id=x align=bogus>a</div><span id=s align=center>a</span>\
         <table id=tc align=center><tr id=r align=right><td id=c align=center>a<th id=th align=left>b</table>\
         <table id=tl align=left><tr><td>a</table><table id=tr align=right><tr><td>a</table>",
    );
    let sheet = Stylesheet::parse("#m { text-align: left }").unwrap();
    let styles = compute(&doc, &[sheet]);
    let of = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    // Block containers and table parts centre their blocks too.
    assert_eq!(of("d").text_align, TextAlign::WebkitCenter);
    assert_eq!(of("c").text_align, TextAlign::WebkitCenter);
    assert_eq!(of("r").text_align, TextAlign::Right);
    assert_eq!(of("th").text_align, TextAlign::Left);
    // Paragraphs and headings centre only their text.
    assert_eq!(of("p").text_align, TextAlign::Center);
    assert_eq!(of("h").text_align, TextAlign::Right);
    assert_eq!(of("j").text_align, TextAlign::Justify);
    // Author CSS wins; unknown values and other elements are no hint.
    assert_eq!(of("m").text_align, TextAlign::Left);
    assert_eq!(of("x").text_align, TextAlign::Start);
    assert_eq!(of("s").text_align, TextAlign::Start);
    // A table floats left or right, or centres with auto margins.
    assert_eq!(of("tl").float, Float::Left);
    assert_eq!(of("tr").float, Float::Right);
    assert_eq!(of("tc").float, Float::None);
    assert_eq!(of("tc").margin.left, Dimension::Auto);
    assert_eq!(of("tc").margin.right, Dimension::Auto);
    // ... and does not align its cells' text.
    assert_eq!(of("tl").text_align, TextAlign::Start);
}

#[test]
fn embedded_content_keeps_a_zero_size() {
    // A tracking pixel's `width=0 height=0` is a size, where a table's
    // zero width is no hint.
    let html =
        r#"<img id="i" width="0" height="0"><table id="t" width="0"><tr><td>a</td></tr></table>"#;
    assert_eq!(width_of(html, "", "i"), Dimension::Px(0.0));
    assert_eq!(width_of(html, "", "t"), Dimension::Auto);
}

#[test]
fn cells_centre_vertically_and_valign_moves_them() {
    use noonoo_style::VerticalAlign;
    let doc = noonoo_html::parse(
        "<table><tr><td id=m>a<td id=t valign=TOP>b<td id=a>c</tr>\
         <tr valign=bottom><td id=b>d<th id=h valign=baseline>e</tr></table>",
    );
    let sheet = Stylesheet::parse("#a { vertical-align: top }").unwrap();
    let styles = compute(&doc, &[sheet]);
    let of = |id: &str| styles.get(doc.get_element_by_id(id).unwrap()).unwrap();
    assert_eq!(of("m").vertical_align, VerticalAlign::Middle);
    assert_eq!(of("t").vertical_align, VerticalAlign::Top);
    assert_eq!(of("a").vertical_align, VerticalAlign::Top);
    assert_eq!(of("b").vertical_align, VerticalAlign::Bottom);
    assert_eq!(of("h").vertical_align, VerticalAlign::Baseline);
}

#[test]
fn height_attributes_size_tables_rows_and_cells() {
    let doc = noonoo_html::parse(
        r#"<table id=t height=50><tr id=r height="12"><td id=c height="30%">a</td><td id=z height=0>b</td></tr></table><div id=d height=9></div>"#,
    );
    let styles = compute(&doc, &[]);
    let of = |id: &str| {
        styles
            .get(doc.get_element_by_id(id).unwrap())
            .unwrap()
            .height
            .clone()
    };
    assert_eq!(of("t"), Dimension::Px(50.0));
    assert_eq!(of("r"), Dimension::Px(12.0));
    assert_eq!(of("c"), Dimension::Percent(30.0));
    assert_eq!(of("z"), Dimension::Auto);
    assert_eq!(of("d"), Dimension::Auto);
}
