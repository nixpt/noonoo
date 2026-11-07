//! # noonoo-layout
//!
//! Box layout: computed style plus measured text into a positioned box tree.
//! This is Phase B of the dependency-exit plan (`docs/DEPENDENCY-EXIT.md`) — it
//! replaces `taffy` with an owned engine.
//!
//! ## What it covers (v1)
//!
//! - **Block flow** — children stacked vertically, auto width filling the
//!   container.
//! - **Inline flow** — text and inline-level boxes wrapped into lines.
//! - **Flex** — row/column, `flex-grow`/`flex-shrink`, `justify-content`,
//!   `align-items`/`align-self` cross alignment and stretch, gaps, and row
//!   `flex-wrap`.
//! - **Positioning** — `position: relative` (offsets its subtree without moving
//!   siblings) and `position: absolute` / `fixed` (out of flow, placed against
//!   the nearest positioned ancestor's padding box — the viewport for `fixed`).
//!   `sticky` is treated as `relative`.
//! - **Floats** — `float: left`/`right` shapes its block formatting context:
//!   line boxes wrap around floats (including floats declared in sibling
//!   blocks), floats stack beside each other and drop below when a band is too
//!   narrow, `clear` sits below named sides, `width: auto` shrinks to fit, and
//!   a container's auto height includes its own floats' bottoms. Boxes that
//!   establish a formatting context (clipping boxes, floats, flex, out-of-flow)
//!   isolate their floats.
//! - **Grid** — `display: grid` with track lists (`<length-percentage>`,
//!   `auto`, `<fr>`, and `repeat()`), `column-gap`/`row-gap`, line-number and
//!   `span N` placement, sparse row-major auto-placement, implicit tracks
//!   sized by `grid-auto-rows`/`grid-auto-columns`, and a container whose auto
//!   height is its rows. Columns size against the definite width (fixed first,
//!   `fr` shares the leftover, otherwise `auto` tracks stretch); rows size to
//!   content. Items stretch to their area's height (`align-items`/`align-self`
//!   position within it); loose text in a grid container is dropped (no
//!   anonymous items yet).
//! - **Vertical margin collapsing** — adjacent siblings share the larger of
//!   their facing margins; a block collapses its top/bottom margin through its
//!   first/last child when no border or padding separates them; and an empty
//!   block's two margins become one that joins the surrounding chain.
//! - **Overflow** — a box with a non-`visible` `overflow` clips its content to
//!   its padding box, hit-tests within that clip, and establishes a formatting
//!   context. An `overflow-x`/`overflow-y` of `auto`/`scroll` makes it a **scroll
//!   container** on that axis: the tree tracks a per-box offset, `scroll_by`
//!   clamps it to the content range, and paint shifts — and hit-testing maps —
//!   the content by it.
//! - **Stacking order** — every box carries a paint key from `position` and
//!   `z-index`; paint and hit-testing order siblings by it (negative `z-index`
//!   first, then in-flow content, then positioned boxes by `z-index`).
//! - Width: `auto` / px / percentage (and stretch between `left`+`right`),
//!   clamped by `min-width`/`max-width` (which share `width`'s box per
//!   `box-sizing`, with `min` winning over `max` — flex bases freeze and
//!   re-distribute per CSS 9.7); height: `auto` / px / percentage of the
//!   containing block (percentages behave as `auto` against an indefinite
//!   container), clamped by `min-height`/`max-height`.
//! - `box-sizing`, margins, padding, and borders, resolved to absolute pixels.
//!
//! ## What it does not (yet)
//!
//! Full stacking contexts (a positioned box is ordered within its
//! parent, not hoisted), scrollbar *rendering* (content scrolls; no bars are
//! drawn), `align-items` cross positioning on flex *columns*, `justify-items`
//! positioning in grid areas, and writing modes. Floats are ignored for flex
//! and grid items, per CSS. Percentage
//! tracks on an indefinite axis behave as `auto`. Absolute boxes whose width or height is `auto` fall
//! back to the containing block when only the trailing inset (`right`/`bottom`)
//! is set. Text is measured through the [`Shaper`] seam — the heuristic
//! `MetricShaper` stands in unless the host provides real font metrics.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod block;
mod flex;
mod float;
mod grid;
mod margin;
mod resolve;

use std::collections::{HashMap, HashSet};

use noonoo_dom::{Document, NodeData, NodeId};
use noonoo_geometry::{Point, Rect, Size};
use noonoo_style::{BoxSizing, ComputedStyle, Dimension, Display, Edges, Position, Styles, ZIndex};

pub use noonoo_text::{Align, MetricShaper, Shaper};

/// The role a laid-out box plays.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoxKind {
    /// A block-level element.
    Block,
    /// An inline-level element box.
    Inline,
    /// A flex container.
    Flex,
    /// A grid container.
    Grid,
    /// A text run.
    Text,
}

/// A positioned box, in absolute page coordinates (border-box).
#[derive(Clone, Debug)]
pub struct LayoutBox {
    /// The source node, if any (anonymous boxes have none).
    pub node: Option<NodeId>,
    /// The element whose computed style governs this box's painting.
    pub style_node: NodeId,
    /// The box's role.
    pub kind: BoxKind,
    /// The border-box rectangle.
    pub rect: Rect,
    /// The text this box draws, if any (text runs; inline element content).
    pub text: Option<String>,
    /// Resolved margins.
    pub margin: Edges<f32>,
    /// Resolved padding.
    pub padding: Edges<f32>,
    /// Resolved border widths.
    pub border: Edges<f32>,
    /// Whether this box clips its content (a non-`visible` `overflow`).
    pub clips: bool,
    /// The maximum horizontal scroll for this box (its content right minus its
    /// padding box right, floored at zero). Zero when it does not scroll.
    pub scroll_max_x: f32,
    /// The maximum vertical scroll for this box.
    pub scroll_max_y: f32,
    /// The current horizontal scroll offset.
    pub scroll_x: f32,
    /// The current vertical scroll offset.
    pub scroll_y: f32,
    /// The box's paint tier within its parent: `0` for a negative-`z-index`
    /// positioned box, `1` for in-flow content, `2` for a positioned box with
    /// `z-index: 0`/`auto`/positive.
    pub z_tier: u8,
    /// The box's `z-index` (`auto` is `0`).
    pub z_index: i32,
    /// Child box indices.
    pub children: Vec<usize>,
}

impl LayoutBox {
    /// The padding-box rectangle (inside the border).
    pub fn padding_box(&self) -> Rect {
        self.rect.inset(
            self.border.top,
            self.border.right,
            self.border.bottom,
            self.border.left,
        )
    }

    /// The content-box rectangle (inside the padding).
    pub fn content_box(&self) -> Rect {
        self.padding_box().inset(
            self.padding.top,
            self.padding.right,
            self.padding.bottom,
            self.padding.left,
        )
    }
}

/// The result of a layout pass: every box, plus a node → box index.
#[derive(Clone, Debug, Default)]
pub struct LayoutTree {
    boxes: Vec<LayoutBox>,
    index: HashMap<NodeId, usize>,
    roots: Vec<usize>,
}

impl LayoutTree {
    /// The box for `node`, if it produced one.
    pub fn get(&self, node: NodeId) -> Option<&LayoutBox> {
        self.index.get(&node).map(|&i| &self.boxes[i])
    }

    /// Every box, in creation (post-order) order.
    pub fn boxes(&self) -> &[LayoutBox] {
        &self.boxes
    }

    /// The top-level boxes.
    pub fn roots(&self) -> impl Iterator<Item = &LayoutBox> {
        self.roots.iter().map(|&i| &self.boxes[i])
    }

    /// Indices of the top-level boxes.
    pub fn root_indices(&self) -> &[usize] {
        &self.roots
    }

    /// The total height of the laid-out content (the bottom of the lowest root
    /// box). The scroll range is this minus the viewport height.
    pub fn content_height(&self) -> f32 {
        self.roots
            .iter()
            .map(|&index| self.boxes[index].rect.bottom())
            .fold(0.0, f32::max)
    }

    /// The deepest node whose border-box contains `point`.
    ///
    /// Later siblings (painted on top) win over earlier ones at equal depth, so
    /// this matches what the user sees.
    pub fn hit_test(&self, point: Point) -> Option<NodeId> {
        let mut best: Option<(usize, NodeId)> = None;
        let mut roots = self.roots.clone();
        roots.sort_by_key(|&root| (self.boxes[root].z_tier, self.boxes[root].z_index));
        for root in roots {
            self.hit_box(root, point, 0, None, Point::ZERO, &mut best);
        }
        best.map(|(_, node)| node)
    }

    fn hit_box(
        &self,
        index: usize,
        point: Point,
        depth: usize,
        clip: Option<Rect>,
        scroll: Point,
        best: &mut Option<(usize, NodeId)>,
    ) {
        // A point outside a clipping ancestor's box reaches nothing inside it.
        if clip.is_some_and(|rect| !rect.contains(point)) {
            return;
        }
        let box_ = &self.boxes[index];
        // `point` is in window space and content is shifted by any ancestor
        // scroll offsets, so test the box in its own (content) space.
        let content = Point::new(point.x + scroll.x, point.y + scroll.y);
        // A positioned child may sit outside its parent's box, so we always
        // descend; we only *record* a box whose rect actually contains the point.
        if box_.rect.contains(content)
            && let Some(node) = box_.node
        {
            let better = match best {
                Some((best_depth, _)) => depth >= *best_depth,
                None => true,
            };
            if better {
                *best = Some((depth, node));
            }
        }
        let child_clip = if box_.clips {
            Some(box_.padding_box().translate(-scroll.x, -scroll.y))
        } else {
            clip
        };
        let child_scroll = Point::new(scroll.x + box_.scroll_x, scroll.y + box_.scroll_y);
        // Visit children in paint order, so the topmost (last-painted) box wins
        // the depth tie-break below.
        let mut children = box_.children.clone();
        children.sort_by_key(|&child| (self.boxes[child].z_tier, self.boxes[child].z_index));
        for child in children {
            self.hit_box(child, point, depth + 1, child_clip, child_scroll, best);
        }
    }

    /// The current vertical scroll offset of `node` (zero if it does not scroll).
    pub fn scroll_offset(&self, node: NodeId) -> f32 {
        self.index
            .get(&node)
            .map(|&i| self.boxes[i].scroll_y)
            .unwrap_or(0.0)
    }

    /// The current horizontal scroll offset of `node`.
    pub fn scroll_offset_x(&self, node: NodeId) -> f32 {
        self.index
            .get(&node)
            .map(|&i| self.boxes[i].scroll_x)
            .unwrap_or(0.0)
    }

    /// The maximum vertical scroll of `node`.
    pub fn scroll_max(&self, node: NodeId) -> f32 {
        self.index
            .get(&node)
            .map(|&i| self.boxes[i].scroll_max_y)
            .unwrap_or(0.0)
    }

    /// The maximum horizontal scroll of `node`.
    pub fn scroll_max_x(&self, node: NodeId) -> f32 {
        self.index
            .get(&node)
            .map(|&i| self.boxes[i].scroll_max_x)
            .unwrap_or(0.0)
    }

    /// Scroll `node` vertically by `dy`, clamped to its range; returns the amount
    /// consumed.
    pub fn scroll_by(&mut self, node: NodeId, dy: f32) -> f32 {
        let Some(&i) = self.index.get(&node) else {
            return 0.0;
        };
        let max = self.boxes[i].scroll_max_y;
        let before = self.boxes[i].scroll_y;
        let after = (before + dy).clamp(0.0, max);
        self.boxes[i].scroll_y = after;
        after - before
    }

    /// Scroll `node` horizontally by `dx`, clamped to its range.
    pub fn scroll_by_x(&mut self, node: NodeId, dx: f32) -> f32 {
        let Some(&i) = self.index.get(&node) else {
            return 0.0;
        };
        let max = self.boxes[i].scroll_max_x;
        let before = self.boxes[i].scroll_x;
        let after = (before + dx).clamp(0.0, max);
        self.boxes[i].scroll_x = after;
        after - before
    }

    /// The scrollable boxes under `point`, innermost first.
    pub fn scroll_chain(&self, point: Point) -> Vec<NodeId> {
        let mut indices = Vec::new();
        for &root in self.roots.iter().rev() {
            self.collect_scroll(root, point, None, 0.0, 0.0, &mut indices);
        }
        indices.reverse();
        indices
            .into_iter()
            .filter_map(|i| self.boxes[i].node)
            .collect()
    }

    fn collect_scroll(
        &self,
        index: usize,
        point: Point,
        clip: Option<Rect>,
        sx: f32,
        sy: f32,
        out: &mut Vec<usize>,
    ) {
        if clip.is_some_and(|rect| !rect.contains(point)) {
            return;
        }
        let box_ = &self.boxes[index];
        let scrollport = box_.padding_box().translate(-sx, -sy);
        if box_.scroll_max_y > 0.0 && scrollport.contains(point) {
            out.push(index);
        }
        let child_clip = if box_.clips { Some(scrollport) } else { clip };
        let (child_sx, child_sy) = (sx + box_.scroll_x, sy + box_.scroll_y);
        let mut children = box_.children.clone();
        children.sort_by_key(|&child| (self.boxes[child].z_tier, self.boxes[child].z_index));
        for child in children {
            self.collect_scroll(child, point, child_clip, child_sx, child_sy, out);
        }
    }

    /// Scroll the chain of containers under `point`, innermost first, returning
    /// `(node, consumed)` pairs and the leftover delta.
    pub fn scroll_at(&mut self, point: Point, dy: f32) -> (Vec<(NodeId, f32)>, f32) {
        let chain = self.scroll_chain(point);
        let mut result = Vec::new();
        let mut leftover = dy;
        for node in chain {
            let consumed = self.scroll_by(node, leftover);
            if consumed != 0.0 {
                result.push((node, consumed));
            }
            leftover -= consumed;
            if leftover.abs() < 1e-4 {
                break;
            }
        }
        (result, leftover)
    }

    /// Copy scroll offsets from `previous` for nodes present in both trees, so a
    /// re-layout preserves what the user scrolled.
    pub fn inherit_scroll(&mut self, previous: &LayoutTree) {
        for i in 0..self.boxes.len() {
            if let Some(node) = self.boxes[i].node
                && let Some(&j) = previous.index.get(&node)
            {
                let (max_x, max_y) = (self.boxes[i].scroll_max_x, self.boxes[i].scroll_max_y);
                self.boxes[i].scroll_x = previous.boxes[j].scroll_x.clamp(0.0, max_x);
                self.boxes[i].scroll_y = previous.boxes[j].scroll_y.clamp(0.0, max_y);
            }
        }
    }
}

/// Lay out `document` for the given `viewport`.
pub fn layout(
    document: &Document,
    styles: &Styles,
    viewport: Size,
    shaper: &dyn Shaper,
) -> LayoutTree {
    let mut ctx = Ctx {
        document,
        styles,
        shaper,
        viewport,
        pending: Vec::new(),
        floats: Vec::new(),
        tree: LayoutTree::default(),
    };
    // The root's children share one block formatting context.
    ctx.push_bfc();

    // Pass 1 — normal flow, collapsing vertical margins. The viewport is the
    // container: it has no margins, so a leading child margin is not collapsed
    // away (the first child is simply pushed down by it).
    let root_children = child_nodes(document, document.root());
    let (roots, _) = block::layout_block_children(
        &mut ctx,
        document.root(),
        &root_children,
        0.0,
        0.0,
        viewport.width,
        &ComputedStyle::default(),
        false,
        false,
    );
    ctx.tree.roots = roots;

    // Pass 2 — out-of-flow boxes, against their now-laid-out containing block.
    // Laying one out may enqueue nested out-of-flow descendants, so this is a
    // growing, FIFO sweep rather than a fixed loop.
    let mut cursor = 0;
    while cursor < ctx.pending.len() {
        let node = ctx.pending[cursor];
        cursor += 1;
        ctx.layout_out_of_flow(node);
    }

    ctx.pop_bfc();
    ctx.tree
}

/// Shared layout state.
pub(crate) struct Ctx<'a> {
    pub document: &'a Document,
    pub styles: &'a Styles,
    pub shaper: &'a dyn Shaper,
    pub viewport: Size,
    /// Out-of-flow nodes awaiting pass 2.
    pub pending: Vec<NodeId>,
    /// The block formatting context stack; the top holds the floats visible to
    /// the box being laid out.
    pub floats: Vec<crate::float::Floats>,
    pub tree: LayoutTree,
}

impl Ctx<'_> {
    /// The floats of the current block formatting context.
    pub fn floats(&self) -> &crate::float::Floats {
        self.floats.last().expect("a formatting context is open")
    }

    /// The floats of the current block formatting context, mutably.
    pub fn floats_mut(&mut self) -> &mut crate::float::Floats {
        self.floats
            .last_mut()
            .expect("a formatting context is open")
    }

    /// Open a fresh block formatting context (floats inside it do not escape,
    /// and outer floats do not reach in).
    pub fn push_bfc(&mut self) {
        self.floats.push(crate::float::Floats::new());
    }

    pub fn pop_bfc(&mut self) {
        self.floats.pop();
    }

    pub fn style(&self, node: NodeId) -> &ComputedStyle {
        self.styles
            .get(node)
            .expect("layout only visits styled elements")
    }

    pub fn is_text(&self, node: NodeId) -> bool {
        matches!(self.document.node(node), Some(NodeData::Text(_)))
    }

    /// Whether `node` is inline-level (text, or an inline/inline-block element).
    pub fn is_inline_level(&self, node: NodeId) -> bool {
        if self.is_text(node) {
            return true;
        }
        matches!(
            self.style(node).display,
            Display::Inline | Display::InlineBlock
        )
    }

    /// Whether `node` is `display: none`.
    pub fn is_hidden(&self, node: NodeId) -> bool {
        self.styles
            .get(node)
            .is_some_and(|style| style.display == Display::None)
    }

    /// Whether `node` is taken out of normal flow (`position: absolute`/`fixed`).
    pub fn is_out_of_flow(&self, node: NodeId) -> bool {
        !self.is_text(node)
            && matches!(
                self.style(node).position,
                Position::Absolute | Position::Fixed
            )
    }

    /// Whether `node` floats. Floats leave normal flow but shape their
    /// container, so they are laid out during pass 1, not deferred like
    /// out-of-flow boxes.
    pub fn is_float(&self, node: NodeId) -> bool {
        !self.is_text(node)
            && !matches!(
                self.style(node).position,
                Position::Absolute | Position::Fixed
            )
            && self.style(node).float != noonoo_style::Float::None
    }

    /// The containing block for an out-of-flow `node`.
    ///
    /// `fixed` resolves against the viewport; `absolute` against the padding box
    /// of the nearest positioned ancestor, falling back to the viewport (the
    /// initial containing block) when there is none.
    pub fn containing_block(&self, node: NodeId) -> Rect {
        if self.style(node).position == Position::Fixed {
            return Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height);
        }
        for ancestor in self.document.tree().ancestors(node) {
            if let Some(style) = self.styles.get(ancestor)
                && style.position != Position::Static
                && let Some(index) = self.tree_index(ancestor)
            {
                return self.tree.boxes[index].padding_box();
            }
        }
        Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height)
    }

    /// The box index for `node`, if it has been laid out.
    pub fn tree_index(&self, node: NodeId) -> Option<usize> {
        self.tree.index.get(&node).copied()
    }

    pub fn push(&mut self, b: LayoutBox) -> usize {
        let index = self.tree.boxes.len();
        if let Some(node) = b.node {
            self.tree.index.insert(node, index);
        }
        self.tree.boxes.push(b);
        index
    }

    /// The bottom edge of `index`'s box subtree (the maximum over descendants).
    fn subtree_bottom(&self, index: usize) -> f32 {
        let box_ = &self.tree.boxes[index];
        let mut bottom = box_.rect.bottom();
        for &child in &box_.children {
            bottom = bottom.max(self.subtree_bottom(child));
        }
        bottom
    }

    /// The right edge of `index`'s box subtree.
    fn subtree_right(&self, index: usize) -> f32 {
        let box_ = &self.tree.boxes[index];
        let mut right = box_.rect.right();
        for &child in &box_.children {
            right = right.max(self.subtree_right(child));
        }
        right
    }

    /// The `(x, y)` scroll range of a box whose children are `indices`: their
    /// furthest extent minus the padding box, floored at zero per axis. An axis
    /// is zero unless the box scrolls on it.
    fn scroll_max_for(
        &self,
        style: &ComputedStyle,
        indices: &[usize],
        padding_box: Rect,
    ) -> (f32, f32) {
        let range = |scrollable: bool, extent: f32, edge: f32| {
            if scrollable && extent.is_finite() {
                (extent - edge).max(0.0)
            } else {
                0.0
            }
        };
        let right = indices
            .iter()
            .map(|&child| self.subtree_right(child))
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = indices
            .iter()
            .map(|&child| self.subtree_bottom(child))
            .fold(f32::NEG_INFINITY, f32::max);
        (
            range(resolve::scrollable_x(style), right, padding_box.right()),
            range(resolve::scrollable_y(style), bottom, padding_box.bottom()),
        )
    }

    /// Lay out one element's border box at `(border_x, border_y)` and return its
    /// border-box height.
    ///
    /// The vertical margins are *not* applied here: the caller has already
    /// collapsed them and positioned the border box (see [`crate::margin`]).
    ///
    /// `definite_height` is a height the container has already decided for the
    /// item — a stretched flex/grid item fills it when the item's own `height`
    /// is `auto`. `None` sizes to content, as block flow does.
    pub fn layout_element(
        &mut self,
        node: NodeId,
        border_x: f32,
        border_y: f32,
        available_width: f32,
        definite_height: Option<f32>,
    ) -> f32 {
        let style = self.style(node).clone();
        if style.display == Display::None {
            return 0.0;
        }

        let margin = resolve::margins(&style.margin, available_width);
        let padding = resolve::paddings(&style.padding, available_width);
        let border = resolve::borders(&style);

        let horizontal = padding.left + padding.right + border.left + border.right;
        let vertical = padding.top + padding.bottom + border.top + border.bottom;

        // `width`, `min-width`, and `max-width` share a box (per `box-sizing`):
        // normalise them all to border boxes, clamp — `min` wins over `max` —
        // and derive the content width. An auto width fills the available
        // width (its border box is the whole reference).
        let to_border_box = |width: f32| match style.box_sizing {
            BoxSizing::BorderBox => width,
            BoxSizing::ContentBox => (width + horizontal).max(0.0),
        };
        let specified = resolve::dimension(style.width.clone(), available_width).map(to_border_box);
        let minimum =
            resolve::dimension(style.min_width.clone(), available_width).map(to_border_box);
        let maximum =
            resolve::dimension(style.max_width.clone(), available_width).map(to_border_box);
        let mut border_box_width = specified.unwrap_or_else(|| available_width.max(0.0));
        if let Some(maximum) = maximum {
            border_box_width = border_box_width.min(maximum);
        }
        if let Some(minimum) = minimum {
            border_box_width = border_box_width.max(minimum);
        }
        let content_width = (border_box_width - horizontal).max(0.0);

        let (mut border_x, mut border_y) = (border_x, border_y);

        // `relative` (and `sticky`, unsupported as such) shifts the box and its
        // whole subtree, but leaves the box's flow position — and thus its
        // siblings — untouched.
        if matches!(style.position, Position::Relative | Position::Sticky) {
            let inset = resolve::insets(&style.inset, available_width);
            border_x += inset
                .left
                .or(inset.right.map(|right| -right))
                .unwrap_or(0.0);
            border_y += inset
                .top
                .or(inset.bottom.map(|bottom| -bottom))
                .unwrap_or(0.0);
        }

        let content_x = border_x + border.left + padding.left;
        let content_y = border_y + border.top + padding.top;

        let children_nodes = child_nodes(self.document, node);

        // A block collapses its top/bottom margin through its first/last child
        // only when nothing separates them; a clipping box is a formatting
        // context, so its margins never collapse with its children.
        let clips = resolve::clips(&style);
        let collapse_top = border.top == 0.0 && padding.top == 0.0 && !clips;
        let collapse_bottom = border.bottom == 0.0
            && padding.bottom == 0.0
            && matches!(style.height, Dimension::Auto)
            && !clips;

        // A new block formatting context isolates floats: clipping boxes,
        // floats, flex containers, and out-of-flow boxes start fresh, and
        // their own floats do not escape to their siblings.
        let establishes_bfc = clips
            || style.float != noonoo_style::Float::None
            || matches!(style.position, Position::Absolute | Position::Fixed)
            || matches!(
                style.display,
                Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid
            );
        if establishes_bfc {
            self.push_bfc();
        }

        let (child_indices, content_height) = match style.display {
            Display::Flex | Display::InlineFlex => flex::layout_flex_children(
                self,
                &style,
                &children_nodes,
                content_x,
                content_y,
                content_width,
            ),
            Display::Grid | Display::InlineGrid => crate::grid::layout_grid_children(
                self,
                &style,
                &children_nodes,
                content_x,
                content_y,
                content_width,
            ),
            _ => {
                let (indices, height) = block::layout_block_children(
                    self,
                    node,
                    &children_nodes,
                    content_x,
                    content_y,
                    content_width,
                    &style,
                    collapse_top,
                    collapse_bottom,
                );
                (indices, height)
            }
        };

        if establishes_bfc {
            self.pop_bfc();
        }

        // `height`, `min-height`, and `max-height` share a box (per
        // `box-sizing`), like the widths above. A percentage-bearing height has
        // no definite containing-block height here (the container is still
        // being sized), so it behaves as `auto`.
        let to_border_box_height = |height: f32| match style.box_sizing {
            BoxSizing::BorderBox => height,
            BoxSizing::ContentBox => height + vertical,
        };
        let specified_height = resolve::definite(&style.height).map(to_border_box_height);
        let mut border_box_height = match specified_height {
            Some(height) => height,
            // A stretched item fills the height its container decided.
            None => definite_height.unwrap_or(content_height + vertical),
        };
        if let Some(maximum) = resolve::definite(&style.max_height) {
            border_box_height = border_box_height.min(to_border_box_height(maximum));
        }
        if let Some(minimum) = resolve::definite(&style.min_height) {
            border_box_height = border_box_height.max(to_border_box_height(minimum));
        }

        let padding_box = Rect::new(
            border_x + border.left,
            border_y + border.top,
            (border_box_width - border.left - border.right).max(0.0),
            (border_box_height - border.top - border.bottom).max(0.0),
        );
        let (scroll_max_x, scroll_max_y) = self.scroll_max_for(&style, &child_indices, padding_box);
        let (z_tier, z_index) = paint_key(&style);

        let index = self.push(LayoutBox {
            node: Some(node),
            style_node: node,
            kind: match style.display {
                Display::Flex | Display::InlineFlex => BoxKind::Flex,
                Display::Inline | Display::InlineBlock => BoxKind::Inline,
                _ => BoxKind::Block,
            },
            rect: Rect::new(border_x, border_y, border_box_width, border_box_height),
            text: None,
            margin,
            padding,
            border,
            clips,
            scroll_max_x,
            scroll_max_y,
            scroll_x: 0.0,
            scroll_y: 0.0,
            z_tier,
            z_index,
            children: child_indices,
        });
        let _ = index;

        border_box_height
    }

    /// Measure a node's laid-out border-box height by laying it out at
    /// `(x, y)` and discarding the boxes — for flows that must know heights
    /// before they place (flex lines, grid rows). Out-of-flow descendants
    /// discovered during measurement are enqueued; call [`Ctx::prune_pending`]
    /// after a measurement pass.
    pub fn measure_height(
        &mut self,
        node: NodeId,
        x: f32,
        y: f32,
        available_width: f32,
        definite_height: Option<f32>,
    ) -> f32 {
        let base = self.tree.boxes.len();
        let height = self.layout_element(node, x, y, available_width, definite_height);
        self.tree.boxes.truncate(base);
        self.tree.index.retain(|_, &mut index| index < base);
        height
    }

    /// Drop out-of-flow nodes enqueued more than once (measurement lays items
    /// out, then the real pass lays them again).
    pub fn prune_pending(&mut self) {
        let mut seen = HashSet::new();
        self.pending.retain(|node| seen.insert(*node));
    }

    /// Lay out an out-of-flow box (`position: absolute`/`fixed`) against its
    /// containing block, and attach it to the box tree so it paints and
    /// hit-tests alongside its siblings.
    pub fn layout_out_of_flow(&mut self, node: NodeId) -> Option<usize> {
        let style = self.style(node).clone();
        if style.display == Display::None {
            return None;
        }

        let containing = self.containing_block(node);
        let cb_width = containing.width;
        let cb_height = containing.height;

        let margin = resolve::margins(&style.margin, cb_width);
        let padding = resolve::paddings(&style.padding, cb_width);
        let border = resolve::borders(&style);
        let horizontal = padding.left + padding.right + border.left + border.right;
        let vertical = padding.top + padding.bottom + border.top + border.bottom;
        let inset = resolve::insets(&style.inset, cb_width);

        // Width: an explicit width, or stretched between `left`+`right`, or
        // shrink-to-fit against the available space.
        let specified =
            resolve::dimension(style.width.clone(), cb_width).map(|width| match style.box_sizing {
                BoxSizing::BorderBox => (width - horizontal).max(0.0),
                BoxSizing::ContentBox => width,
            });
        let stretched = match (inset.left, inset.right) {
            (Some(left), Some(right)) => Some((cb_width - left - right - horizontal).max(0.0)),
            _ => None,
        };
        let definite_width = specified.or(stretched);
        let available = match (inset.left, inset.right) {
            (Some(left), None) => {
                (containing.right() - (containing.x + left) - horizontal).max(0.0)
            }
            (None, Some(right)) => (cb_width - right - horizontal).max(0.0),
            (None, None) => (cb_width - horizontal).max(0.0),
            (Some(_), Some(_)) => definite_width.unwrap_or(0.0),
        };
        let content_width = definite_width.unwrap_or(available);

        // Horizontal placement: `left` wins; `right` only when the width is
        // known (an auto width cannot be right-aligned without measuring first).
        let border_x = if let Some(left) = inset.left {
            containing.x + left + margin.left
        } else if let Some(right) = inset.right {
            if definite_width.is_some() {
                containing.right() - right - margin.right - (content_width + horizontal)
            } else {
                containing.x + margin.left
            }
        } else {
            containing.x + margin.left
        };

        let border_y = if let Some(top) = inset.top {
            containing.y + top + margin.top
        } else if let Some(bottom) = inset.bottom {
            match resolve::dimension(style.height.clone(), cb_height) {
                Some(height) => containing.bottom() - bottom - margin.bottom - (height + vertical),
                None => containing.y + margin.top,
            }
        } else {
            containing.y + margin.top
        };

        let content_x = border_x + border.left + padding.left;
        let content_y = border_y + border.top + padding.top;

        let children_nodes = child_nodes(self.document, node);
        let (child_indices, content_height) = match style.display {
            Display::Flex | Display::InlineFlex => flex::layout_flex_children(
                self,
                &style,
                &children_nodes,
                content_x,
                content_y,
                content_width,
            ),
            _ => {
                let (indices, height) = block::layout_block_children(
                    self,
                    node,
                    &children_nodes,
                    content_x,
                    content_y,
                    content_width,
                    &style,
                    false,
                    false,
                );
                (indices, height)
            }
        };

        let border_width = match definite_width {
            Some(width) => width + horizontal,
            None => {
                let used = child_indices
                    .iter()
                    .map(|&child| self.tree.boxes[child].rect.right())
                    .fold(content_x, f32::max)
                    - content_x;
                used.clamp(0.0, available) + horizontal
            }
        };

        let border_height = match resolve::dimension(style.height.clone(), cb_height) {
            Some(height) => height + vertical,
            None => content_height + vertical,
        };

        let padding_box = Rect::new(
            border_x + border.left,
            border_y + border.top,
            (border_width - border.left - border.right).max(0.0),
            (border_height - border.top - border.bottom).max(0.0),
        );
        let (scroll_max_x, scroll_max_y) = self.scroll_max_for(&style, &child_indices, padding_box);
        let (z_tier, z_index) = paint_key(&style);

        let index = self.push(LayoutBox {
            node: Some(node),
            style_node: node,
            kind: match style.display {
                Display::Flex | Display::InlineFlex => BoxKind::Flex,
                Display::Inline | Display::InlineBlock => BoxKind::Inline,
                _ => BoxKind::Block,
            },
            rect: Rect::new(border_x, border_y, border_width, border_height),
            text: None,
            margin,
            padding,
            border,
            clips: resolve::clips(&style),
            scroll_max_x,
            scroll_max_y,
            scroll_x: 0.0,
            scroll_y: 0.0,
            z_tier,
            z_index,
            children: child_indices,
        });

        match self
            .document
            .tree()
            .parent(node)
            .and_then(|parent| self.tree_index(parent))
        {
            Some(parent_index) => self.tree.boxes[parent_index].children.push(index),
            None => self.tree.roots.push(index),
        }

        Some(index)
    }
}

/// The paint-order key of a box within its parent: negative-`z-index` positioned
/// boxes first, then in-flow content, then positioned boxes with `z-index`
/// `0`/`auto` or positive (ordered by `z-index` among themselves).
fn paint_key(style: &ComputedStyle) -> (u8, i32) {
    if style.position == Position::Static {
        return (1, 0);
    }
    match style.z_index {
        ZIndex::Auto => (2, 0),
        ZIndex::Number(z) if z < 0 => (0, z),
        ZIndex::Number(z) => (2, z),
    }
}

/// The child nodes of `node` that produce boxes: elements and text.
pub(crate) fn child_nodes(document: &Document, node: NodeId) -> Vec<NodeId> {
    document
        .tree()
        .children(node)
        .iter()
        .copied()
        .filter(|&child| {
            matches!(
                document.node(child),
                Some(NodeData::Element(_)) | Some(NodeData::Text(_))
            )
        })
        .collect()
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;
