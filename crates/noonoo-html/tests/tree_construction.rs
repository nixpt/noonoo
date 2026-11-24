//! Broken markup gets the tree the HTML standard says (§13.2.6): the
//! expected trees here are the spec's.

mod common;

use common::{check, tree};

fn document(source: &str) -> String {
    let doc = noonoo_html::parse(source);
    tree(&doc, doc.root())
}

fn inner(context: &str, source: &str) -> String {
    let mut doc = noonoo_html::parse("<!doctype html>");
    let node = doc.create_element(context);
    noonoo_html::set_inner_html(&mut doc, node, source).unwrap();
    tree(&doc, node)
}

#[test]
fn misnested_formatting_is_adopted() {
    check(&[
        ("<p><b>1<i>2</b>3</i>4", r#"p[b["1" i["2"]] i["3"] "4"]"#),
        ("<b>1<p>2</b>3</p>4", r#"b["1"] p[b["2"] "3"] "4""#),
        (
            "<font color=a><p>1</font>2",
            r#"font(color=a) p[font(color=a)["1"] "2"]"#,
        ),
        (
            "<a><div><div><div><div>1</a>2",
            r#"a div[a div[a div[a div[a["1"] "2"]]]]"#,
        ),
        // A cell's content can't close what is outside the table.
        (
            "<b>1<div><table><tr><td></b>2</table>3",
            r#"b["1" div[table[tbody[tr[td["2"]]]] "3"]]"#,
        ),
        // Nor an applet's.
        ("<applet><b>a</applet>b", r#"applet[b["a"]] "b""#),
    ]);
}

/// The adoption agency goes down eight blocks, leaving an `a` in each.
#[test]
fn an_end_tag_reaches_into_eight_blocks() {
    for blocks in 0..12 {
        let source = format!("<a>{}1</a>2", "<div>".repeat(blocks));
        let reached = blocks.min(8);
        // Going down eight leaves none to close the link with: it holds the
        // blocks after them.
        let mut expected = match blocks - reached {
            0 if reached < 8 => r#"a["1"] "2""#.to_owned(),
            rest => format!("a[{}\"12\"{}]", "div[".repeat(rest), "]".repeat(rest)),
        };
        for _ in 0..reached {
            expected = format!("a div[{expected}]");
        }
        check(&[(&source, &expected)]);
    }
}

#[test]
fn an_unclosed_paragraph_passes_its_formatting_on() {
    check(&[(
        "<p><b class=x>1<p>2</b>3",
        r#"p[b(class=x)["1"]] p[b(class=x)["2"] "3"]"#,
    )]);
    // At most three alike are reopened.
    check(&[(
        "<p><b><b><b><b><b>1<p>2",
        r#"p[b[b[b[b[b["1"]]]]]] p[b[b[b["2"]]]]"#,
    )]);
}

#[test]
fn a_link_closes_an_open_link() {
    check(&[
        (
            "<a href=1>x<a href=2>y</a>z",
            r#"a(href=1)["x"] a(href=2)["y"] "z""#,
        ),
        (
            "<a href=1><p>x<a href=2>y",
            r#"a(href=1) p[a(href=1)["x"] a(href=2)["y"]]"#,
        ),
        ("<nobr>1<nobr>2", r#"nobr["1"] nobr["2"]"#),
    ]);
}

#[test]
fn a_link_outside_the_scope_of_the_next_is_not_left_open() {
    check(&[(
        "<a><math><mi><a>1</a></math>2",
        r#"a[math:math[math:mi[a["1"]]]] "2""#,
    )]);
}

#[test]
fn up_to_three_alike_formatting_elements_reopen() {
    check(&[
        (
            "<p><b x=1><b x=2><b x=3><b x=4>1<p>2",
            r#"p[b(x=1)[b(x=2)[b(x=3)[b(x=4)["1"]]]]] p[b(x=1)[b(x=2)[b(x=3)[b(x=4)["2"]]]]]"#,
        ),
        (
            "<p><b x=1><b x=1><b x=1><b x=1>1<p>2",
            r#"p[b(x=1)[b(x=1)[b(x=1)[b(x=1)["1"]]]]] p[b(x=1)[b(x=1)[b(x=1)["2"]]]]"#,
        ),
        (
            "<p><b x=1 y=1><b y=1 x=1><b x=1 y=1><b x=1 y=1>1<p>2",
            concat!(
                r#"p[b(x=1 y=1)[b(y=1 x=1)[b(x=1 y=1)[b(x=1 y=1)["1"]]]]] "#,
                r#"p[b(y=1 x=1)[b(x=1 y=1)[b(x=1 y=1)["2"]]]]"#
            ),
        ),
    ]);
}

/// How many differ is the page's affair.
#[test]
fn many_formatting_elements_reopen() {
    let open: String = (0..500).map(|n| format!("<b x={n}>")).collect();
    let nested = |text: &str| {
        let open: String = (0..500).map(|n| format!("b(x={n})[")).collect();
        format!("{open}\"{text}\"{}", "]".repeat(500))
    };
    let expected = format!("p[{}] p[{}]", nested("1"), nested("2"));
    check(&[(&format!("<p>{open}1<p>2"), &expected)]);
}

/// So is how deep it nests.
#[test]
fn an_end_tag_closes_an_element_far_up() {
    let spans = 400;
    let source = format!("<div>{}</div>x", "<span>".repeat(spans));
    let nested = format!("{}span{}", "span[".repeat(spans - 1), "]".repeat(spans - 1));
    check(&[(&source, &format!(r#"div[{nested}] "x""#))]);
    // Past the depth limit the tree is flat, but the stack is not.
    let source = format!("<div>{}</div>x", "<span>".repeat(4000));
    let doc = noonoo_html::parse(&source);
    let body = doc.get_elements_by_tag_name("body")[0];
    assert_eq!(doc.tree().children(body).len(), 2);
}

#[test]
fn a_caption_takes_its_formatting_with_it() {
    check(&[(
        "<table><caption><b>1</caption>2",
        r#""2" table[caption[b["1"]]]"#,
    )]);
}

#[test]
fn preformatted_text_rules_a_frameset_out() {
    check(&[
        ("<pre></pre><frameset>", "pre"),
        ("<listing></listing><frameset>", "listing"),
    ]);
}

#[test]
fn content_misnested_in_a_table_goes_before_it() {
    check(&[
        (
            "<table>oops<tr><td>1</table>",
            r#""oops" table[tbody[tr[td["1"]]]]"#,
        ),
        (
            "<table><div>x</div><tr><td>1</table>",
            r#"div["x"] table[tbody[tr[td["1"]]]]"#,
        ),
        (
            "<table><tr>a<td>b</td>c</tr>d</table>e",
            r#""acd" table[tbody[tr[td["b"]]]] "e""#,
        ),
        (
            "x<table>y<b>z<tr><td>1",
            r#""xy" b["z"] table[tbody[tr[td["1"]]]]"#,
        ),
        (
            "<table><a>1<tr><td></a>2</table>",
            r#"a["1"] table[tbody[tr[td["2"]]]]"#,
        ),
        // White space stays where it is.
        (
            "<table> <tr> <td>1</td> </tr> </table>",
            r#"table[" " tbody[tr[" " td["1"] " "] " "]]"#,
        ),
        (
            "<table><textarea>a</textarea><select><option>b</select><tr><td>c</table>",
            r#"textarea["a"] select[option["b"]] table[tbody[tr[td["c"]]]]"#,
        ),
    ]);
}

#[test]
fn a_table_without_a_parent_fosters_into_its_stack_parent() {
    assert_eq!(inner("table", "a<tr><td>b"), r#""a" tbody[tr[td["b"]]]"#);
    assert_eq!(
        inner("div", "<table>a<tr><td>b"),
        r#""a" table[tbody[tr[td["b"]]]]"#
    );
}

#[test]
fn hidden_inputs_and_forms_stay_in_their_table() {
    check(&[
        (
            "<div><table><input type=hidden name=h><input name=v><tr><td>1</table></div>",
            r#"div[input(name=v) table[input(type=hidden name=h) tbody[tr[td["1"]]]]]"#,
        ),
        (
            "<table><input type=HIDDEN><form><input name=a></form><tr><td>1</table>",
            r#"input(name=a) table[input(type=HIDDEN) form tbody[tr[td["1"]]]]"#,
        ),
        // The form is empty, but owns the controls up to its end tag.
        (
            "<table><form><tr><td><input name=x></td></tr></form></table>z",
            r#"table[form tbody[tr[td[input(name=x)]]]] "z""#,
        ),
    ]);
}

#[test]
fn a_form_in_a_form_is_ignored() {
    check(&[
        (
            "<form id=a><form id=b><input></form>x</form>y",
            r#"form(id=a)[input] "xy""#,
        ),
        (
            "<form id=a><div><form id=b></div></form><form id=c>",
            "form(id=a)[div] form(id=c)",
        ),
        ("<div><form>a</div>b</form>c", r#"div[form["a"]] "bc""#),
        // Not in a template.
        (
            "<form><template><form></form></template></form>",
            "form[template[form]]",
        ),
    ]);
    // A fragment knows the form it is in.
    let mut doc = noonoo_html::parse("<!doctype html><form><div id=d>");
    let div = doc.get_element_by_id("d").unwrap();
    noonoo_html::set_inner_html(&mut doc, div, "<form><input></form>").unwrap();
    assert_eq!(tree(&doc, div), "input");
}

/// The form the fragment is in isn't on the stack of open elements: its end
/// tag closes nothing.
#[test]
fn a_fragments_own_form_is_not_closed_in_it() {
    assert_eq!(inner("form", "<p>a</form>b"), r#"p["ab"]"#);
    assert_eq!(
        inner("form", "<div><p>a</form>b</div><form>c"),
        r#"div[p["ab"]] form["c"]"#
    );
}

#[test]
fn a_button_closes_an_open_button() {
    check(&[
        (
            "<button>1<button>2</button>3",
            r#"button["1"] button["2"] "3""#,
        ),
        (
            "<button><b>1<button>2</button>3",
            r#"button[b["1"]] b[button["2"] "3"]"#,
        ),
        // A paragraph in a button doesn't close the one around it.
        (
            "<p>a<button>b<p>c</button>d",
            r#"p["a" button["b" p["c"]] "d"]"#,
        ),
    ]);
}

// The current rules for select: any element may be in one (the "in select"
// insertion modes are gone).
#[test]
fn a_select_holds_markup_but_not_controls() {
    check(&[
        (
            "<select><div><option>a</div><option>b</select>c",
            r#"select[div[option["a"]] option["b"]] "c""#,
        ),
        (
            "<select><option>a<option>b<optgroup><option>c<hr><option>d</select>",
            r#"select[option["a"] option["b"] optgroup[option["c"]] hr option["d"]]"#,
        ),
        (
            "<select><b>1<option>2</b></select>3",
            r#"select[b["1" option["2"]]] "3""#,
        ),
        // A select or an input closes it.
        (
            "<select><option>a<select><option>b",
            r#"select[option["a"]] option["b"]"#,
        ),
        (
            "<select><option>a<input name=x>b",
            r#"select[option["a"]] input(name=x) "b""#,
        ),
        // So does the table around it.
        (
            "<table><tr><td><select><option>a<td>b</table>",
            r#"table[tbody[tr[td[select[option["a"]]] td["b"]]]]"#,
        ),
    ]);
    assert_eq!(
        inner("select", "<input><select><option>a"),
        r#"option["a"]"#
    );
}

#[test]
fn controls_group_as_written() {
    check(&[
        (
            "<fieldset><label><input type=radio name=g value=1>one</label>\
             <label><input type=radio name=g checked>two</fieldset>",
            "fieldset[label[input(type=radio name=g value=1) \"one\"] \
             label[input(type=radio name=g checked=) \"two\"]]",
        ),
        (
            "<ul><li><input type=checkbox>a<li><input type=checkbox checked>b</ul>",
            r#"ul[li[input(type=checkbox) "a"] li[input(type=checkbox checked=) "b"]]"#,
        ),
    ]);
}

#[test]
fn a_table_stays_in_a_paragraph_only_in_quirks_mode() {
    check(&[(
        "<p>a<table><tr><td>b</table>c",
        r#"p["a"] table[tbody[tr[td["b"]]]] "c""#,
    )]);
    assert_eq!(
        document("<p>a<table><tr><td>b</table>c"),
        r#"html[head body[p["a" table[tbody[tr[td["b"]]]] "c"]]]"#
    );
}

#[test]
fn foreign_content_ends_at_html() {
    check(&[
        (
            "<svg><g><div>a</div></g></svg>",
            r#"svg:svg[svg:g] div["a"]"#,
        ),
        ("<svg><p>a</svg>b", r#"svg:svg p["ab"]"#),
        ("<svg><font size=1>a", r#"svg:svg font(size=1)["a"]"#),
        ("<svg><font>a</font></svg>", r#"svg:svg[svg:font["a"]]"#),
        ("<svg><a/>b</svg>c", r#"svg:svg[svg:a "b"] "c""#),
        // End tags match whatever the case, and close what they name.
        (
            "<svg><foreignObject>a</FOREIGNobject><path/></svg>b",
            r#"svg:svg[svg:foreignobject["a"] svg:path] "b""#,
        ),
    ]);
}

#[test]
fn integration_points_hold_html() {
    check(&[
        (
            "<svg><title><p>a</p></title><g><desc><b>c</b></desc></g></svg>d",
            r#"svg:svg[svg:title[p["a"]] svg:g[svg:desc[b["c"]]]] "d""#,
        ),
        // An SVG title's content is markup, unlike an HTML title's.
        ("<svg><title>a</svg>b", r#"svg:svg[svg:title["a"]] "b""#),
        (
            "<svg><style><a>b</style></svg>c",
            r#"svg:svg[svg:style[svg:a["b"]]] "c""#,
        ),
        (
            "<math><mi><b>a</b></mi><annotation-xml encoding=text/html><div>b</div></annotation-xml></math>",
            r#"math:math[math:mi[b["a"]] math:annotation-xml(encoding=text/html)[div["b"]]]"#,
        ),
        (
            "<math><annotation-xml><svg><g></g></svg><div>",
            "math:math[math:annotation-xml[svg:svg[svg:g]]] div",
        ),
    ]);
}

#[test]
fn an_end_tag_does_not_close_an_integration_point() {
    for (root, point) in [
        ("svg", "desc"),
        ("svg", "title"),
        ("svg", "foreignobject"),
        ("math", "mi"),
        ("math", "mo"),
        ("math", "mn"),
        ("math", "ms"),
        ("math", "mtext"),
    ] {
        check(&[(
            &format!("<x-y><{root}><{point}></x-y>z"),
            &format!(r#"x-y[{root}:{root}[{root}:{point}["z"]]]"#),
        )]);
    }
    check(&[(
        "<x-y><math><annotation-xml encoding=text/html></x-y>z",
        r#"x-y[math:math[math:annotation-xml(encoding=text/html)["z"]]]"#,
    )]);
}

#[test]
fn cdata_is_a_section_only_in_foreign_content() {
    check(&[
        (
            "<svg><![CDATA[a<b>&amp;]]></svg><![CDATA[x]]>",
            r#"svg:svg["a<b>&amp;"] <!--[CDATA[x]]-->"#,
        ),
        ("<svg><![CDATA[a]]]]><g>", r#"svg:svg["a]]" svg:g]"#),
        ("<svg><![CDATA[a", r#"svg:svg["a"]"#),
        (
            "<svg><desc><![CDATA[a]]><p><![CDATA[b]]>",
            r#"svg:svg[svg:desc["a" p[<!--[CDATA[b]]-->]]]"#,
        ),
    ]);
}

#[test]
fn template_contents_parse_in_their_own_mode() {
    check(&[
        (
            "<p>a<template><tr><td>b</template>c",
            r#"p["a" template[tr[td["b"]]] "c"]"#,
        ),
        (
            "<table><template><tr><td>1</td></tr></template><tr><td>2</table>",
            r#"table[template[tr[td["1"]]] tbody[tr[td["2"]]]]"#,
        ),
        (
            "<b><template>a</b>b</template>c",
            r#"b[template["ab"] "c"]"#,
        ),
    ]);
}

#[test]
fn noscript_holds_markup_as_scripts_do_not_run() {
    check(&[(
        "<p>a<noscript><img src=x></noscript>",
        r#"p["a" noscript[img(src=x)]]"#,
    )]);
    assert_eq!(
        document("<!doctype html><noscript><link><p>a"),
        r#"html[head[noscript[link]] body[p["a"]]]"#
    );
}

#[test]
fn stray_structure_merges_into_the_document() {
    assert_eq!(
        document(
            "<!doctype html><html lang=a><body class=b>x<body class=c id=d><html lang=e dir=f>"
        ),
        r#"html(lang=a dir=f)[head body(class=b id=d)["x"]]"#
    );
    assert_eq!(
        document("<!doctype html>a</body>b<!--c--></html><!--d-->e"),
        r#"html[head body["ab" <!--c--> "e"]] <!--d-->"#
    );
    assert_eq!(
        document("<!doctype html><head><title>a</title></head><link rel=x><p>b"),
        r#"html[head[title["a"] link(rel=x)] body[p["b"]]]"#
    );
    assert_eq!(
        document("<!doctype html><frameset><frame></frameset><noframes>a"),
        r#"html[head frameset[frame] noframes["a"]]"#
    );
    assert_eq!(
        document("<!doctype html><b><frameset></frameset></html>x"),
        "html[head frameset]"
    );
}

#[test]
fn stray_tags_are_ignored_or_replaced() {
    check(&[
        ("<td>a</td><tr>b<col><caption>c", r#""abc""#),
        (
            "<image src=a>b</br>c</p>d",
            r#"img(src=a) "b" br "c" p "d""#,
        ),
        ("<h1>a<h2>b</h3>c", r#"h1["a"] h2["b"] "c""#),
        // A self-closing slash means nothing on an HTML element.
        ("<div/>a", r#"div["a"]"#),
    ]);
}

#[test]
fn text_elements_hold_text() {
    check(&[
        (
            "<plaintext>a</plaintext><b>",
            r#"plaintext["a</plaintext><b>"]"#,
        ),
        (
            "<xmp><b>a</xmp><iframe><b>c</iframe>d",
            r#"xmp["<b>a"] iframe["<b>c"] "d""#,
        ),
        // The end tag's name must be complete.
        (
            "<textarea>a</textareax></textarea>b",
            r#"textarea["a</textareax>"] "b""#,
        ),
        (
            "<p><script><!--<script>a</script>b--></script>c",
            r#"p[script["<!--<script>a</script>b-->"] "c"]"#,
        ),
    ]);
}

#[test]
fn odd_tokens() {
    check(&[
        (
            "a<!-->b<!--->c<!--d--!>e<!--f",
            r#""a" <!----> "b" <!----> "c" <!--d--> "e" <!--f-->"#,
        ),
        (
            "a</ x><!x><?y z>b</>c<>d< e",
            r#""a" <!-- x--> <!--x--> <!--?y z--> "bc<>d< e""#,
        ),
        // The first of a repeated attribute counts.
        (
            "<p a=1 A=2 b = 'c' =d e>x</p f=g>y",
            r#"p(a=1 b=c =d= e=)["x"] "y""#,
        ),
        // A tag cut off by the end isn't one.
        (r#"<p>a<b c=""#, r#"p["a"]"#),
        ("<p>a\r\nb\rc", r#"p["a\nb\nc"]"#),
    ]);
}

#[test]
fn a_fragment_parses_in_its_context() {
    assert_eq!(inner("tr", "<td>a<td>b"), r#"td["a"] td["b"]"#);
    assert_eq!(inner("div", "<td>a"), r#""a""#);
    assert_eq!(
        inner("select", "<option>a<option>b"),
        r#"option["a"] option["b"]"#
    );
    assert_eq!(inner("div", "<html><body class=x>a</body>b"), r#""ab""#);
    assert_eq!(inner("template", "<tr><td>a"), r#"tr[td["a"]]"#);
    assert_eq!(inner("colgroup", "<col>a<col>"), "col col");
}

#[test]
fn a_fragment_of_the_document_node_may_be_any_part() {
    let mut doc = noonoo_dom::Document::new();
    let root = doc.root();
    for (source, expected) in [
        ("<tr><td>a", r#"tr[td["a"]]"#),
        ("<option>a<option>b", r#"option["a"] option["b"]"#),
        ("<p>a<b>b", r#"p["a" b["b"]]"#),
        ("a", r#""a""#),
        (
            "<!doctype html><html><title>a</title><body>b",
            r#"title["a"] "b""#,
        ),
    ] {
        noonoo_html::set_inner_html(&mut doc, root, source).unwrap();
        assert_eq!(tree(&doc, root), expected);
    }
}

/// The text a fragment starts with stays apart from the text before it, and
/// nodes that were there keep theirs.
#[test]
fn inserted_text_does_not_change_the_text_before_it() {
    use noonoo_html::{Adjacent, insert_adjacent_html};
    let mut doc = noonoo_html::parse("<!doctype html><p>a");
    let p = doc.get_elements_by_tag_name("p")[0];
    let text = doc.tree().children(p)[0];
    let new = insert_adjacent_html(&mut doc, p, Adjacent::BeforeEnd, "b<b>c</b>d").unwrap();
    assert_eq!(new.len(), 3);
    assert_eq!(tree(&doc, p), r#""a" "b" b["c"] "d""#);
    assert_eq!(doc.node(text).unwrap().as_text(), Some("a"));
}
