//! Foreign content: `<svg>` and `<math>` and what they hold are SVG and
//! MathML elements, whose attribute names the parser adjusts to their
//! proper case (HTML's "adjust SVG attributes" and "adjust MathML
//! attributes"); an HTML integration point's children are HTML again.

use noonoo_dom::{Document, Namespace};

fn element<'a>(doc: &'a Document, id: &str) -> &'a noonoo_dom::ElementData {
    doc.element(doc.get_element_by_id(id).unwrap()).unwrap()
}

#[test]
fn svg_attributes_take_their_proper_case() {
    let doc = noonoo_html::parse(
        r#"<div id=h VIEWBOX="x" Data-A="1"></div>
           <svg id=s viewbox="0 0 10 10" PRESERVEASPECTRATIO="none" Width="5">
             <linearGradient id=g gradientunits="userSpaceOnUse"></linearGradient>
             <foreignObject id=f refx="1"><div id=d viewbox="2"></div></foreignObject>
             <desc id=desc><p id=p viewbox="3"></p></desc>
           </svg>
           <math id=m definitionurl="u"><mi id=mi><span id=sp definitionurl="v"></span></mi></math>"#,
    );
    let h = element(&doc, "h");
    assert_eq!(h.namespace, Namespace::Html);
    assert_eq!(
        (h.attr("viewbox"), h.attr("data-a")),
        (Some("x"), Some("1"))
    );
    let s = element(&doc, "s");
    assert_eq!(s.namespace, Namespace::Svg);
    assert_eq!(s.attr("viewBox"), Some("0 0 10 10"));
    assert_eq!(s.attr("preserveAspectRatio"), Some("none"));
    // Names outside the table are lowercased.
    assert_eq!(s.attr("width"), Some("5"));
    assert_eq!(
        element(&doc, "g").attr("gradientUnits"),
        Some("userSpaceOnUse")
    );
    let f = element(&doc, "f");
    assert_eq!((f.namespace, f.attr("refX")), (Namespace::Svg, Some("1")));
    // An integration point's children are HTML.
    let d = element(&doc, "d");
    assert_eq!(
        (d.namespace, d.attr("viewbox")),
        (Namespace::Html, Some("2"))
    );
    assert_eq!(element(&doc, "p").namespace, Namespace::Html);
    let m = element(&doc, "m");
    assert_eq!(
        (m.namespace, m.attr("definitionURL")),
        (Namespace::MathMl, Some("u"))
    );
    assert_eq!(element(&doc, "mi").namespace, Namespace::MathMl);
    let sp = element(&doc, "sp");
    assert_eq!(
        (sp.namespace, sp.attr("definitionurl")),
        (Namespace::Html, Some("v"))
    );
}

#[test]
fn an_svg_closes_back_to_html() {
    let doc =
        noonoo_html::parse(r#"<svg><rect id=r rx="1"/></svg><div id=after viewbox="1"></div>"#);
    assert_eq!(element(&doc, "r").namespace, Namespace::Svg);
    let after = element(&doc, "after");
    assert_eq!(
        (after.namespace, after.attr("viewbox")),
        (Namespace::Html, Some("1"))
    );
}
