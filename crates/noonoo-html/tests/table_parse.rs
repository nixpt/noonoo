//! Table structure from `parse`: the implied closes keep rows in their
//! row group.

use noonoo_html::parse;
use noonoo_html::serialize::inner_html;

fn round_trip(source: &str) -> String {
    let doc = parse(source);
    inner_html(&doc, doc.root())
}

#[test]
fn rows_stay_in_an_explicit_row_group() {
    let source = "<table><thead><tr><th>h</th></tr></thead><tbody><tr><td>1</td></tr><tr><td>2</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot></table>";
    assert_eq!(round_trip(source), source);
    let doc = parse(source);
    let tbody = doc.query_selector("tbody").unwrap().unwrap();
    let rows = doc.tree().children(tbody);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|&r| doc.element(r).unwrap().name == "tr"));
}

#[test]
fn a_new_row_closes_the_open_row_but_not_its_group() {
    assert_eq!(
        round_trip("<table><tbody><tr><td>1<tr><td>2</table>"),
        "<table><tbody><tr><td>1</td></tr><tr><td>2</td></tr></tbody></table>"
    );
}

#[test]
fn a_new_row_group_closes_the_open_one() {
    assert_eq!(
        round_trip("<table><thead><tr><th>h<tbody><tr><td>1</table>"),
        "<table><thead><tr><th>h</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>"
    );
}

#[test]
fn rows_without_a_row_group_get_an_implied_tbody() {
    // As in HTML5 (and every browser): `table > tr` matches nothing.
    assert_eq!(
        round_trip("<table><tr><td>1<tr><td>2</table>"),
        "<table><tbody><tr><td>1</td></tr><tr><td>2</td></tr></tbody></table>"
    );
}
