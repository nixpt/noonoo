//! Foreign content: the case SVG and MathML attribute names take (HTML
//! §13.2.6.1, "adjust SVG attributes" and "adjust MathML attributes").
//!
//! Not modelled: SVG tag-name case (`clipPath` stays `clippath`, as element
//! names are lowercase in the DOM) and the namespaced `xlink:`/`xml:`
//! attributes, which keep their prefixed names.

use noonoo_dom::Namespace;

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
