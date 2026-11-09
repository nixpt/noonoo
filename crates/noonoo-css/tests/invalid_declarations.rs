//! Invalid declarations are dropped while parsing, so they never compete in
//! the cascade (bliss-diff: `display: ` and `display: flow-root` turned
//! blocks inline by beating the user-agent `display: block`).

use noonoo_css::parse_declaration;

#[test]
fn an_empty_value_is_invalid_except_for_a_custom_property() {
    assert!(parse_declaration("display", "", false).is_err());
    assert!(parse_declaration("color", "  ", false).is_err());
    assert!(parse_declaration("--gap", "", false).is_ok());
}

#[test]
fn display_takes_only_the_keywords_layout_knows() {
    for keyword in [
        "none",
        "block",
        "inline",
        "inline-block",
        "flow-root",
        "flex",
        "inline-flex",
        "grid",
        "inline-grid",
        "list-item",
        "table",
        "table-row-group",
        "table-row",
        "table-cell",
        "inherit",
        "initial",
        "unset",
        "BLOCK",
    ] {
        assert!(
            parse_declaration("display", keyword, false).is_ok(),
            "{keyword}"
        );
    }
    for keyword in ["contents", "cnn", "block flex"] {
        assert!(
            parse_declaration("display", keyword, false).is_err(),
            "{keyword}"
        );
    }
    // A var() is checked once it's substituted.
    assert!(parse_declaration("display", "var(--d)", false).is_ok());
}
