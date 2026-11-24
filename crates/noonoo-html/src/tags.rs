//! The element kinds the tree construction rules name, interned: the stack
//! of open elements is searched by kind on almost every token, so a kind is
//! one byte rather than a string.

use noonoo_dom::{ElementData, Namespace};

macro_rules! tags {
    ($($variant:ident $name:literal,)*) => {
        /// What the parsing rules know an open element (or a tag) as.
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub(crate) enum Tag {
            $(#[doc = $name] $variant,)*
            /// An HTML element no rule names.
            Other,
            /// MathML `mi`, `mo`, `mn`, `ms` or `mtext`: a text integration
            /// point.
            MathText,
            /// A MathML `annotation-xml` that isn't an integration point.
            Annotation,
            /// A MathML `annotation-xml` holding HTML: an HTML integration
            /// point.
            AnnotationHtml,
            /// Any other MathML element.
            Math,
            /// SVG `foreignObject`, `desc` or `title`: an HTML integration
            /// point.
            SvgPoint,
            /// Any other SVG element.
            Svg,
        }

        /// Every named kind, sorted by name.
        const NAMES: &[(&str, Tag)] = &[$(($name, Tag::$variant),)*];
    };
}

tags! {
    A "a", Address "address", Applet "applet", Area "area", Article "article", Aside "aside",
    B "b", Base "base", Basefont "basefont", Bgsound "bgsound", Big "big",
    Blockquote "blockquote", Body "body", Br "br", Button "button", Caption "caption",
    Center "center", Code "code", Col "col", Colgroup "colgroup", Dd "dd", Details "details",
    Dialog "dialog", Dir "dir", Div "div", Dl "dl", Dt "dt", Em "em", Embed "embed",
    Fieldset "fieldset", Figcaption "figcaption", Figure "figure", Font "font",
    Footer "footer", Form "form", Frame "frame", Frameset "frameset", H1 "h1", H2 "h2",
    H3 "h3", H4 "h4", H5 "h5", H6 "h6", Head "head", Header "header", Hgroup "hgroup",
    Hr "hr", Html "html", I "i", Iframe "iframe", Image "image", Img "img", Input "input",
    Keygen "keygen", Li "li", Link "link", Listing "listing", Main "main",
    Malignmark "malignmark", Marquee "marquee", MathRoot "math", Menu "menu", Meta "meta",
    Mglyph "mglyph", Nav "nav", Nobr "nobr", Noembed "noembed", Noframes "noframes",
    Noscript "noscript", Object "object", Ol "ol", Optgroup "optgroup", Option "option",
    P "p", Param "param", Plaintext "plaintext", Pre "pre", Rb "rb", Rp "rp", Rt "rt",
    Rtc "rtc", Ruby "ruby", S "s", Script "script", Search "search", Section "section",
    Select "select", Small "small", Source "source", Span "span", Strike "strike",
    Strong "strong", Style "style", Sub "sub", Summary "summary", Sup "sup", SvgRoot "svg",
    Table "table", Tbody "tbody", Td "td", Template "template", Textarea "textarea",
    Tfoot "tfoot", Th "th", Thead "thead", Title "title", Tr "tr", Track "track", Tt "tt",
    U "u", Ul "ul", Var "var", Wbr "wbr", Xmp "xmp",
}

impl Tag {
    /// The kind of the tag `name` (lowercase), as an HTML element.
    pub(crate) fn of(name: &str) -> Tag {
        NAMES
            .binary_search_by(|(known, _)| known.cmp(&name))
            .map_or(Tag::Other, |index| NAMES[index].1)
    }

    /// The kind of an element already in the tree.
    pub(crate) fn of_element(element: &ElementData) -> Tag {
        match element.namespace {
            Namespace::Html => Tag::of(&element.name),
            Namespace::Svg => match element.name.as_str() {
                "foreignobject" | "desc" | "title" => Tag::SvgPoint,
                _ => Tag::Svg,
            },
            Namespace::MathMl => match element.name.as_str() {
                "mi" | "mo" | "mn" | "ms" | "mtext" => Tag::MathText,
                "annotation-xml" => {
                    let html = element.attr("encoding").is_some_and(|encoding| {
                        encoding.eq_ignore_ascii_case("text/html")
                            || encoding.eq_ignore_ascii_case("application/xhtml+xml")
                    });
                    if html {
                        Tag::AnnotationHtml
                    } else {
                        Tag::Annotation
                    }
                }
                _ => Tag::Math,
            },
        }
    }

    /// Whether this is an element in the HTML namespace.
    pub(crate) fn is_html(self) -> bool {
        self.namespace() == Namespace::Html
    }

    /// The namespace an element of this kind is in.
    pub(crate) fn namespace(self) -> Namespace {
        match self {
            Tag::SvgPoint | Tag::Svg => Namespace::Svg,
            Tag::MathText | Tag::Annotation | Tag::AnnotationHtml | Tag::Math => Namespace::MathMl,
            _ => Namespace::Html,
        }
    }

    /// A MathML text, MathML annotation or SVG integration point: foreign
    /// elements that bound a scope and are "special".
    fn is_foreign_boundary(self) -> bool {
        matches!(
            self,
            Tag::MathText | Tag::Annotation | Tag::AnnotationHtml | Tag::SvgPoint
        )
    }

    pub(crate) fn is_heading(self) -> bool {
        matches!(
            self,
            Tag::H1 | Tag::H2 | Tag::H3 | Tag::H4 | Tag::H5 | Tag::H6
        )
    }

    /// Where "has an element in scope" stops.
    pub(crate) fn bounds_scope(self) -> bool {
        matches!(
            self,
            Tag::Applet
                | Tag::Caption
                | Tag::Html
                | Tag::Table
                | Tag::Td
                | Tag::Th
                | Tag::Marquee
                | Tag::Object
                | Tag::Select
                | Tag::Template
        ) || self.is_foreign_boundary()
    }

    /// Where "has an element in list item scope" stops.
    pub(crate) fn bounds_list_item_scope(self) -> bool {
        matches!(self, Tag::Ol | Tag::Ul) || self.bounds_scope()
    }

    /// Where "has an element in button scope" stops.
    pub(crate) fn bounds_button_scope(self) -> bool {
        self == Tag::Button || self.bounds_scope()
    }

    /// Where "has an element in table scope" stops.
    pub(crate) fn bounds_table_scope(self) -> bool {
        matches!(self, Tag::Html | Tag::Table | Tag::Template)
    }

    /// The elements an end tag may leave out, which the rules close on
    /// their own ("generate implied end tags").
    pub(crate) fn has_implied_end(self) -> bool {
        matches!(
            self,
            Tag::Dd
                | Tag::Dt
                | Tag::Li
                | Tag::Optgroup
                | Tag::Option
                | Tag::P
                | Tag::Rb
                | Tag::Rp
                | Tag::Rt
                | Tag::Rtc
        )
    }

    /// The elements whose content fosters out of them.
    pub(crate) fn fosters(self) -> bool {
        matches!(
            self,
            Tag::Table | Tag::Tbody | Tag::Tfoot | Tag::Thead | Tag::Tr
        )
    }

    /// The elements kept in the list of active formatting elements.
    pub(crate) fn is_formatting(self) -> bool {
        matches!(
            self,
            Tag::A
                | Tag::B
                | Tag::Big
                | Tag::Code
                | Tag::Em
                | Tag::Font
                | Tag::I
                | Tag::Nobr
                | Tag::S
                | Tag::Small
                | Tag::Strike
                | Tag::Strong
                | Tag::Tt
                | Tag::U
        )
    }

    /// The spec's "special" category.
    pub(crate) fn is_special(self) -> bool {
        matches!(
            self,
            Tag::Address
                | Tag::Applet
                | Tag::Area
                | Tag::Article
                | Tag::Aside
                | Tag::Base
                | Tag::Basefont
                | Tag::Bgsound
                | Tag::Blockquote
                | Tag::Body
                | Tag::Br
                | Tag::Button
                | Tag::Caption
                | Tag::Center
                | Tag::Col
                | Tag::Colgroup
                | Tag::Dd
                | Tag::Details
                | Tag::Dir
                | Tag::Div
                | Tag::Dl
                | Tag::Dt
                | Tag::Embed
                | Tag::Fieldset
                | Tag::Figcaption
                | Tag::Figure
                | Tag::Footer
                | Tag::Form
                | Tag::Frame
                | Tag::Frameset
                | Tag::Head
                | Tag::Header
                | Tag::Hgroup
                | Tag::Hr
                | Tag::Html
                | Tag::Iframe
                | Tag::Img
                | Tag::Input
                | Tag::Keygen
                | Tag::Li
                | Tag::Link
                | Tag::Listing
                | Tag::Main
                | Tag::Marquee
                | Tag::Menu
                | Tag::Meta
                | Tag::Nav
                | Tag::Noembed
                | Tag::Noframes
                | Tag::Noscript
                | Tag::Object
                | Tag::Ol
                | Tag::P
                | Tag::Param
                | Tag::Plaintext
                | Tag::Pre
                | Tag::Script
                | Tag::Search
                | Tag::Section
                | Tag::Select
                | Tag::Source
                | Tag::Style
                | Tag::Summary
                | Tag::Table
                | Tag::Tbody
                | Tag::Td
                | Tag::Template
                | Tag::Textarea
                | Tag::Tfoot
                | Tag::Th
                | Tag::Thead
                | Tag::Title
                | Tag::Tr
                | Tag::Track
                | Tag::Ul
                | Tag::Wbr
                | Tag::Xmp
        ) || self.is_heading()
            || self.is_foreign_boundary()
    }

    /// Start tags that end foreign content (`<font>` also does, with some
    /// attributes).
    pub(crate) fn breaks_out_of_foreign(self) -> bool {
        matches!(
            self,
            Tag::B
                | Tag::Big
                | Tag::Blockquote
                | Tag::Body
                | Tag::Br
                | Tag::Center
                | Tag::Code
                | Tag::Dd
                | Tag::Div
                | Tag::Dl
                | Tag::Dt
                | Tag::Em
                | Tag::Embed
                | Tag::Head
                | Tag::Hr
                | Tag::I
                | Tag::Img
                | Tag::Li
                | Tag::Listing
                | Tag::Menu
                | Tag::Meta
                | Tag::Nobr
                | Tag::Ol
                | Tag::P
                | Tag::Pre
                | Tag::Ruby
                | Tag::S
                | Tag::Small
                | Tag::Span
                | Tag::Strong
                | Tag::Strike
                | Tag::Sub
                | Tag::Sup
                | Tag::Table
                | Tag::Tt
                | Tag::U
                | Tag::Ul
                | Tag::Var
        ) || self.is_heading()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sorted_so_the_lookup_finds_each() {
        assert!(NAMES.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for &(name, tag) in NAMES {
            assert_eq!(Tag::of(name), tag);
        }
        assert_eq!(Tag::of("x-y"), Tag::Other);
    }
}
