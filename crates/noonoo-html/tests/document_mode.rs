//! The document's mode, from its doctype, as HTML's "initial" insertion
//! mode decides it: no-quirks for `<!DOCTYPE html>`, quirks for no doctype
//! or a legacy one, limited-quirks for the transitional XHTML and HTML 4.01
//! doctypes with a system identifier.

use noonoo_dom::{Document, QuirksMode};

fn mode(html: &str) -> QuirksMode {
    noonoo_html::parse(html).quirks_mode()
}

#[test]
fn the_html5_doctype_is_no_quirks() {
    assert_eq!(mode("<!DOCTYPE html><p>x"), QuirksMode::NoQuirks);
    assert_eq!(mode("<!doctype HTML><p>x"), QuirksMode::NoQuirks);
    assert_eq!(mode("<!-- c -->\n<!DocType html>"), QuirksMode::NoQuirks);
    assert_eq!(
        mode(r#"<!DOCTYPE html SYSTEM "about:legacy-compat">"#),
        QuirksMode::NoQuirks
    );
}

#[test]
fn no_doctype_or_a_late_or_foreign_one_is_quirks() {
    assert_eq!(mode("<p>x"), QuirksMode::Quirks);
    assert_eq!(mode(""), QuirksMode::Quirks);
    assert_eq!(mode("<p>x</p><!DOCTYPE html>"), QuirksMode::Quirks);
    assert_eq!(mode("<!DOCTYPE svg>"), QuirksMode::Quirks);
    assert_eq!(mode("<!DOCTYPE>"), QuirksMode::Quirks);
}

#[test]
fn legacy_public_identifiers_are_quirks() {
    assert_eq!(
        mode(r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01 Transitional//EN">"#),
        QuirksMode::Quirks
    );
    assert_eq!(
        mode(r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 3.2 Final//EN">"#),
        QuirksMode::Quirks
    );
    assert_eq!(
        mode(r#"<!doctype html public '-//ietf//dtd html//en'>"#),
        QuirksMode::Quirks
    );
    assert_eq!(
        mode(
            r#"<!DOCTYPE html SYSTEM "http://www.ibm.com/data/dtd/v11/ibmxhtml1-transitional.dtd">"#
        ),
        QuirksMode::Quirks
    );
}

#[test]
fn transitional_doctypes_with_a_system_identifier_are_limited_quirks() {
    assert_eq!(
        mode(
            r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01 Transitional//EN" "http://www.w3.org/TR/html4/loose.dtd">"#
        ),
        QuirksMode::LimitedQuirks
    );
    assert_eq!(
        mode(
            r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">"#
        ),
        QuirksMode::LimitedQuirks
    );
}

#[test]
fn strict_legacy_doctypes_are_no_quirks() {
    assert_eq!(
        mode(
            r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">"#
        ),
        QuirksMode::NoQuirks
    );
    assert_eq!(
        mode(
            r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Strict//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd">"#
        ),
        QuirksMode::NoQuirks
    );
}

#[test]
fn a_new_document_is_no_quirks_and_fragments_keep_the_mode() {
    assert_eq!(Document::new().quirks_mode(), QuirksMode::NoQuirks);
    let mut doc = noonoo_html::parse("<div id=d></div>");
    let div = doc.get_element_by_id("d").unwrap();
    noonoo_html::set_inner_html(&mut doc, div, "<!DOCTYPE html><p>x").unwrap();
    assert_eq!(doc.quirks_mode(), QuirksMode::Quirks);
}

#[test]
fn a_declaration_with_multibyte_text_parses() {
    assert_eq!(mode("<!ab\u{FFFD}cd><p>x"), QuirksMode::Quirks);
    assert_eq!(mode("<!\u{FFFD}\u{FFFD}\u{FFFD}>"), QuirksMode::Quirks);
}
