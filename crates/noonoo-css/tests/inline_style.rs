//! Editing an inline `style` attribute one property at a time, as
//! `element.style.setProperty` / `removeProperty` do.

use noonoo_css::{InlineStyleError, remove_inline_property, set_inline_property};

#[test]
fn a_new_property_is_appended() {
    assert_eq!(
        set_inline_property("color: red", "width", "10px", false).unwrap(),
        "color: red; width: 10px"
    );
    assert_eq!(
        set_inline_property("", "width", "10px", false).unwrap(),
        "width: 10px"
    );
}

#[test]
fn an_existing_property_is_replaced_in_place() {
    assert_eq!(
        set_inline_property("color: red; width: 1px; height: 2px", "WIDTH", "10px", true).unwrap(),
        "color: red; width: 10px !important; height: 2px"
    );
}

/// Setting a shorthand replaces its longhands, as `setProperty` does; a
/// longhand set over its shorthand goes after it so it wins.
#[test]
fn order_keeps_the_cascade_right() {
    assert_eq!(
        set_inline_property("margin: 0", "margin-top", "5px", false).unwrap(),
        "margin: 0; margin-top: 5px"
    );
    assert_eq!(
        set_inline_property("margin: 0; margin-top: 5px", "margin", "8px", false).unwrap(),
        "margin: 8px"
    );
    assert_eq!(
        set_inline_property("margin-top: 5px; margin: 0", "margin-top", "9px", false).unwrap(),
        "margin: 0; margin-top: 9px"
    );
}

#[test]
fn custom_properties_keep_their_case() {
    assert_eq!(
        set_inline_property("--Brand: red", "--brand", "blue", false).unwrap(),
        "--Brand: red; --brand: blue"
    );
    assert_eq!(
        set_inline_property("--Brand: red", "--Brand", "blue", false).unwrap(),
        "--Brand: blue"
    );
}

#[test]
fn an_empty_value_removes_the_property() {
    assert_eq!(
        set_inline_property("color: red; width: 1px", "color", "  ", false).unwrap(),
        "width: 1px"
    );
}

/// A value can't smuggle in other declarations or a priority.
#[test]
fn a_value_that_escapes_its_declaration_is_refused() {
    for value in [
        "red; display: none",
        "red !important",
        "red } p { color: blue",
        "a: b",
    ] {
        assert_eq!(
            set_inline_property("width: 1px", "color", value, false),
            Err(InlineStyleError::InvalidValue),
            "{value}"
        );
    }
}

#[test]
fn a_bad_name_is_refused() {
    for name in ["", "col or", "color:", "a;b"] {
        assert_eq!(
            set_inline_property("", name, "red", false),
            Err(InlineStyleError::InvalidName),
            "{name:?}"
        );
    }
}

#[test]
fn an_unsupported_but_well_formed_property_is_kept() {
    assert_eq!(
        set_inline_property("", "letter-spacing", "1px", false).unwrap(),
        "letter-spacing: 1px"
    );
}

#[test]
fn remove_takes_out_every_declaration_of_the_name() {
    assert_eq!(
        remove_inline_property("color: red; width: 1px; Color: blue", "color"),
        "width: 1px"
    );
    assert_eq!(remove_inline_property("width: 1px", "color"), "width: 1px");
}

/// Removing a shorthand removes its longhands too, as `removeProperty` does.
#[test]
fn removing_a_shorthand_removes_its_longhands() {
    assert_eq!(
        remove_inline_property("margin: 0; margin-top: 5px; color: red", "margin"),
        "color: red"
    );
}
