//! Tree construction (HTML §13.2.6): the insertion modes, over the stack of
//! open elements and the list of active formatting elements.
//!
//! The section names in the comments are the spec's. What differs:
//!
//! - A `<template>`'s contents are its children: the DOM has no separate
//!   fragment for them, and the parser none of the declarative shadow root
//!   and content patching steps.
//! - Scripts don't run, so `<noscript>` holds markup.
//! - Where the spec sets no limit, hostile markup can make a small source a
//!   huge tree or a long wait. The limits here are far past any real page:
//!   the tree's depth ([`MAX_TREE_DEPTH`]), how far a search of the stack
//!   looks ([`MAX_SEARCH`]), and how many formatting elements stay active
//!   ([`MAX_FORMATTING`]) and how much reopening them may copy
//!   ([`COPY_ALLOWANCE`]). Past one, the misnested markup is closed or
//!   ignored instead.
//!
//! Nodes are created in the order the source names them (the copies and
//! implied elements when they are needed), so a source always gives the
//! same [`NodeId`]s.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use noonoo_dom::{Document, Namespace, NodeData, NodeId, QuirksMode};

use crate::tags::Tag;
use crate::tokenizer::{Content, Tokenizer};
use crate::{MAX_TREE_DEPTH, Token, entities, foreign, mode};

/// How many open elements a search looks through. Deeper ones count as not
/// found: without a bound, deep nesting makes each tag cost its depth.
const MAX_SEARCH: usize = 4096;

/// How many formatting elements stay active after a marker. The earliest
/// drop out, as they do past three alike (the "Noah's Ark clause").
const MAX_FORMATTING: usize = 512;

/// How many bytes of elements reopening formatting elements may copy, on
/// top of [`COPY_FACTOR`] per source byte: each unclosed formatting element
/// is copied into every block after it, which is quadratic.
const COPY_ALLOWANCE: usize = 16 << 20;

/// See [`COPY_ALLOWANCE`].
const COPY_FACTOR: usize = 16;

/// What an element or attribute counts as, beside its strings.
const COPY_NODE: usize = 64;

/// How many attributes a tag has before telling repeats apart takes a set.
const MANY_ATTRIBUTES: usize = 32;

/// Past this many attributes, repeated `<html>` and `<body>` tags add none.
const MAX_MERGED: usize = 1024;

/// The insertion modes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    InHeadNoscript,
    AfterHead,
    InBody,
    Text,
    InTable,
    InTableText,
    InCaption,
    InColumnGroup,
    InTableBody,
    InRow,
    InCell,
    InTemplate,
    AfterBody,
    InFrameset,
    AfterFrameset,
    AfterAfterBody,
    AfterAfterFrameset,
}

/// An open element.
#[derive(Clone, Copy)]
struct Open {
    id: NodeId,
    tag: Tag,
}

/// An entry of the list of active formatting elements.
#[derive(Clone, Copy)]
enum Active {
    Marker,
    Element {
        id: NodeId,
        tag: Tag,
        /// Its attributes, whatever their order.
        attrs: u64,
    },
}

/// A start tag.
struct Start {
    tag: Tag,
    name: String,
    attrs: Vec<(String, Option<String>)>,
    self_closing: bool,
}

/// A token as the rules see it.
enum Tok {
    Start(Start),
    End(Tag, String),
    Text(String),
    Comment(String),
    Doctype {
        name: String,
        public_id: Option<String>,
        system_id: Option<String>,
    },
    Eof,
}

/// What the rules did with a token.
enum Did {
    /// It is dealt with.
    Done,
    /// Process it again, in the mode it switched to.
    Again,
}

/// Build `source` as a whole document into the empty `doc`.
pub(crate) fn document(doc: &mut Document, source: &str) {
    let mut builder = Builder::new(doc, source);
    builder.run();
    builder.limit_depth();
}

/// Build `source` as a fragment in the context of `base`, whose children
/// the nodes join at the end. `base` is never closed.
pub(crate) fn fragment(doc: &mut Document, base: NodeId, context: Context, source: &str) {
    let context = match (context, doc.element(base)) {
        (Context::Body, _) => Tag::Body,
        (Context::Node, Some(element)) => Tag::of_element(element),
        // No element to go by: allow what a template does.
        (Context::Node, None) => Tag::Template,
    };
    // The form the fragment is in, whose controls it adds to.
    let form = std::iter::once(base)
        .chain(doc.tree().ancestors(base))
        .find(|&id| {
            doc.element(id)
                .is_some_and(|e| e.namespace == Namespace::Html && e.name == "form")
        });
    let mut builder = Builder::new(doc, source);
    builder.form = form;
    builder.context = Some(context);
    // The base stands in for the root `html` element of the spec's fragment
    // parser.
    builder.push(Open {
        id: base,
        tag: Tag::Html,
    });
    if context == Tag::Template {
        builder.templates.push(Mode::InTemplate);
    }
    builder.reset_mode();
    builder.run();
    builder.limit_depth();
}

/// What a fragment parses in the context of.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context {
    /// The node it is put in.
    Node,
    /// `<body>`, whatever the node.
    Body,
}

struct Builder<'d, 's> {
    doc: &'d mut Document,
    tokenizer: Tokenizer<'s>,
    mode: Mode,
    /// Where "text" and "in table text" return to.
    original: Mode,
    /// The stack of template insertion modes.
    templates: Vec<Mode>,
    /// The stack of open elements. Never empty once the `html` element is
    /// in; the first is never popped.
    open: Vec<Open>,
    /// Whether each node made here (by index after `first_new`) is open.
    is_open: Vec<bool>,
    active: Vec<Active>,
    head: Option<NodeId>,
    form: Option<NodeId>,
    /// The fragment's context element, for a fragment.
    context: Option<Tag>,
    frameset_ok: bool,
    foster: bool,
    /// HTML drops a newline straight after a `<pre>`, `<listing>` or
    /// `<textarea>` start tag.
    drop_newline: bool,
    /// Characters seen in "in table text".
    table_text: String,
    /// The index of the first node made here.
    first_new: usize,
    /// Bytes of elements that may still be copied.
    copies: usize,
    /// Whether the tree may be deeper than the stack is long, whose length
    /// is what the depth limit goes by: an element left the stack's middle,
    /// or nodes moved.
    moved: bool,
}

impl<'d, 's> Builder<'d, 's> {
    fn new(doc: &'d mut Document, source: &'s str) -> Self {
        let first_new = doc.tree().upcoming_id(0).index();
        Self {
            doc,
            tokenizer: Tokenizer::new(source),
            mode: Mode::Initial,
            original: Mode::Initial,
            templates: Vec::new(),
            open: Vec::new(),
            is_open: Vec::new(),
            active: Vec::new(),
            head: None,
            form: None,
            context: None,
            frameset_ok: true,
            foster: false,
            drop_newline: false,
            table_text: String::new(),
            first_new,
            copies: COPY_ALLOWANCE + COPY_FACTOR * source.len(),
            moved: false,
        }
    }

    fn run(&mut self) {
        loop {
            let cdata = self.adjusted_current().is_some_and(|tag| !tag.is_html());
            let token = match self.tokenizer.next(cdata) {
                Some(token) => token,
                None => {
                    self.process(Tok::Eof);
                    return;
                }
            };
            let drop_newline = std::mem::take(&mut self.drop_newline);
            let token = match token {
                Token::Text(mut text) => {
                    if drop_newline && text.starts_with('\n') {
                        text.remove(0);
                    }
                    if text.is_empty() {
                        continue;
                    }
                    Tok::Text(text)
                }
                Token::StartTag {
                    name,
                    attrs,
                    self_closing,
                } => Tok::Start(Start {
                    tag: Tag::of(&name),
                    name,
                    attrs,
                    self_closing,
                }),
                Token::EndTag { name } => Tok::End(Tag::of(&name), name),
                Token::Comment(text) => Tok::Comment(text),
                Token::Doctype {
                    name,
                    public_id,
                    system_id,
                } => Tok::Doctype {
                    name,
                    public_id,
                    system_id,
                },
            };
            self.process(token);
        }
    }

    /// The tree construction dispatcher.
    fn process(&mut self, mut token: Tok) {
        loop {
            let did = if self.is_foreign(&token) {
                self.in_foreign(&mut token)
            } else {
                self.by_mode(&mut token)
            };
            if let Did::Done = did {
                return;
            }
        }
    }

    fn by_mode(&mut self, token: &mut Tok) -> Did {
        match self.mode {
            Mode::Initial => self.initial(token),
            Mode::BeforeHtml => self.before_html(token),
            Mode::BeforeHead => self.before_head(token),
            Mode::InHead => self.in_head(token),
            Mode::InHeadNoscript => self.in_head_noscript(token),
            Mode::AfterHead => self.after_head(token),
            Mode::InBody => self.in_body(token),
            Mode::Text => self.text(token),
            Mode::InTable => self.in_table(token),
            Mode::InTableText => self.in_table_text(token),
            Mode::InCaption => self.in_caption(token),
            Mode::InColumnGroup => self.in_column_group(token),
            Mode::InTableBody => self.in_table_body(token),
            Mode::InRow => self.in_row(token),
            Mode::InCell => self.in_cell(token),
            Mode::InTemplate => self.in_template(token),
            Mode::AfterBody => self.after_body(token),
            Mode::InFrameset => self.in_frameset(token),
            Mode::AfterFrameset => self.after_frameset(token),
            Mode::AfterAfterBody => self.after_after_body(token),
            Mode::AfterAfterFrameset => self.after_after_frameset(token),
        }
    }

    // --- the stack of open elements ------------------------------------

    fn current(&self) -> Tag {
        self.open.last().map_or(Tag::Other, |open| open.tag)
    }

    fn current_id(&self) -> NodeId {
        self.open.last().map_or(self.doc.root(), |open| open.id)
    }

    /// The adjusted current node: a fragment's context, at its top level.
    fn adjusted_current(&self) -> Option<Tag> {
        match self.context {
            Some(context) if self.open.len() == 1 => Some(context),
            _ => self.open.last().map(|open| open.tag),
        }
    }

    fn push(&mut self, open: Open) {
        self.set_open(open.id, true);
        self.open.push(open);
    }

    fn set_open(&mut self, id: NodeId, open: bool) {
        // A node from before the parse (a fragment's base) isn't tracked.
        let Some(index) = id.index().checked_sub(self.first_new) else {
            return;
        };
        if index >= self.is_open.len() {
            self.is_open.resize(index + 1, false);
        }
        self.is_open[index] = open;
    }

    fn is_open(&self, id: NodeId) -> bool {
        id.index()
            .checked_sub(self.first_new)
            .and_then(|index| self.is_open.get(index).copied())
            .unwrap_or(false)
    }

    /// Pop the elements at `index` and after it. The first never pops.
    fn close_from(&mut self, index: usize) {
        while self.open.len() > index.max(1) {
            if let Some(open) = self.open.pop() {
                self.set_open(open.id, false);
            }
        }
    }

    fn pop(&mut self) {
        self.close_from(self.open.len().saturating_sub(1));
    }

    /// Remove `id` from anywhere in the stack.
    fn remove_open(&mut self, id: NodeId) {
        if let Some(index) = self.find(|open| open.id == id, |_| false)
            && index > 0
        {
            // What is inside it stays open: the stack gets shorter than the
            // tree is deep.
            self.moved |= index + 1 < self.open.len();
            self.open.remove(index);
            self.set_open(id, false);
        }
    }

    /// The topmost open element `want` accepts, unless one `stop` accepts is
    /// above it.
    fn find(&self, want: impl Fn(&Open) -> bool, stop: impl Fn(Tag) -> bool) -> Option<usize> {
        for (index, open) in self.open.iter().enumerate().rev().take(MAX_SEARCH) {
            if want(open) {
                return Some(index);
            }
            if stop(open.tag) {
                return None;
            }
        }
        None
    }

    fn in_scope(&self, tag: Tag) -> Option<usize> {
        self.find(|open| open.tag == tag, Tag::bounds_scope)
    }

    fn in_table_scope(&self, tag: Tag) -> Option<usize> {
        self.find(|open| open.tag == tag, Tag::bounds_table_scope)
    }

    /// Whether `tag` is anywhere on the stack.
    fn has_open(&self, tag: Tag) -> bool {
        self.find(|open| open.tag == tag, |_| false).is_some()
    }

    /// Whether the parser is parsing template contents.
    fn in_template_contents(&self) -> bool {
        !self.templates.is_empty()
    }

    /// Whether the open element is the HTML element `name`, interned `tag`.
    fn is_named(&self, open: &Open, tag: Tag, name: &str) -> bool {
        open.tag == tag
            && (tag != Tag::Other || self.doc.element(open.id).is_some_and(|e| e.name == name))
    }

    /// Generate implied end tags, except for `except`.
    fn implied_ends(&mut self, except: Tag) {
        while self.current().has_implied_end() && self.current() != except {
            self.pop();
        }
    }

    /// Close the `<p>` in button scope, if there is one.
    fn close_p(&mut self) -> bool {
        match self.find(|open| open.tag == Tag::P, Tag::bounds_button_scope) {
            Some(index) => {
                self.close_from(index);
                true
            }
            None => false,
        }
    }

    /// An end tag that closes the element of its kind in scope, if any.
    fn close_in_scope(&mut self, tag: Tag) -> bool {
        match self.in_scope(tag) {
            Some(index) => {
                self.close_from(index);
                true
            }
            None => false,
        }
    }

    /// Clear the stack back to a table, table body or table row context.
    fn clear_to(&mut self, context: &[Tag]) {
        while !context.contains(&self.current())
            && !matches!(self.current(), Tag::Template | Tag::Html)
        {
            self.pop();
        }
    }

    /// Reset the insertion mode appropriately.
    fn reset_mode(&mut self) {
        for index in (0..self.open.len()).rev() {
            let last = index == 0;
            let tag = match self.context {
                Some(context) if last => context,
                _ => self.open[index].tag,
            };
            self.mode = match tag {
                Tag::Td | Tag::Th if !last => Mode::InCell,
                Tag::Tr => Mode::InRow,
                Tag::Tbody | Tag::Thead | Tag::Tfoot => Mode::InTableBody,
                Tag::Caption => Mode::InCaption,
                Tag::Colgroup => Mode::InColumnGroup,
                Tag::Table => Mode::InTable,
                Tag::Template => self.templates.last().copied().unwrap_or(Mode::InBody),
                Tag::Head if !last => Mode::InHead,
                Tag::Body => Mode::InBody,
                Tag::Frameset => Mode::InFrameset,
                Tag::Html if self.head.is_none() => Mode::BeforeHead,
                Tag::Html => Mode::AfterHead,
                _ if last => Mode::InBody,
                _ => continue,
            };
            return;
        }
    }

    // --- the list of active formatting elements ------------------------

    /// The entries after the last marker, last first, with their indices.
    fn active_elements(&self) -> impl Iterator<Item = (usize, NodeId, Tag, u64)> + '_ {
        self.active
            .iter()
            .enumerate()
            .rev()
            .map_while(|(index, entry)| match *entry {
                Active::Marker => None,
                Active::Element { id, tag, attrs } => Some((index, id, tag, attrs)),
            })
    }

    fn active_index(&self, id: NodeId) -> Option<usize> {
        self.active_elements()
            .find(|&(_, active, ..)| active == id)
            .map(|(index, ..)| index)
    }

    fn remove_active(&mut self, id: NodeId) {
        if let Some(index) = self.active_index(id) {
            self.active.remove(index);
        }
    }

    /// Push onto the list of active formatting elements.
    fn push_active(&mut self, id: NodeId, tag: Tag) {
        let attrs = self.attrs_hash(id);
        let (mut alike, mut total) = (0, 0);
        let mut drop = None;
        for (index, _, other, others) in self.active_elements() {
            total += 1;
            if other == tag && others == attrs {
                alike += 1;
                if alike == 3 {
                    drop = Some(index);
                    break;
                }
            }
            if total == MAX_FORMATTING {
                drop = Some(index);
                break;
            }
        }
        if let Some(index) = drop {
            self.active.remove(index);
        }
        self.active.push(Active::Element { id, tag, attrs });
    }

    /// A hash of an element's attributes that their order doesn't change.
    fn attrs_hash(&self, id: NodeId) -> u64 {
        let Some(element) = self.doc.element(id) else {
            return 0;
        };
        element.attrs.iter().fold(0, |sum: u64, attr| {
            let mut hasher = DefaultHasher::new();
            attr.name.hash(&mut hasher);
            attr.value.hash(&mut hasher);
            sum.wrapping_add(hasher.finish())
        })
    }

    /// Clear the list up to the last marker.
    fn clear_active(&mut self) {
        while let Some(Active::Element { .. }) = self.active.pop() {}
    }

    /// A new element for the token `id` was created for, if the allowance
    /// for copies isn't used up.
    fn copy(&mut self, id: NodeId) -> Option<NodeId> {
        let element = self.doc.element(id)?;
        let cost = element.attrs.iter().fold(COPY_NODE, |cost, attr| {
            cost + COPY_NODE + attr.name.len() + attr.value.len()
        });
        self.copies = self.copies.checked_sub(cost)?;
        self.doc.clone_node(id, false).ok()
    }

    /// Reconstruct the active formatting elements.
    fn reconstruct(&mut self) {
        let mut first = self.active.len();
        for (index, id, ..) in self.active_elements() {
            if self.is_open(id) {
                break;
            }
            first = index;
        }
        for index in first..self.active.len() {
            let Active::Element { id, tag, attrs } = self.active[index] else {
                return;
            };
            let Some(new) = self.copy(id) else { return };
            self.insert(new);
            self.push(Open { id: new, tag });
            self.active[index] = Active::Element {
                id: new,
                tag,
                attrs,
            };
        }
    }

    // --- creating and inserting nodes -----------------------------------

    /// The appropriate place for inserting a node: the parent, and the node
    /// it goes before (its end, without one).
    fn place(&self) -> (NodeId, Option<NodeId>) {
        let Some(&target) = self.open.last() else {
            return (self.doc.root(), None);
        };
        if self.foster && target.tag.fosters() {
            return self.foster_place();
        }
        (self.parent_at(self.open.len() - 1), None)
    }

    /// The node that what goes in the open element at `index` is put in:
    /// that element, or past the depth limit its parent.
    fn parent_at(&self, index: usize) -> NodeId {
        let id = self.open[index].id;
        let limit = match self.context {
            Some(_) => MAX_TREE_DEPTH,
            None => MAX_TREE_DEPTH - 1,
        };
        if index >= limit {
            return self.doc.tree().parent(id).unwrap_or(id);
        }
        id
    }

    /// Where content misnested in a table goes: before the table.
    fn foster_place(&self) -> (NodeId, Option<NodeId>) {
        let found = self.find(
            |open| matches!(open.tag, Tag::Template | Tag::Table),
            |_| false,
        );
        let Some(index) = found else {
            return (self.open[0].id, None);
        };
        let open = self.open[index];
        if open.tag == Tag::Template {
            return (self.parent_at(index), None);
        }
        match self.doc.tree().parent(open.id) {
            Some(parent) => (parent, Some(open.id)),
            None => (self.parent_at(index.saturating_sub(1)), None),
        }
    }

    /// Put `node` at the appropriate place.
    fn insert(&mut self, node: NodeId) {
        let place = self.place();
        self.insert_at(place, node);
    }

    /// Put `node` at `place`: whether it could be.
    fn insert_at(&mut self, place: (NodeId, Option<NodeId>), node: NodeId) -> bool {
        match place {
            (_, Some(before)) => self.doc.mutate().insert_before(before, node),
            (parent, None) => self.doc.mutate().append_child(parent, node),
        }
        .is_ok()
    }

    /// Insert `text`, joining the text node it lands after.
    fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let (parent, before) = self.place();
        let siblings = self.doc.tree().children(parent);
        let previous = match before {
            None => siblings.last().copied(),
            Some(before) => siblings
                .iter()
                .rposition(|&id| id == before)
                .and_then(|index| index.checked_sub(1))
                .map(|index| siblings[index]),
        };
        // A text node from before this parse isn't the parser's to extend.
        let joins = previous.filter(|&id| {
            id.index() >= self.first_new && matches!(self.doc.node(id), Some(NodeData::Text(_)))
        });
        match joins {
            Some(node) => {
                let _ = self.doc.mutate().append_text(node, text);
            }
            None => {
                let node = self.doc.create_text(text);
                self.insert_at((parent, before), node);
            }
        }
    }

    fn insert_comment(&mut self, text: &mut String) {
        let node = self.doc.create_comment(std::mem::take(text));
        self.insert(node);
    }

    fn insert_comment_in(&mut self, parent: NodeId, text: &mut String) {
        let node = self.doc.create_comment(std::mem::take(text));
        let _ = self.doc.mutate().append_child(parent, node);
    }

    /// A new element for a start tag, with its attributes; the first of a
    /// repeated name counts.
    fn create(&mut self, start: &mut Start, namespace: Namespace) -> NodeId {
        let node = self
            .doc
            .create_element_ns(std::mem::take(&mut start.name), namespace);
        self.add_attrs(node, &mut start.attrs);
        node
    }

    /// Add the attributes `node` doesn't have yet.
    fn add_attrs(&mut self, node: NodeId, attrs: &mut Vec<(String, Option<String>)>) {
        let Some(element) = self.doc.element(node) else {
            return;
        };
        if element.attrs.len() > MAX_MERGED {
            return;
        }
        let namespace = element.namespace;
        // Looking each up in the list is quicker, unless they are many.
        let mut seen: Option<HashSet<String>> = (attrs.len() > MANY_ATTRIBUTES)
            .then(|| element.attrs.iter().map(|a| a.name.clone()).collect());
        for (name, value) in std::mem::take(attrs) {
            let name = foreign::adjust_attribute(namespace, &name);
            let new = match &mut seen {
                Some(seen) => seen.insert(name.to_owned()),
                None => self
                    .doc
                    .element(node)
                    .is_some_and(|e| e.attr(name).is_none()),
            };
            if new {
                let value = value.map(|value| entities::decode_attribute(&value));
                let value = value.unwrap_or_default();
                let _ = self.doc.mutate().set_attribute(node, name, value);
            }
        }
    }

    /// Insert an HTML element for a start tag.
    fn insert_element(&mut self, start: &mut Start) -> NodeId {
        let tag = start.tag;
        let node = self.create(start, Namespace::Html);
        self.insert(node);
        self.push(Open { id: node, tag });
        node
    }

    /// Insert an HTML element for a start tag that has no content.
    fn insert_void(&mut self, start: &mut Start) {
        let node = self.create(start, Namespace::Html);
        self.insert(node);
    }

    /// Insert an HTML element for an implied start tag.
    fn insert_implied(&mut self, name: &str) -> NodeId {
        self.insert_element(&mut Start {
            tag: Tag::of(name),
            name: name.to_owned(),
            attrs: Vec::new(),
            self_closing: false,
        })
    }

    /// Insert a foreign element for a start tag.
    fn insert_foreign(&mut self, start: &mut Start, namespace: Namespace) {
        let self_closing = start.self_closing;
        let node = self.create(start, namespace);
        self.insert(node);
        if !self_closing {
            let tag = self.doc.element(node).map_or(Tag::Svg, Tag::of_element);
            self.push(Open { id: node, tag });
        }
    }

    /// Insert an element whose content is text: the generic raw text and
    /// RCDATA parsing algorithms.
    fn insert_text_element(&mut self, start: &mut Start) {
        self.tokenizer.switch_to(Content::of(&start.name));
        self.insert_element(start);
        self.original = self.mode;
        self.mode = Mode::Text;
    }

    /// Insert a formatting element.
    fn insert_formatting(&mut self, start: &mut Start) {
        self.reconstruct();
        let tag = start.tag;
        let node = self.insert_element(start);
        self.push_active(node, tag);
    }

    /// A `<form>` start tag: whether it opens one here.
    fn may_open_form(&self) -> bool {
        self.form.is_none() || self.in_template_contents()
    }

    fn insert_form(&mut self, start: &mut Start) {
        let node = self.insert_element(start);
        if !self.in_template_contents() {
            self.form = Some(node);
        }
    }

    // --- the insertion modes --------------------------------------------

    fn initial(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => {
                if skip_blank(text) {
                    return Did::Done;
                }
            }
            Tok::Comment(text) => {
                let root = self.doc.root();
                self.insert_comment_in(root, text);
                return Did::Done;
            }
            Tok::Doctype {
                name,
                public_id,
                system_id,
            } => {
                let quirks = mode::of_doctype(name, public_id.as_deref(), system_id.as_deref());
                self.doc.set_quirks_mode(quirks);
                let name = name.to_ascii_lowercase();
                let node = self.doc.tree_mut().insert_raw(NodeData::Doctype { name });
                let root = self.doc.root();
                let _ = self.doc.mutate().append_child(root, node);
                self.mode = Mode::BeforeHtml;
                return Did::Done;
            }
            _ => {}
        }
        self.doc.set_quirks_mode(QuirksMode::Quirks);
        self.mode = Mode::BeforeHtml;
        Did::Again
    }

    fn before_html(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Doctype { .. } => return Did::Done,
            Tok::Comment(text) => {
                let root = self.doc.root();
                self.insert_comment_in(root, text);
                return Did::Done;
            }
            Tok::Text(text) => {
                if skip_blank(text) {
                    return Did::Done;
                }
            }
            Tok::Start(start) if start.tag == Tag::Html => {
                self.insert_element(start);
                self.mode = Mode::BeforeHead;
                return Did::Done;
            }
            Tok::End(tag, _) if !matches!(tag, Tag::Head | Tag::Body | Tag::Html | Tag::Br) => {
                return Did::Done;
            }
            _ => {}
        }
        self.insert_implied("html");
        self.mode = Mode::BeforeHead;
        Did::Again
    }

    fn before_head(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => {
                if skip_blank(text) {
                    return Did::Done;
                }
            }
            Tok::Comment(text) => {
                self.insert_comment(text);
                return Did::Done;
            }
            Tok::Doctype { .. } => return Did::Done,
            Tok::Start(start) if start.tag == Tag::Html => return self.in_body(token),
            Tok::Start(start) if start.tag == Tag::Head => {
                self.head = Some(self.insert_element(start));
                self.mode = Mode::InHead;
                return Did::Done;
            }
            Tok::End(tag, _) if !matches!(tag, Tag::Head | Tag::Body | Tag::Html | Tag::Br) => {
                return Did::Done;
            }
            _ => {}
        }
        self.head = Some(self.insert_implied("head"));
        self.mode = Mode::InHead;
        Did::Again
    }

    fn in_head(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => {
                if self.take_blank(text) {
                    return Did::Done;
                }
            }
            Tok::Comment(text) => {
                self.insert_comment(text);
                return Did::Done;
            }
            Tok::Doctype { .. } => return Did::Done,
            Tok::Start(start) => match start.tag {
                Tag::Html => return self.in_body(token),
                Tag::Base | Tag::Basefont | Tag::Bgsound | Tag::Link | Tag::Meta => {
                    self.insert_void(start);
                    return Did::Done;
                }
                Tag::Title | Tag::Noframes | Tag::Style | Tag::Script => {
                    self.insert_text_element(start);
                    return Did::Done;
                }
                Tag::Noscript => {
                    self.insert_element(start);
                    self.mode = Mode::InHeadNoscript;
                    return Did::Done;
                }
                Tag::Template => {
                    self.insert_element(start);
                    self.active.push(Active::Marker);
                    self.frameset_ok = false;
                    self.mode = Mode::InTemplate;
                    self.templates.push(Mode::InTemplate);
                    return Did::Done;
                }
                Tag::Head => return Did::Done,
                _ => {}
            },
            Tok::End(tag, _) => match tag {
                Tag::Head => {
                    self.pop();
                    self.mode = Mode::AfterHead;
                    return Did::Done;
                }
                Tag::Template => {
                    self.end_template();
                    return Did::Done;
                }
                Tag::Body | Tag::Html | Tag::Br => {}
                _ => return Did::Done,
            },
            Tok::Eof => {}
        }
        self.pop();
        self.mode = Mode::AfterHead;
        Did::Again
    }

    /// Close the open template, if there is one.
    fn end_template(&mut self) {
        let Some(index) = self.find(|open| open.tag == Tag::Template, |_| false) else {
            return;
        };
        self.close_from(index);
        self.clear_active();
        self.templates.pop();
        self.reset_mode();
    }

    fn in_head_noscript(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Doctype { .. } => return Did::Done,
            Tok::Comment(_) => return self.in_head(token),
            Tok::Text(text) => {
                if self.take_blank(text) {
                    return Did::Done;
                }
            }
            Tok::Start(start) => match start.tag {
                Tag::Html => return self.in_body(token),
                Tag::Basefont
                | Tag::Bgsound
                | Tag::Link
                | Tag::Meta
                | Tag::Noframes
                | Tag::Style => return self.in_head(token),
                Tag::Head | Tag::Noscript => return Did::Done,
                _ => {}
            },
            Tok::End(Tag::Noscript, _) => {
                self.pop();
                self.mode = Mode::InHead;
                return Did::Done;
            }
            Tok::End(Tag::Br, _) | Tok::Eof => {}
            Tok::End(..) => return Did::Done,
        }
        self.pop();
        self.mode = Mode::InHead;
        Did::Again
    }

    fn after_head(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => {
                if self.take_blank(text) {
                    return Did::Done;
                }
            }
            Tok::Comment(text) => {
                self.insert_comment(text);
                return Did::Done;
            }
            Tok::Doctype { .. } => return Did::Done,
            Tok::Start(start) => match start.tag {
                Tag::Html => return self.in_body(token),
                Tag::Body => {
                    self.insert_element(start);
                    self.frameset_ok = false;
                    self.mode = Mode::InBody;
                    return Did::Done;
                }
                Tag::Frameset => {
                    self.insert_element(start);
                    self.mode = Mode::InFrameset;
                    return Did::Done;
                }
                Tag::Base
                | Tag::Basefont
                | Tag::Bgsound
                | Tag::Link
                | Tag::Meta
                | Tag::Noframes
                | Tag::Script
                | Tag::Style
                | Tag::Template
                | Tag::Title => {
                    // Back into the head for this one.
                    let Some(head) = self.head else {
                        return Did::Done;
                    };
                    self.push(Open {
                        id: head,
                        tag: Tag::Head,
                    });
                    let did = self.in_head(token);
                    self.remove_open(head);
                    return did;
                }
                Tag::Head => return Did::Done,
                _ => {}
            },
            Tok::End(tag, _) => match tag {
                Tag::Template => return self.in_head(token),
                Tag::Body | Tag::Html | Tag::Br => {}
                _ => return Did::Done,
            },
            Tok::Eof => {}
        }
        self.insert_implied("body");
        self.frameset_ok = true;
        self.mode = Mode::InBody;
        Did::Again
    }

    fn in_body(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => {
                if text.contains('\0') {
                    text.retain(|c| c != '\0');
                    if text.is_empty() {
                        return Did::Done;
                    }
                }
                self.reconstruct();
                self.insert_text(text);
                if !is_blank(text) {
                    self.frameset_ok = false;
                }
                Did::Done
            }
            Tok::Comment(text) => {
                self.insert_comment(text);
                Did::Done
            }
            Tok::Doctype { .. } => Did::Done,
            Tok::Start(start)
                if matches!(
                    start.tag,
                    Tag::Base
                        | Tag::Basefont
                        | Tag::Bgsound
                        | Tag::Link
                        | Tag::Meta
                        | Tag::Noframes
                        | Tag::Script
                        | Tag::Style
                        | Tag::Template
                        | Tag::Title
                ) =>
            {
                self.in_head(token)
            }
            Tok::End(Tag::Template, _) => self.in_head(token),
            Tok::Start(start) => self.in_body_start(start),
            Tok::End(tag, name) => {
                let tag = *tag;
                self.in_body_end(tag, name)
            }
            Tok::Eof => {
                if self.templates.is_empty() {
                    Did::Done
                } else {
                    self.in_template(token)
                }
            }
        }
    }

    fn in_body_start(&mut self, start: &mut Start) -> Did {
        match start.tag {
            Tag::Html => {
                // A fragment's root stands in for an element that is thrown
                // away.
                if self.context.is_none() && !self.in_template_contents() {
                    let html = self.open[0].id;
                    self.add_attrs(html, &mut start.attrs);
                }
            }
            Tag::Body => {
                if let Some(body) = self.second_is_body()
                    && !self.in_template_contents()
                {
                    self.frameset_ok = false;
                    self.add_attrs(body, &mut start.attrs);
                }
            }
            Tag::Frameset => {
                if let Some(body) = self.second_is_body()
                    && self.frameset_ok
                {
                    let _ = self.doc.mutate().remove(body);
                    self.close_from(1);
                    self.insert_element(start);
                    self.mode = Mode::InFrameset;
                }
            }
            Tag::Address
            | Tag::Article
            | Tag::Aside
            | Tag::Blockquote
            | Tag::Center
            | Tag::Details
            | Tag::Dialog
            | Tag::Dir
            | Tag::Div
            | Tag::Dl
            | Tag::Fieldset
            | Tag::Figcaption
            | Tag::Figure
            | Tag::Footer
            | Tag::Header
            | Tag::Hgroup
            | Tag::Main
            | Tag::Menu
            | Tag::Nav
            | Tag::Ol
            | Tag::P
            | Tag::Search
            | Tag::Section
            | Tag::Summary
            | Tag::Ul => {
                self.close_p();
                self.insert_element(start);
            }
            tag if tag.is_heading() => {
                self.close_p();
                if self.current().is_heading() {
                    self.pop();
                }
                self.insert_element(start);
            }
            Tag::Pre | Tag::Listing => {
                self.close_p();
                self.insert_element(start);
                self.drop_newline = true;
                self.frameset_ok = false;
            }
            Tag::Form => {
                if self.may_open_form() {
                    self.close_p();
                    self.insert_form(start);
                }
            }
            Tag::Li => {
                self.frameset_ok = false;
                self.close_item(&[Tag::Li]);
                self.close_p();
                self.insert_element(start);
            }
            Tag::Dd | Tag::Dt => {
                self.frameset_ok = false;
                self.close_item(&[Tag::Dd, Tag::Dt]);
                self.close_p();
                self.insert_element(start);
            }
            Tag::Plaintext => {
                self.close_p();
                self.insert_element(start);
                self.tokenizer.switch_to(Content::Plaintext);
            }
            Tag::Button => {
                self.close_in_scope(Tag::Button);
                self.reconstruct();
                self.insert_element(start);
                self.frameset_ok = false;
            }
            Tag::A => {
                let existing = self
                    .active_elements()
                    .find(|&(_, _, tag, _)| tag == Tag::A)
                    .map(|(_, id, ..)| id);
                if let Some(existing) = existing {
                    self.adopt(Tag::A, "a");
                    self.remove_active(existing);
                    if self.is_open(existing) {
                        self.remove_open(existing);
                    }
                }
                self.insert_formatting(start);
            }
            Tag::Nobr => {
                self.reconstruct();
                if self.in_scope(Tag::Nobr).is_some() {
                    self.adopt(Tag::Nobr, "nobr");
                }
                self.insert_formatting(start);
            }
            tag if tag.is_formatting() => self.insert_formatting(start),
            Tag::Applet | Tag::Marquee | Tag::Object => {
                self.reconstruct();
                self.insert_element(start);
                self.active.push(Active::Marker);
                self.frameset_ok = false;
            }
            Tag::Table => {
                // In quirks mode a table stays in the paragraph.
                if self.doc.quirks_mode() != QuirksMode::Quirks {
                    self.close_p();
                }
                self.insert_element(start);
                self.frameset_ok = false;
                self.mode = Mode::InTable;
            }
            Tag::Image => {
                start.tag = Tag::Img;
                start.name = "img".to_owned();
                return self.in_body_start(start);
            }
            Tag::Area | Tag::Br | Tag::Embed | Tag::Img | Tag::Keygen | Tag::Wbr => {
                self.reconstruct();
                self.insert_void(start);
                self.frameset_ok = false;
            }
            Tag::Input => {
                if self.context == Some(Tag::Select) {
                    return Did::Done;
                }
                self.close_in_scope(Tag::Select);
                self.reconstruct();
                let hidden = is_hidden(start);
                self.insert_void(start);
                if !hidden {
                    self.frameset_ok = false;
                }
            }
            Tag::Param | Tag::Source | Tag::Track => self.insert_void(start),
            Tag::Hr => {
                self.close_p();
                if self.in_scope(Tag::Select).is_some() {
                    self.implied_ends(Tag::Other);
                }
                self.insert_void(start);
                self.frameset_ok = false;
            }
            Tag::Textarea => {
                self.insert_text_element(start);
                self.drop_newline = true;
                self.frameset_ok = false;
            }
            Tag::Xmp => {
                self.close_p();
                self.reconstruct();
                self.frameset_ok = false;
                self.insert_text_element(start);
            }
            Tag::Iframe => {
                self.frameset_ok = false;
                self.insert_text_element(start);
            }
            Tag::Noembed => self.insert_text_element(start),
            Tag::Select => {
                // A select in a select closes it instead.
                if self.context == Some(Tag::Select) || self.close_in_scope(Tag::Select) {
                    return Did::Done;
                }
                self.reconstruct();
                self.insert_element(start);
                self.frameset_ok = false;
            }
            Tag::Option => {
                if self.in_scope(Tag::Select).is_some() {
                    self.implied_ends(Tag::Optgroup);
                } else if self.current() == Tag::Option {
                    self.pop();
                }
                self.reconstruct();
                self.insert_element(start);
            }
            Tag::Optgroup => {
                if self.in_scope(Tag::Select).is_some() {
                    self.implied_ends(Tag::Other);
                } else if self.current() == Tag::Option {
                    self.pop();
                }
                self.reconstruct();
                self.insert_element(start);
            }
            Tag::Rb | Tag::Rtc => {
                if self.in_scope(Tag::Ruby).is_some() {
                    self.implied_ends(Tag::Other);
                }
                self.insert_element(start);
            }
            Tag::Rp | Tag::Rt => {
                if self.in_scope(Tag::Ruby).is_some() {
                    self.implied_ends(Tag::Rtc);
                }
                self.insert_element(start);
            }
            Tag::MathRoot => {
                self.reconstruct();
                self.insert_foreign(start, Namespace::MathMl);
            }
            Tag::SvgRoot => {
                self.reconstruct();
                self.insert_foreign(start, Namespace::Svg);
            }
            Tag::Caption
            | Tag::Col
            | Tag::Colgroup
            | Tag::Frame
            | Tag::Head
            | Tag::Tbody
            | Tag::Td
            | Tag::Tfoot
            | Tag::Th
            | Tag::Thead
            | Tag::Tr => {}
            _ => {
                self.reconstruct();
                self.insert_element(start);
            }
        }
        Did::Done
    }

    /// The body element, if it is the second on the stack.
    fn second_is_body(&self) -> Option<NodeId> {
        self.open
            .get(1)
            .filter(|open| open.tag == Tag::Body)
            .map(|open| open.id)
    }

    /// Close the open item a new `<li>` (or `<dd>`/`<dt>`) replaces.
    fn close_item(&mut self, kinds: &[Tag]) {
        let found = self.find(
            |open| kinds.contains(&open.tag),
            |tag| tag.is_special() && !matches!(tag, Tag::Address | Tag::Div | Tag::P),
        );
        if let Some(index) = found {
            self.close_from(index);
        }
    }

    fn in_body_end(&mut self, tag: Tag, name: &str) -> Did {
        match tag {
            Tag::Body | Tag::Html => {
                if self.in_scope(Tag::Body).is_some() {
                    self.mode = Mode::AfterBody;
                    if tag == Tag::Html {
                        return Did::Again;
                    }
                }
            }
            Tag::Address
            | Tag::Article
            | Tag::Aside
            | Tag::Blockquote
            | Tag::Button
            | Tag::Center
            | Tag::Details
            | Tag::Dialog
            | Tag::Dir
            | Tag::Div
            | Tag::Dl
            | Tag::Fieldset
            | Tag::Figcaption
            | Tag::Figure
            | Tag::Footer
            | Tag::Header
            | Tag::Hgroup
            | Tag::Listing
            | Tag::Main
            | Tag::Menu
            | Tag::Nav
            | Tag::Ol
            | Tag::Pre
            | Tag::Search
            | Tag::Section
            | Tag::Select
            | Tag::Summary
            | Tag::Ul
            | Tag::Dd
            | Tag::Dt => {
                self.close_in_scope(tag);
            }
            Tag::Form => {
                if self.in_template_contents() {
                    self.close_in_scope(Tag::Form);
                } else if let Some(form) = self.form.take()
                    && self
                        .find(|open| open.id == form, Tag::bounds_scope)
                        .is_some()
                {
                    self.implied_ends(Tag::Other);
                    self.remove_open(form);
                }
            }
            Tag::P => {
                if !self.close_p() {
                    self.insert_implied("p");
                    self.pop();
                }
            }
            Tag::Li => {
                if let Some(index) =
                    self.find(|open| open.tag == Tag::Li, Tag::bounds_list_item_scope)
                {
                    self.close_from(index);
                }
            }
            _ if tag.is_heading() => {
                if let Some(index) = self.find(|open| open.tag.is_heading(), Tag::bounds_scope) {
                    self.close_from(index);
                }
            }
            _ if tag.is_formatting() => self.adopt(tag, name),
            Tag::Applet | Tag::Marquee | Tag::Object => {
                if self.close_in_scope(tag) {
                    self.clear_active();
                }
            }
            Tag::Br => {
                self.reconstruct();
                self.insert_implied("br");
                self.pop();
                self.frameset_ok = false;
            }
            _ => self.any_other_end(tag, name),
        }
        Did::Done
    }

    /// Any other end tag: it closes the element of its name, unless a
    /// special one is in the way.
    fn any_other_end(&mut self, tag: Tag, name: &str) {
        let found = self.find(|open| self.is_named(open, tag, name), Tag::is_special);
        if let Some(index) = found {
            self.close_from(index);
        }
    }

    /// The adoption agency algorithm: an end tag for a formatting element
    /// that may be misnested with the blocks around it.
    fn adopt(&mut self, tag: Tag, name: &str) {
        if self.current() == tag && self.active_index(self.current_id()).is_none() {
            self.pop();
            return;
        }
        for _ in 0..8 {
            let found = self
                .active_elements()
                .find(|&(_, _, active, _)| active == tag);
            let Some((index, formatting, _, attrs)) = found else {
                self.any_other_end(tag, name);
                return;
            };
            if !self.is_open(formatting) {
                self.active.remove(index);
                return;
            }
            let Some(at) = self.find(|open| open.id == formatting, Tag::bounds_scope) else {
                return;
            };
            let block = self.open[at..]
                .iter()
                .position(|open| open.tag.is_special());
            let Some(block_at) = block.map(|offset| at + offset) else {
                self.close_from(at);
                self.active.remove(index);
                return;
            };
            let Some(new) = self.copy(formatting) else {
                // Nothing more to copy with: just close it.
                self.close_from(at);
                self.active.remove(index);
                return;
            };
            self.moved = true;
            let block = self.open[block_at];
            let ancestor = self.open[at - 1];

            // The bookmark: the list entry the new element goes in front of.
            let mut bookmark = index;
            let mut last = block.id;
            // The stack between the formatting element and the block, as it
            // will be, from the block up.
            let mut kept = Vec::new();
            for inner in 1.. {
                let node_at = block_at - inner;
                if node_at == at {
                    break;
                }
                let node = self.open[node_at];
                let listed = self.active_index(node.id);
                let copy = match listed {
                    Some(listed) if inner <= 3 => self.copy(node.id).map(|copy| (listed, copy)),
                    _ => None,
                };
                let Some((listed, copy)) = copy else {
                    if let Some(listed) = listed {
                        self.active.remove(listed);
                        if listed < bookmark {
                            bookmark -= 1;
                        }
                    }
                    self.set_open(node.id, false);
                    continue;
                };
                if let Active::Element { id, .. } = &mut self.active[listed] {
                    *id = copy;
                }
                self.set_open(node.id, false);
                self.set_open(copy, true);
                kept.push(Open {
                    id: copy,
                    tag: node.tag,
                });
                if last == block.id {
                    bookmark = listed + 1;
                }
                let _ = self.doc.mutate().append_child(copy, last);
                last = copy;
            }

            let place = if self.foster && ancestor.tag.fosters() {
                self.foster_place()
            } else {
                (self.parent_at(at - 1), None)
            };
            // Past the depth limit the place may be in what moves there.
            if !self.insert_at(place, last) {
                let _ = self.doc.mutate().append_child(self.open[0].id, last);
            }

            // The new element takes the block's children, and is its child.
            let children = self.doc.tree().children(block.id).to_vec();
            let _ = self.doc.mutate().replace_children(block.id, &[]);
            let _ = self.doc.mutate().replace_children(new, &children);
            let _ = self.doc.mutate().append_child(block.id, new);

            if let Some(old) = self.active_index(formatting) {
                self.active.remove(old);
                if old < bookmark {
                    bookmark -= 1;
                }
            }
            self.active.insert(
                bookmark.min(self.active.len()),
                Active::Element {
                    id: new,
                    tag,
                    attrs,
                },
            );
            self.set_open(formatting, false);
            self.set_open(new, true);
            kept.reverse();
            kept.push(block);
            kept.push(Open { id: new, tag });
            self.open.splice(at..=block_at, kept);
        }
    }

    fn text(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => {
                self.insert_text(text);
                Did::Done
            }
            Tok::End(..) => {
                self.pop();
                self.mode = self.original;
                Did::Done
            }
            _ => {
                self.pop();
                self.mode = self.original;
                Did::Again
            }
        }
    }

    fn in_table(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(_) if self.current().fosters() || self.current() == Tag::Template => {
                self.table_text.clear();
                self.original = self.mode;
                self.mode = Mode::InTableText;
                return Did::Again;
            }
            Tok::Text(_) => {}
            Tok::Comment(text) => {
                self.insert_comment(text);
                return Did::Done;
            }
            Tok::Doctype { .. } => return Did::Done,
            Tok::Start(start) => match start.tag {
                Tag::Caption => {
                    self.clear_to(&[Tag::Table]);
                    self.active.push(Active::Marker);
                    self.insert_element(start);
                    self.mode = Mode::InCaption;
                    return Did::Done;
                }
                Tag::Colgroup => {
                    self.clear_to(&[Tag::Table]);
                    self.insert_element(start);
                    self.mode = Mode::InColumnGroup;
                    return Did::Done;
                }
                Tag::Col => {
                    self.clear_to(&[Tag::Table]);
                    self.insert_implied("colgroup");
                    self.mode = Mode::InColumnGroup;
                    return Did::Again;
                }
                Tag::Tbody | Tag::Tfoot | Tag::Thead => {
                    self.clear_to(&[Tag::Table]);
                    self.insert_element(start);
                    self.mode = Mode::InTableBody;
                    return Did::Done;
                }
                Tag::Td | Tag::Th | Tag::Tr => {
                    self.clear_to(&[Tag::Table]);
                    self.insert_implied("tbody");
                    self.mode = Mode::InTableBody;
                    return Did::Again;
                }
                Tag::Table => {
                    let Some(index) = self.in_table_scope(Tag::Table) else {
                        return Did::Done;
                    };
                    self.close_from(index);
                    self.reset_mode();
                    return Did::Again;
                }
                Tag::Style | Tag::Script | Tag::Template => return self.in_head(token),
                Tag::Input if is_hidden(start) => {
                    self.insert_void(start);
                    return Did::Done;
                }
                Tag::Form => {
                    if self.may_open_form() {
                        self.insert_form(start);
                        self.pop();
                    }
                    return Did::Done;
                }
                _ => {}
            },
            Tok::End(tag, _) => match tag {
                Tag::Table => {
                    if let Some(index) = self.in_table_scope(Tag::Table) {
                        self.close_from(index);
                        self.reset_mode();
                    }
                    return Did::Done;
                }
                Tag::Body
                | Tag::Caption
                | Tag::Col
                | Tag::Colgroup
                | Tag::Html
                | Tag::Tbody
                | Tag::Td
                | Tag::Tfoot
                | Tag::Th
                | Tag::Thead
                | Tag::Tr => return Did::Done,
                Tag::Template => return self.in_head(token),
                _ => {}
            },
            Tok::Eof => return self.in_body(token),
        }
        self.foster = true;
        let did = self.in_body(token);
        self.foster = false;
        did
    }

    fn in_table_text(&mut self, token: &mut Tok) -> Did {
        if let Tok::Text(text) = token {
            self.table_text.extend(text.chars().filter(|&c| c != '\0'));
            return Did::Done;
        }
        let text = std::mem::take(&mut self.table_text);
        self.mode = self.original;
        if is_blank(&text) {
            self.insert_text(&text);
        } else {
            self.foster = true;
            self.in_body(&mut Tok::Text(text));
            self.foster = false;
        }
        Did::Again
    }

    fn in_caption(&mut self, token: &mut Tok) -> Did {
        let again = match token {
            Tok::End(Tag::Caption, _) => false,
            Tok::Start(Start {
                tag:
                    Tag::Caption
                    | Tag::Col
                    | Tag::Colgroup
                    | Tag::Tbody
                    | Tag::Td
                    | Tag::Tfoot
                    | Tag::Th
                    | Tag::Thead
                    | Tag::Tr,
                ..
            })
            | Tok::End(Tag::Table, _) => true,
            Tok::End(
                Tag::Body
                | Tag::Col
                | Tag::Colgroup
                | Tag::Html
                | Tag::Tbody
                | Tag::Td
                | Tag::Tfoot
                | Tag::Th
                | Tag::Thead
                | Tag::Tr,
                _,
            ) => return Did::Done,
            _ => return self.in_body(token),
        };
        let Some(index) = self.in_table_scope(Tag::Caption) else {
            return Did::Done;
        };
        self.close_from(index);
        self.clear_active();
        self.mode = Mode::InTable;
        if again { Did::Again } else { Did::Done }
    }

    fn in_column_group(&mut self, token: &mut Tok) -> Did {
        let mut again = true;
        match token {
            Tok::Text(text) => {
                if self.take_blank(text) {
                    return Did::Done;
                }
            }
            Tok::Comment(text) => {
                self.insert_comment(text);
                return Did::Done;
            }
            Tok::Doctype { .. } => return Did::Done,
            Tok::Start(start) => match start.tag {
                Tag::Html => return self.in_body(token),
                Tag::Col => {
                    self.insert_void(start);
                    return Did::Done;
                }
                Tag::Template => return self.in_head(token),
                _ => {}
            },
            Tok::End(tag, _) => match tag {
                Tag::Colgroup => again = false,
                Tag::Col => return Did::Done,
                Tag::Template => return self.in_head(token),
                _ => {}
            },
            Tok::Eof => return self.in_body(token),
        }
        if self.current() != Tag::Colgroup {
            // In a template: only white space stays.
            if let Tok::Text(text) = token {
                self.insert_blanks(text);
            }
            return Did::Done;
        }
        self.pop();
        self.mode = Mode::InTable;
        if again { Did::Again } else { Did::Done }
    }

    fn in_table_body(&mut self, token: &mut Tok) -> Did {
        const CONTEXT: &[Tag] = &[Tag::Tbody, Tag::Tfoot, Tag::Thead];
        match token {
            Tok::Start(start) => match start.tag {
                Tag::Tr => {
                    self.clear_to(CONTEXT);
                    self.insert_element(start);
                    self.mode = Mode::InRow;
                    return Did::Done;
                }
                Tag::Th | Tag::Td => {
                    self.clear_to(CONTEXT);
                    self.insert_implied("tr");
                    self.mode = Mode::InRow;
                    return Did::Again;
                }
                Tag::Caption | Tag::Col | Tag::Colgroup | Tag::Tbody | Tag::Tfoot | Tag::Thead => {}
                _ => return self.in_table(token),
            },
            Tok::End(tag, _) => match tag {
                Tag::Tbody | Tag::Tfoot | Tag::Thead => {
                    if self.in_table_scope(*tag).is_some() {
                        self.clear_to(CONTEXT);
                        self.pop();
                        self.mode = Mode::InTable;
                    }
                    return Did::Done;
                }
                Tag::Table => {}
                Tag::Body
                | Tag::Caption
                | Tag::Col
                | Tag::Colgroup
                | Tag::Html
                | Tag::Td
                | Tag::Th
                | Tag::Tr => return Did::Done,
                _ => return self.in_table(token),
            },
            _ => return self.in_table(token),
        }
        let group = self.find(|open| CONTEXT.contains(&open.tag), Tag::bounds_table_scope);
        if group.is_none() {
            return Did::Done;
        }
        self.clear_to(CONTEXT);
        self.pop();
        self.mode = Mode::InTable;
        Did::Again
    }

    fn in_row(&mut self, token: &mut Tok) -> Did {
        let again = match token {
            Tok::Start(start) => match start.tag {
                Tag::Th | Tag::Td => {
                    self.clear_to(&[Tag::Tr]);
                    self.insert_element(start);
                    self.mode = Mode::InCell;
                    self.active.push(Active::Marker);
                    return Did::Done;
                }
                Tag::Caption
                | Tag::Col
                | Tag::Colgroup
                | Tag::Tbody
                | Tag::Tfoot
                | Tag::Thead
                | Tag::Tr => true,
                _ => return self.in_table(token),
            },
            Tok::End(tag, _) => match tag {
                Tag::Tr => false,
                Tag::Table => true,
                Tag::Tbody | Tag::Tfoot | Tag::Thead => {
                    if self.in_table_scope(*tag).is_none() {
                        return Did::Done;
                    }
                    true
                }
                Tag::Body
                | Tag::Caption
                | Tag::Col
                | Tag::Colgroup
                | Tag::Html
                | Tag::Td
                | Tag::Th => return Did::Done,
                _ => return self.in_table(token),
            },
            _ => return self.in_table(token),
        };
        if self.in_table_scope(Tag::Tr).is_none() {
            return Did::Done;
        }
        self.clear_to(&[Tag::Tr]);
        self.pop();
        self.mode = Mode::InTableBody;
        if again { Did::Again } else { Did::Done }
    }

    fn in_cell(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::End(tag @ (Tag::Td | Tag::Th), _) => {
                if let Some(index) = self.in_table_scope(*tag) {
                    self.close_cell(index);
                }
                Did::Done
            }
            Tok::Start(Start {
                tag:
                    Tag::Caption
                    | Tag::Col
                    | Tag::Colgroup
                    | Tag::Tbody
                    | Tag::Td
                    | Tag::Tfoot
                    | Tag::Th
                    | Tag::Thead
                    | Tag::Tr,
                ..
            }) => self.end_cell(),
            Tok::End(Tag::Body | Tag::Caption | Tag::Col | Tag::Colgroup | Tag::Html, _) => {
                Did::Done
            }
            Tok::End(tag @ (Tag::Table | Tag::Tbody | Tag::Tfoot | Tag::Thead | Tag::Tr), _) => {
                if self.in_table_scope(*tag).is_none() {
                    return Did::Done;
                }
                self.end_cell()
            }
            _ => self.in_body(token),
        }
    }

    /// Close the cell and process the token again.
    fn end_cell(&mut self) -> Did {
        let cell = self.find(
            |open| matches!(open.tag, Tag::Td | Tag::Th),
            Tag::bounds_table_scope,
        );
        match cell {
            Some(index) => {
                self.close_cell(index);
                Did::Again
            }
            None => Did::Done,
        }
    }

    fn close_cell(&mut self, index: usize) {
        self.close_from(index);
        self.clear_active();
        self.mode = Mode::InRow;
    }

    fn in_template(&mut self, token: &mut Tok) -> Did {
        let mode = match token {
            Tok::Text(_) | Tok::Comment(_) | Tok::Doctype { .. } => return self.in_body(token),
            Tok::Start(start) => match start.tag {
                Tag::Base
                | Tag::Basefont
                | Tag::Bgsound
                | Tag::Link
                | Tag::Meta
                | Tag::Noframes
                | Tag::Script
                | Tag::Style
                | Tag::Template
                | Tag::Title => return self.in_head(token),
                Tag::Caption | Tag::Colgroup | Tag::Tbody | Tag::Tfoot | Tag::Thead => {
                    Mode::InTable
                }
                Tag::Col => Mode::InColumnGroup,
                Tag::Tr => Mode::InTableBody,
                Tag::Td | Tag::Th => Mode::InRow,
                _ => Mode::InBody,
            },
            Tok::End(Tag::Template, _) => return self.in_head(token),
            Tok::End(..) => return Did::Done,
            Tok::Eof => {
                if !self.has_open(Tag::Template) {
                    return Did::Done;
                }
                self.end_template();
                return Did::Again;
            }
        };
        self.templates.pop();
        self.templates.push(mode);
        self.mode = mode;
        Did::Again
    }

    fn after_body(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) if is_blank(text) => return self.in_body(token),
            Tok::Comment(text) => {
                let html = self.open[0].id;
                self.insert_comment_in(html, text);
                return Did::Done;
            }
            Tok::Doctype { .. } | Tok::Eof => return Did::Done,
            Tok::Start(start) if start.tag == Tag::Html => return self.in_body(token),
            Tok::End(Tag::Html, _) => {
                if self.context.is_none() {
                    self.mode = Mode::AfterAfterBody;
                }
                return Did::Done;
            }
            _ => {}
        }
        self.mode = Mode::InBody;
        Did::Again
    }

    fn in_frameset(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => self.insert_blanks(text),
            Tok::Comment(text) => self.insert_comment(text),
            Tok::Start(start) => match start.tag {
                Tag::Html => return self.in_body(token),
                Tag::Frameset => {
                    self.insert_element(start);
                }
                Tag::Frame => self.insert_void(start),
                Tag::Noframes => return self.in_head(token),
                _ => {}
            },
            Tok::End(Tag::Frameset, _) if self.open.len() > 1 => {
                self.pop();
                if self.context.is_none() && self.current() != Tag::Frameset {
                    self.mode = Mode::AfterFrameset;
                }
            }
            _ => {}
        }
        Did::Done
    }

    fn after_frameset(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => self.insert_blanks(text),
            Tok::Comment(text) => self.insert_comment(text),
            Tok::Start(start) if start.tag == Tag::Html => return self.in_body(token),
            Tok::Start(start) if start.tag == Tag::Noframes => return self.in_head(token),
            Tok::End(Tag::Html, _) => self.mode = Mode::AfterAfterFrameset,
            _ => {}
        }
        Did::Done
    }

    fn after_after_body(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Comment(text) => {
                let root = self.doc.root();
                self.insert_comment_in(root, text);
                Did::Done
            }
            Tok::Doctype { .. } | Tok::Eof => Did::Done,
            Tok::Text(text) if is_blank(text) => self.in_body(token),
            Tok::Start(start) if start.tag == Tag::Html => self.in_body(token),
            _ => {
                self.mode = Mode::InBody;
                Did::Again
            }
        }
    }

    fn after_after_frameset(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Comment(text) => {
                let root = self.doc.root();
                self.insert_comment_in(root, text);
                Did::Done
            }
            Tok::Text(text) => {
                text.retain(is_space);
                if text.is_empty() {
                    return Did::Done;
                }
                self.in_body(token)
            }
            Tok::Start(start) if start.tag == Tag::Html => self.in_body(token),
            Tok::Start(start) if start.tag == Tag::Noframes => self.in_head(token),
            _ => Did::Done,
        }
    }

    // --- foreign content --------------------------------------------------

    /// Whether the rules for foreign content process `token`.
    fn is_foreign(&self, token: &Tok) -> bool {
        let Some(node) = self.adjusted_current() else {
            return false;
        };
        if node.is_html() {
            return false;
        }
        let points = match node {
            Tag::MathText => match token {
                Tok::Start(start) => !matches!(start.tag, Tag::Mglyph | Tag::Malignmark),
                _ => matches!(token, Tok::Text(_)),
            },
            Tag::Annotation => matches!(token, Tok::Start(start) if start.tag == Tag::SvgRoot),
            Tag::AnnotationHtml | Tag::SvgPoint => matches!(token, Tok::Start(_) | Tok::Text(_)),
            _ => false,
        };
        !points && !matches!(token, Tok::Eof)
    }

    /// The rules for parsing tokens in foreign content.
    fn in_foreign(&mut self, token: &mut Tok) -> Did {
        match token {
            Tok::Text(text) => {
                if !text.chars().all(|c| is_space(c) || c == '\0') {
                    self.frameset_ok = false;
                }
                self.insert_text(&text.replace('\0', "\u{FFFD}"));
                Did::Done
            }
            Tok::Comment(text) => {
                self.insert_comment(text);
                Did::Done
            }
            Tok::Doctype { .. } | Tok::Eof => Did::Done,
            Tok::Start(start) => {
                let breaks_out = start.tag.breaks_out_of_foreign()
                    || (start.tag == Tag::Font
                        && start
                            .attrs
                            .iter()
                            .any(|(name, _)| matches!(name.as_str(), "color" | "face" | "size")));
                if breaks_out {
                    self.break_out();
                    return self.by_mode(token);
                }
                let namespace = self
                    .adjusted_current()
                    .map_or(Namespace::Html, Tag::namespace);
                self.insert_foreign(start, namespace);
                Did::Done
            }
            Tok::End(Tag::Br | Tag::P, _) => {
                self.break_out();
                self.by_mode(token)
            }
            Tok::End(_, name) => {
                for index in (1..self.open.len()).rev().take(MAX_SEARCH) {
                    let open = self.open[index];
                    if open.tag.is_html() {
                        // The rules for HTML content deal with it.
                        return self.by_mode(token);
                    }
                    if self.doc.element(open.id).is_some_and(|e| e.name == *name) {
                        self.close_from(index);
                        return Did::Done;
                    }
                }
                Did::Done
            }
        }
    }

    /// Pop to where HTML can go.
    fn break_out(&mut self) {
        while self.open.len() > 1
            && !matches!(
                self.current(),
                Tag::MathText | Tag::AnnotationHtml | Tag::SvgPoint
            )
            && !self.current().is_html()
        {
            self.pop();
        }
    }

    // --- text ---------------------------------------------------------------

    /// Insert the white space `text` starts with, and drop it from `text`:
    /// whether that was all of it.
    fn take_blank(&mut self, text: &mut String) -> bool {
        let rest = text.trim_start_matches(is_space).len();
        let blank = text.len() - rest;
        if blank > 0 {
            let (blank, _) = text.split_at(blank);
            let blank = blank.to_owned();
            self.insert_text(&blank);
        }
        text.drain(..blank);
        text.is_empty()
    }

    /// Insert only the white space of `text`.
    fn insert_blanks(&mut self, text: &str) {
        let blanks: String = text.chars().filter(|&c| is_space(c)).collect();
        self.insert_text(&blanks);
    }

    // --- depth ----------------------------------------------------------------

    /// Keep the tree within the depth limit where reparenting may have made
    /// it deeper: what is past it moves up beside its ancestor at the limit,
    /// in document order.
    fn limit_depth(&mut self) {
        if !self.moved {
            return;
        }
        let base = match self.context {
            Some(_) => self.open[0].id,
            None => self.doc.root(),
        };
        // The parents whose children are at the limit.
        let mut parents = Vec::new();
        let mut stack = vec![(base, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            let children = self.doc.tree().children(node);
            if depth + 1 == MAX_TREE_DEPTH {
                let deep = |&child: &NodeId| !self.doc.tree().children(child).is_empty();
                if children.iter().any(deep) {
                    parents.push(node);
                }
                continue;
            }
            stack.extend(children.iter().map(|&child| (child, depth + 1)));
        }
        for parent in parents {
            let mut flat = Vec::new();
            for &child in self.doc.tree().children(parent) {
                flat.push(child);
                flat.extend(self.doc.tree().descendants(child));
            }
            for &node in &flat {
                if self.doc.node(node).is_some_and(NodeData::can_have_children) {
                    let _ = self.doc.mutate().replace_children(node, &[]);
                }
            }
            let _ = self.doc.mutate().replace_children(parent, &flat);
        }
    }
}

/// HTML's ASCII whitespace.
fn is_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' ')
}

fn is_blank(text: &str) -> bool {
    text.chars().all(is_space)
}

/// Drop the white space `text` starts with: whether that was all of it.
fn skip_blank(text: &mut String) -> bool {
    let blank = text.len() - text.trim_start_matches(is_space).len();
    text.drain(..blank);
    text.is_empty()
}

/// Whether the tag is an `<input type=hidden>`.
fn is_hidden(start: &Start) -> bool {
    start
        .attrs
        .iter()
        .find(|(name, _)| name == "type")
        .and_then(|(_, value)| value.as_deref())
        .is_some_and(|value| value.eq_ignore_ascii_case("hidden"))
}
