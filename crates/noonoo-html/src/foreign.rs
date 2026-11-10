//! Foreign content: which namespace a new element joins, and the case SVG
//! and MathML attribute names take (HTML §13.2.6.5, "adjust SVG
//! attributes" and "adjust MathML attributes").
//!
//! Not modelled: the start tags that break out of foreign content (a
//! `<div>` straight inside `<svg>` stays there), SVG tag-name case
//! (`clipPath` stays `clippath`) and the namespaced `xlink:`/`xml:`
//! attributes.

use noonoo_dom::{Document, Namespace, NodeId};

/// The namespace an element named `name` joins under `parent`: `<svg>`
/// starts SVG and `<math>` MathML; anything else takes its parent's,
/// except that an HTML integration point's children are HTML.
pub(crate) fn namespace_of(doc: &Document, parent: NodeId, name: &str) -> Namespace {
    match name {
        "svg" => return Namespace::Svg,
        "math" => return Namespace::MathMl,
        _ => {}
    }
    let Some(parent) = doc.element(parent) else {
        return Namespace::Html;
    };
    match parent.namespace {
        Namespace::Svg if matches!(parent.name.as_str(), "foreignobject" | "desc" | "title") => {
            Namespace::Html
        }
        Namespace::MathMl if is_mathml_text_point(parent) => Namespace::Html,
        namespace => namespace,
    }
}

/// MathML's text integration points, and an `annotation-xml` holding HTML.
fn is_mathml_text_point(element: &noonoo_dom::ElementData) -> bool {
    match element.name.as_str() {
        "mi" | "mo" | "mn" | "ms" | "mtext" => true,
        "annotation-xml" => element.attr("encoding").is_some_and(|encoding| {
            encoding.eq_ignore_ascii_case("text/html")
                || encoding.eq_ignore_ascii_case("application/xhtml+xml")
        }),
        _ => false,
    }
}

/// The name a (lowercased) attribute takes on an element in `namespace`.
pub(crate) fn adjust_attribute(namespace: Namespace, name: &str) -> &str {
    match namespace {
        Namespace::Html => name,
        Namespace::MathMl => {
            if name == "definitionurl" {
                "definitionURL"
            } else {
                name
            }
        }
        Namespace::Svg => SVG_ATTRIBUTES
            .binary_search_by(|(lower, _)| lower.cmp(&name))
            .map_or(name, |index| SVG_ATTRIBUTES[index].1),
    }
}

/// HTML's SVG attribute table, sorted by the lowercase name.
const SVG_ATTRIBUTES: &[(&str, &str)] = &[
    ("attributename", "attributeName"),
    ("attributetype", "attributeType"),
    ("basefrequency", "baseFrequency"),
    ("baseprofile", "baseProfile"),
    ("calcmode", "calcMode"),
    ("clippathunits", "clipPathUnits"),
    ("diffuseconstant", "diffuseConstant"),
    ("edgemode", "edgeMode"),
    ("filterunits", "filterUnits"),
    ("glyphref", "glyphRef"),
    ("gradienttransform", "gradientTransform"),
    ("gradientunits", "gradientUnits"),
    ("kernelmatrix", "kernelMatrix"),
    ("kernelunitlength", "kernelUnitLength"),
    ("keypoints", "keyPoints"),
    ("keysplines", "keySplines"),
    ("keytimes", "keyTimes"),
    ("lengthadjust", "lengthAdjust"),
    ("limitingconeangle", "limitingConeAngle"),
    ("markerheight", "markerHeight"),
    ("markerunits", "markerUnits"),
    ("markerwidth", "markerWidth"),
    ("maskcontentunits", "maskContentUnits"),
    ("maskunits", "maskUnits"),
    ("numoctaves", "numOctaves"),
    ("pathlength", "pathLength"),
    ("patterncontentunits", "patternContentUnits"),
    ("patterntransform", "patternTransform"),
    ("patternunits", "patternUnits"),
    ("pointsatx", "pointsAtX"),
    ("pointsaty", "pointsAtY"),
    ("pointsatz", "pointsAtZ"),
    ("preservealpha", "preserveAlpha"),
    ("preserveaspectratio", "preserveAspectRatio"),
    ("primitiveunits", "primitiveUnits"),
    ("refx", "refX"),
    ("refy", "refY"),
    ("repeatcount", "repeatCount"),
    ("repeatdur", "repeatDur"),
    ("requiredextensions", "requiredExtensions"),
    ("requiredfeatures", "requiredFeatures"),
    ("specularconstant", "specularConstant"),
    ("specularexponent", "specularExponent"),
    ("spreadmethod", "spreadMethod"),
    ("startoffset", "startOffset"),
    ("stddeviation", "stdDeviation"),
    ("stitchtiles", "stitchTiles"),
    ("surfacescale", "surfaceScale"),
    ("systemlanguage", "systemLanguage"),
    ("tablevalues", "tableValues"),
    ("targetx", "targetX"),
    ("targety", "targetY"),
    ("textlength", "textLength"),
    ("viewbox", "viewBox"),
    ("viewtarget", "viewTarget"),
    ("xchannelselector", "xChannelSelector"),
    ("ychannelselector", "yChannelSelector"),
    ("zoomandpan", "zoomAndPan"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_svg_table_is_sorted_and_lowercase_keyed() {
        assert!(SVG_ATTRIBUTES.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(
            SVG_ATTRIBUTES
                .iter()
                .all(|(lower, proper)| proper.to_ascii_lowercase() == *lower)
        );
    }
}
