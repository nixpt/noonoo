//! HTML5's implied end tags and implied table sections, for the stack-based
//! tree builder: the subset of the insertion modes real markup leans on.
//!
//! - A block-level start tag closes an open `<p>` in button scope, and a
//!   stray `</p>` makes an empty paragraph.
//! - `<li>`, `<dd>`/`<dt>` close the open item of their kind, unless a
//!   "special" element other than `<address>`, `<div>` or `<p>` is in the
//!   way (a nested list).
//! - A heading closes a heading that's still open; inside `<select>`,
//!   `<option>` and `<optgroup>` close the open option and group.
//! - A row directly in a `<table>` gets a `<tbody>`, and a cell a `<tr>`.

use noonoo_dom::{Document, NodeId};

/// Start tags that close an open `<p>` (in button scope).
fn closes_p(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "center"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "header"
            | "hgroup"
            | "hr"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "listing"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "ul"
            | "xmp"
            | "plaintext"
            | "li"
            | "dd"
            | "dt"
    ) || is_heading(name)
}

fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// Where a scope search stops (the spec's default scope, plus `button`).
fn bounds_button_scope(name: &str) -> bool {
    matches!(
        name,
        "applet"
            | "caption"
            | "html"
            | "table"
            | "td"
            | "th"
            | "marquee"
            | "object"
            | "template"
            | "button"
    )
}

/// The spec's "special" elements, which stop the search for an open
/// `<li>`/`<dd>`/`<dt>` (except `address`, `div` and `p`).
fn is_special(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "applet"
            | "area"
            | "article"
            | "aside"
            | "base"
            | "blockquote"
            | "body"
            | "br"
            | "button"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "embed"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "head"
            | "header"
            | "hgroup"
            | "hr"
            | "html"
            | "iframe"
            | "img"
            | "input"
            | "li"
            | "link"
            | "listing"
            | "main"
            | "marquee"
            | "menu"
            | "meta"
            | "nav"
            | "noscript"
            | "object"
            | "ol"
            | "p"
            | "param"
            | "plaintext"
            | "pre"
            | "script"
            | "search"
            | "section"
            | "select"
            | "source"
            | "style"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "textarea"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
            | "wbr"
            | "xmp"
    ) || is_heading(name)
}

fn name(doc: &Document, id: NodeId) -> &str {
    doc.element(id).map_or("", |element| element.name.as_str())
}

/// The open element stack: `stack[0]` is the node being built into, which
/// is never popped and bounds every search.
pub(crate) struct Open<'a> {
    pub stack: &'a mut Vec<NodeId>,
    pub doc: &'a Document,
}

impl Open<'_> {
    fn current(&self) -> &str {
        match self.stack.len() {
            0 | 1 => "",
            n => name(self.doc, self.stack[n - 1]),
        }
    }

    /// Where an open `<p>` sits, if one is in button scope.
    fn p_in_button_scope(&self) -> Option<usize> {
        for index in (1..self.stack.len()).rev() {
            let open = name(self.doc, self.stack[index]);
            if open == "p" {
                return Some(index);
            }
            if bounds_button_scope(open) {
                return None;
            }
        }
        None
    }

    /// Close the `<p>` at `index` and whatever is open inside it.
    fn close_at(&mut self, index: usize) {
        self.stack.truncate(index);
    }

    /// Whether an open `<p>` was closed.
    fn close_p(&mut self) -> bool {
        match self.p_in_button_scope() {
            Some(index) => {
                self.close_at(index);
                true
            }
            None => false,
        }
    }

    /// Close the open item `<li>` (or `<dd>`/`<dt>`) a new one replaces.
    fn close_item(&mut self, kinds: &[&str]) {
        for index in (1..self.stack.len()).rev() {
            let open = name(self.doc, self.stack[index]);
            if kinds.contains(&open) {
                self.close_at(index);
                return;
            }
            if is_special(open) && !matches!(open, "address" | "div" | "p") {
                return;
            }
        }
    }

    fn in_select(&self) -> bool {
        self.stack[1..]
            .iter()
            .any(|&id| name(self.doc, id) == "select")
    }

    /// Pop what a start tag `tag` implicitly closes. Inside `<select>` only
    /// options and groups close anything: the rest stays inside it.
    pub(crate) fn before_start_tag(&mut self, tag: &str) {
        if self.in_select() {
            if matches!(tag, "option" | "optgroup") && self.current() == "option" {
                self.stack.pop();
            }
            if tag == "optgroup" && self.current() == "optgroup" {
                self.stack.pop();
            }
            return;
        }
        match tag {
            "li" => self.close_item(&["li"]),
            "dd" | "dt" => self.close_item(&["dd", "dt"]),
            _ => {}
        }
        if closes_p(tag) {
            self.close_p();
        }
        if is_heading(tag) && is_heading(self.current()) {
            self.stack.pop();
        }
    }

    /// Handle `</p>`: whether there was an open paragraph to close. With
    /// none, the caller inserts an empty one, as the spec does.
    pub(crate) fn end_p(&mut self) -> bool {
        // Closing the `p` closes whatever is still open inside it.
        self.close_p()
    }

    /// The table section a start tag `tag` needs opened first, if the
    /// current node can't hold it: `tbody` for a row directly in a table,
    /// then `tr` for a cell directly in a table or a row group.
    pub(crate) fn implied_section(&self, tag: &str) -> Option<&'static str> {
        match (tag, self.current()) {
            ("tr" | "td" | "th", "table") => Some("tbody"),
            ("td" | "th", "tbody" | "thead" | "tfoot") => Some("tr"),
            _ => None,
        }
    }
}
