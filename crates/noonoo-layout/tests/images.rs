//! `<img>` sizing from loaded images ([`Images`]): CSS sizes, then the
//! `width`/`height` attributes, then the natural size, keeping the ratio.

use noonoo_dom::{Document, NodeId};
use noonoo_geometry::Size;
use noonoo_layout::{Image, Images, LayoutTree, layout_with_images};
use noonoo_text::MetricShaper;

/// A PNG signature and IHDR chunk for a `width` × `height` image.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
    data.extend(13u32.to_be_bytes());
    data.extend(b"IHDR");
    data.extend(width.to_be_bytes());
    data.extend(height.to_be_bytes());
    data.extend([8, 6, 0, 0, 0]);
    data
}

/// A 40 × 30 image loaded as `a.png`.
fn images() -> Images {
    let mut images = Images::new();
    images.insert("a.png", Image::from_bytes(png(40, 30)).unwrap());
    images
}

fn page(html: &str, css: &str) -> (Document, LayoutTree) {
    let doc = noonoo_html::parse(html);
    let sheet = noonoo_css::Stylesheet::parse(css).unwrap();
    let viewport = Size::new(400.0, 600.0);
    let styles = noonoo_style::compute_with(
        &doc,
        &[sheet],
        noonoo_style::Viewport {
            width: viewport.width,
            height: viewport.height,
        },
    );
    let tree = layout_with_images(&doc, &styles, viewport, &MetricShaper, &images());
    (doc, tree)
}

fn id(doc: &Document, name: &str) -> NodeId {
    doc.get_element_by_id(name).unwrap()
}

/// The content box's width and height.
fn size(doc: &Document, tree: &LayoutTree, name: &str) -> (f32, f32) {
    let content = tree.get(id(doc, name)).unwrap().content_box();
    (content.width, content.height)
}

#[test]
fn a_loaded_image_takes_its_natural_size() {
    let (doc, tree) = page(r#"<p><img id="i" src="a.png" alt="pic"></p>"#, "");
    assert_eq!(size(&doc, &tree, "i"), (40.0, 30.0));
    // No alt text is drawn over a loaded image.
    assert!(
        tree.boxes()
            .iter()
            .all(|b| b.text.as_deref() != Some("pic"))
    );
}

#[test]
fn one_given_dimension_keeps_the_ratio() {
    let (doc, tree) = page(
        r#"<img id="a" src="a.png"><img id="b" src="a.png" height="15">"#,
        "#a { width: 80px }",
    );
    assert_eq!(size(&doc, &tree, "a"), (80.0, 60.0));
    assert_eq!(size(&doc, &tree, "b"), (20.0, 15.0));
}

#[test]
fn css_sizes_override_the_attributes_one_axis_at_a_time() {
    let (doc, tree) = page(
        r#"<img id="a" src="a.png" width="10" height="10">"#,
        "#a { width: 80px }",
    );
    assert_eq!(size(&doc, &tree, "a"), (80.0, 10.0));
}

#[test]
fn max_width_scales_the_image_down() {
    let (doc, tree) = page(
        r#"<div><img id="a" src="a.png"></div>"#,
        "div { width: 20px } img { max-width: 100% }",
    );
    assert_eq!(size(&doc, &tree, "a"), (20.0, 15.0));
}

#[test]
fn padding_and_border_sit_outside_the_image() {
    let (doc, tree) = page(
        r#"<img id="a" src="a.png">"#,
        "img { padding: 2px; border: 3px solid #000 }",
    );
    let image = tree.get(id(&doc, "a")).unwrap();
    assert_eq!((image.rect.width, image.rect.height), (50.0, 40.0));
    assert_eq!(size(&doc, &tree, "a"), (40.0, 30.0));
    let (doc, tree) = page(
        r#"<img id="a" src="a.png">"#,
        "img { box-sizing: border-box; width: 50px; padding: 5px }",
    );
    assert_eq!(size(&doc, &tree, "a"), (40.0, 30.0));
}

#[test]
fn block_floated_and_positioned_images_keep_their_size() {
    let (doc, tree) = page(
        r#"<img id="block" src="a.png"><div id="float"><img src="a.png"></div><img id="abs" src="a.png">"#,
        "#block { display: block } #float { float: left } #abs { position: absolute }",
    );
    assert_eq!(size(&doc, &tree, "block"), (40.0, 30.0));
    assert_eq!(size(&doc, &tree, "abs"), (40.0, 30.0));
    // A shrink-to-fit container wraps the image.
    assert_eq!(size(&doc, &tree, "float").0, 40.0);
}

#[test]
fn the_tree_records_each_drawn_image() {
    let (doc, tree) = page(
        r#"<img id="a" src="a.png"><img id="b" src="missing.png" alt="gone">"#,
        "",
    );
    let image = tree.image(id(&doc, "a")).unwrap();
    assert_eq!(image.data[..8], png(1, 1)[..8]);
    assert!(tree.image(id(&doc, "b")).is_none());
    // A missing image keeps its alt text.
    assert!(
        tree.boxes()
            .iter()
            .any(|b| b.text.as_deref() == Some("gone"))
    );
}

#[test]
fn an_image_sits_on_the_line_like_an_inline_block_of_its_size() {
    let (doc, tree) = page(
        r#"<p id="p">ab<img id="a" src="a.png"></p>"#,
        "p { margin: 0 }",
    );
    let (other, reference) = page(
        r#"<p id="p">ab<span id="a"></span></p>"#,
        "p { margin: 0 } span { display: inline-block; width: 40px; height: 30px }",
    );
    let rect =
        |doc: &Document, tree: &LayoutTree, name: &str| tree.get(id(doc, name)).unwrap().rect;
    assert_eq!(rect(&doc, &tree, "a"), rect(&other, &reference, "a"));
    assert_eq!(rect(&doc, &tree, "p"), rect(&other, &reference, "p"));
}

#[test]
fn min_height_scales_the_width_through_the_ratio() {
    let (doc, tree) = page(r#"<img id="a" src="a.png">"#, "img { min-height: 60px }");
    assert_eq!(size(&doc, &tree, "a"), (80.0, 60.0));
    // A given width stays put.
    let (doc, tree) = page(
        r#"<img id="a" src="a.png">"#,
        "img { width: 40px; min-height: 60px }",
    );
    assert_eq!(size(&doc, &tree, "a"), (40.0, 60.0));
}

#[test]
fn an_image_cannot_shrink_below_its_width() {
    // A flex item never shrinks below its min-content width: the image's.
    let (doc, tree) = page(
        r#"<div><img id="a" src="a.png"><img id="b" src="a.png"></div>"#,
        "div { display: flex; width: 50px }",
    );
    assert_eq!(size(&doc, &tree, "a"), (40.0, 30.0));
    assert_eq!(size(&doc, &tree, "b").0, 40.0);
}

#[test]
fn the_width_attribute_alone_keeps_the_ratio() {
    let (doc, tree) = page(r#"<img id="a" src="a.png" width="20">"#, "");
    assert_eq!(size(&doc, &tree, "a"), (20.0, 15.0));
}

#[test]
fn only_img_elements_are_replaced() {
    let (doc, tree) = page(
        r#"<span id="a" src="a.png"></span>"#,
        "span { display: inline-block }",
    );
    assert_eq!(size(&doc, &tree, "a").0, 0.0);
    assert!(tree.image(id(&doc, "a")).is_none());
}

#[test]
fn percentages_count_for_nothing_in_an_intrinsic_width() {
    // The float's width depends on the image's, so the image's percentage
    // minimum has nothing to resolve against there.
    let (doc, tree) = page(
        r#"<div id="f"><img src="a.png"></div>"#,
        "#f { float: left } img { min-width: 10% }",
    );
    assert_eq!(size(&doc, &tree, "f").0, 40.0);
}
