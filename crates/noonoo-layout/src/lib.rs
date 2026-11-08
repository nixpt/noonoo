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
//!   `sticky` stays in flow and is moved by scrolling to keep its insets in
//!   its nearest scrolling ancestor's scrollport, or the viewport at the
//!   host's page scroll ([`LayoutTree::set_page_scroll`]), within its
//!   containing block.
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
//!   position within it); loose text becomes anonymous items — runs of text
//!   merge, inherit the container's style, and wrap in their cell.
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
//!   containing block (the viewport for the root; percentages behave as
//!   `auto` against an indefinite container), clamped by
//!   `min-height`/`max-height`.
//! - `box-sizing`, margins, padding, and borders, resolved to absolute pixels.
//!
//! ## What it does not (yet)
//!
//! Full stacking contexts (a positioned box is ordered within its
//! parent, not hoisted), scrollbar *rendering* (content scrolls; no bars are
//! drawn), `justify-items`
//! positioning in grid areas, and writing modes. Floats are ignored for flex
//! and grid items, per CSS. Percentage
//! tracks on an indefinite axis behave as `auto`. Text is measured through the [`Shaper`] seam — the heuristic
//! `MetricShaper` stands in unless the host provides real font metrics.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod block;
mod flex;
mod float;
mod grid;
pub mod image;
mod margin;
mod replaced;
mod resolve;
mod runs;
mod table;

use std::collections::{HashMap, HashSet};

use noonoo_dom::{Document, NodeData, NodeId};
use noonoo_geometry::{Point, Rect, Size};
use noonoo_style::{
    BoxSizing, ComputedStyle, Dimension, Display, Edges, Position, PseudoElement, Styles,
    WhiteSpace, ZIndex,
};

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

/// `text` split at the newlines `white_space` preserves (`pre`,
/// `pre-wrap`, `pre-line`): each piece starts a fresh line. Other values
/// collapse a newline like any space, so the text stays one piece.
pub(crate) fn forced_lines(text: &str, white_space: WhiteSpace) -> Vec<&str> {
    match white_space {
        WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::PreLine => text.split('\n').collect(),
        WhiteSpace::Normal | WhiteSpace::NoWrap => vec![text],
    }
}

/// Whether `element` is a single-line text field: an `<input>` whose type
/// takes typed text (the default type is `text`).
fn is_text_field(element: &noonoo_dom::ElementData) -> bool {
    element.name == "input"
        && matches!(
            element
                .attr("type")
                .map(str::to_ascii_lowercase)
                .as_deref()
                .unwrap_or("text"),
            "text" | "search" | "email" | "url" | "tel" | "password" | "number"
        )
}

/// Whether `display` makes an *atomic* inline-level box: one that sits on a
/// line like a word but is a box of its own, with its own width, height,
/// padding, border, and interior layout (`inline-block`, `inline-flex`,
/// `inline-grid`, and through the UA sheet, form controls and images).
pub(crate) fn is_atomic_inline(display: Display) -> bool {
    matches!(
        display,
        Display::InlineBlock | Display::InlineFlex | Display::InlineGrid
    )
}

/// The result of a layout pass: every box, plus a node → box index.
#[derive(Clone, Debug, Default)]
pub struct LayoutTree {
    boxes: Vec<LayoutBox>,
    index: HashMap<NodeId, usize>,
    roots: Vec<usize>,
    /// Generated (`::before`/`::after`) element and text ids → the element
    /// they belong to.
    generated: HashMap<NodeId, NodeId>,
    /// The `position: sticky` boxes, in creation order.
    sticky: Vec<StickyBox>,
    /// The viewport laid out for: a sticky box with no scrolling ancestor
    /// sticks to it.
    viewport: Size,
    /// How far the host has scrolled the page (see
    /// [`LayoutTree::set_page_scroll`]).
    page_scroll: Point,
    /// The image each `<img>` with a loaded source draws.
    images: HashMap<NodeId, EncodedImage>,
}

/// A `position: sticky` box and how far it's currently shifted from its
/// place in flow.
#[derive(Clone, Debug)]
struct StickyBox {
    index: usize,
    inset: noonoo_style::Edges<Dimension>,
    offset: Point,
}

impl LayoutTree {
    /// The image `node` draws over its content box, if it's an `<img>`
    /// whose source was loaded (see [`layout_with_images`]).
    pub fn image(&self, node: NodeId) -> Option<&EncodedImage> {
        self.images.get(&node)
    }

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
        // Generated content isn't in the DOM; a hit on it is a hit on its
        // element, as in a browser.
        best.map(|(_, node)| self.generated.get(&node).copied().unwrap_or(node))
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

    /// Where `node` is drawn (`getBoundingClientRect`): the rectangle around
    /// every box it produced (a wrapped text node has one per line), moved
    /// by the scroll offsets of the boxes it scrolls with. `None` if it
    /// produced no box.
    ///
    /// The coordinates are layout space: a host that scrolls the whole page
    /// itself subtracts that scroll.
    pub fn bounding_client_rect(&self, node: NodeId) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        let mut stack: Vec<(usize, Point)> =
            self.roots.iter().map(|&root| (root, Point::ZERO)).collect();
        while let Some((index, scroll)) = stack.pop() {
            let box_ = &self.boxes[index];
            if box_.node == Some(node) {
                let rect = box_.rect.translate(-scroll.x, -scroll.y);
                bounds = Some(match bounds {
                    None => rect,
                    Some(seen) => {
                        let (x, y) = (seen.x.min(rect.x), seen.y.min(rect.y));
                        Rect::new(
                            x,
                            y,
                            seen.right().max(rect.right()) - x,
                            seen.bottom().max(rect.bottom()) - y,
                        )
                    }
                });
            }
            let inner = Point::new(scroll.x + box_.scroll_x, scroll.y + box_.scroll_y);
            stack.extend(box_.children.iter().map(|&child| (child, inner)));
        }
        bounds
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
        self.update_sticky();
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
        self.update_sticky();
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
        self.page_scroll = previous.page_scroll;
        self.update_sticky();
    }

    /// Tell the tree how far the host has scrolled the page, for hosts that
    /// scroll by moving the painted output: sticky boxes with no scrolling
    /// ancestor stick to the viewport at that scroll.
    pub fn set_page_scroll(&mut self, scroll: Point) {
        self.page_scroll = scroll;
        self.update_sticky();
    }

    /// Move each sticky box (and its subtree) to where the current scroll
    /// offsets put it (CSS Positioned Layout §3.4): as little as keeps its
    /// insets inside its nearest scrolling ancestor's scrollport — or the
    /// viewport — but never out of its containing block, and back to its
    /// place in flow when nothing pushes it.
    fn update_sticky(&mut self) {
        if self.sticky.is_empty() {
            return;
        }
        // Each box's nearest scrolling ancestor and parent, parents first.
        let mut order = Vec::new();
        let mut context: HashMap<usize, (Option<usize>, Option<usize>)> = HashMap::new();
        let mut stack: Vec<(usize, Option<usize>, Option<usize>)> = self
            .roots
            .iter()
            .rev()
            .map(|&root| (root, None, None))
            .collect();
        while let Some((index, scroller, parent)) = stack.pop() {
            order.push(index);
            context.insert(index, (scroller, parent));
            let inner = if self.boxes[index].clips {
                Some(index)
            } else {
                scroller
            };
            for &child in self.boxes[index].children.iter().rev() {
                stack.push((child, inner, Some(index)));
            }
        }
        let rank: HashMap<usize, usize> = order.iter().enumerate().map(|(k, &i)| (i, k)).collect();
        let mut sticky = std::mem::take(&mut self.sticky);
        sticky.sort_by_key(|item| rank.get(&item.index).copied().unwrap_or(usize::MAX));
        for item in &mut sticky {
            let Some(&(scroller, parent)) = context.get(&item.index) else {
                continue;
            };
            // The visible window onto the scroller's content, in layout space.
            let port = match scroller {
                Some(s) => {
                    let s = &self.boxes[s];
                    s.padding_box().translate(s.scroll_x, s.scroll_y)
                }
                None => Rect::new(
                    self.page_scroll.x,
                    self.page_scroll.y,
                    self.viewport.width,
                    self.viewport.height,
                ),
            };
            let limit = parent.map(|p| {
                let p = &self.boxes[p];
                p.padding_box().inset(
                    p.padding.top,
                    p.padding.right,
                    p.padding.bottom,
                    p.padding.left,
                )
            });
            // Where flow put it: its rect less the shift already applied.
            let rect = self.boxes[item.index]
                .rect
                .translate(-item.offset.x, -item.offset.y);
            let inset = crate::resolve::insets(&item.inset, port.width, port.height);
            let place = |start: f32,
                         size: f32,
                         port_start: f32,
                         port_end: f32,
                         near: Option<f32>,
                         far: Option<f32>,
                         limit: Option<(f32, f32)>| {
                let mut at = start;
                if let Some(far) = far {
                    at = at.min(port_end - far - size);
                }
                if let Some(near) = near {
                    at = at.max(port_start + near);
                }
                if let Some((low, high)) = limit {
                    // Pushed down (or right) it stops at the block's end;
                    // pulled up (or left), at its start.
                    if at > start {
                        at = at.min((high - size).max(start));
                    } else {
                        at = at.max(low.min(start));
                    }
                }
                at
            };
            let y = place(
                rect.y,
                rect.height,
                port.y,
                port.bottom(),
                inset.top,
                inset.bottom,
                limit.map(|l| (l.y, l.bottom())),
            );
            let x = place(
                rect.x,
                rect.width,
                port.x,
                port.right(),
                inset.left,
                inset.right,
                limit.map(|l| (l.x, l.right())),
            );
            let offset = Point::new(x - rect.x, y - rect.y);
            let (dx, dy) = (offset.x - item.offset.x, offset.y - item.offset.y);
            if dx != 0.0 || dy != 0.0 {
                self.shift(item.index, dx, dy);
            }
            item.offset = offset;
        }
        self.sticky = sticky;
    }

    /// Move box `index` and every box under it by `(dx, dy)`.
    fn shift(&mut self, index: usize, dx: f32, dy: f32) {
        let mut stack = vec![index];
        while let Some(i) = stack.pop() {
            let b = &mut self.boxes[i];
            b.rect = b.rect.translate(dx, dy);
            stack.extend(b.children.iter().copied());
        }
    }
}

/// Lay out `document` for the given `viewport`.
///
/// `::before`/`::after` boxes with content ([`Styles::generated`]) are laid
/// out from a copy of the document that holds them as real nodes; the
/// document itself is untouched.
pub fn layout(
    document: &Document,
    styles: &Styles,
    viewport: Size,
    shaper: &dyn Shaper,
) -> LayoutTree {
    layout_with_images(document, styles, viewport, shaper, &Images::new())
}

/// [`layout`], with `images` holding the loaded `<img>` sources: an image
/// whose `src` is there is sized from it and drawn (see
/// [`LayoutTree::image`]); one that isn't shows its `alt` text.
pub fn layout_with_images(
    document: &Document,
    styles: &Styles,
    viewport: Size,
    shaper: &dyn Shaper,
    images: &Images,
) -> LayoutTree {
    let with_generated = with_generated_content(document, styles);
    let document = with_generated.as_ref().unwrap_or(document);
    let mut ctx = Ctx {
        document,
        styles,
        shaper,
        viewport,
        images,
        pending: Vec::new(),
        floats: Vec::new(),
        tree: LayoutTree::default(),
        margins: Default::default(),
        measured: HashMap::new(),
        measuring: 0,
        preferred: Default::default(),
        min_content: Default::default(),
        // The initial containing block: percentages on the root resolve
        // against the viewport's height.
        heights: vec![Some(viewport.height)],
        flexed: None,
        flexed_width: None,
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
    for generated in styles.generated() {
        ctx.tree
            .generated
            .insert(generated.element, generated.owner);
        ctx.tree.generated.insert(generated.text, generated.owner);
    }
    ctx.tree.viewport = viewport;
    ctx.tree.update_sticky();
    ctx.tree
}

/// A copy of `document` with each generated box inserted where its
/// pseudo-element goes: an element holding one text node, first (`::before`)
/// or last (`::after`) in its owner. `None` when there is nothing to insert,
/// or when `styles` was computed for another document (the reserved ids
/// don't line up).
fn with_generated_content(document: &Document, styles: &Styles) -> Option<Document> {
    let generated = styles.generated();
    if generated.is_empty() {
        return None;
    }
    let mut copy = document.clone();
    for item in generated {
        let element = copy.create_element(match item.pseudo {
            PseudoElement::Before => "::before",
            PseudoElement::After => "::after",
        });
        let text = copy.create_text(item.content.clone());
        if element != item.element || text != item.text || copy.element(item.owner).is_none() {
            return None;
        }
        let mut mutate = copy.mutate();
        mutate.append_child(element, text).ok()?;
        let first = mutate.tree().children(item.owner).first().copied();
        match (item.pseudo, first) {
            (PseudoElement::Before, Some(first)) => mutate.insert_before(first, element).ok()?,
            _ => mutate.append_child(item.owner, element).ok()?,
        }
    }
    Some(copy)
}

/// What a block formatting context root's height depends on, as bits so
/// it can key a map.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct MeasureKey {
    node: NodeId,
    available_width: u32,
    definite_height: Option<u32>,
    percent_basis: Option<u32>,
    /// A column flex container's decided height for the node.
    flexed: Option<u32>,
    /// A row flex container's decided width for the node.
    flexed_width: Option<u32>,
}

/// Shared layout state.
pub(crate) struct Ctx<'a> {
    pub document: &'a Document,
    pub styles: &'a Styles,
    pub shaper: &'a dyn Shaper,
    pub viewport: Size,
    /// Loaded `<img>` sources.
    pub images: &'a Images,
    /// Out-of-flow nodes awaiting pass 2.
    pub pending: Vec<NodeId>,
    /// The block formatting context stack; the top holds the floats visible to
    /// the box being laid out.
    pub floats: Vec<crate::float::Floats>,
    pub tree: LayoutTree,
    /// Collapsed vertical margins, which depend only on the document, the
    /// styles and the containing width, so they are computed once per layout.
    pub margins: crate::margin::Cache,
    /// Measured heights of block formatting context roots, by their inputs.
    pub measured: HashMap<MeasureKey, f32>,
    /// How many [`Ctx::measure_height`] calls are under way: their boxes
    /// are thrown away.
    pub measuring: usize,
    /// [`float::preferred_width`] results by node.
    pub preferred: std::cell::RefCell<HashMap<NodeId, f32>>,
    /// [`float::min_content_width`] results by node.
    pub min_content: std::cell::RefCell<HashMap<NodeId, f32>>,
    /// The containing blocks' definite content heights, innermost last
    /// (`None` where it is indefinite): what a percentage height resolves
    /// against.
    pub heights: Vec<Option<f32>>,
    /// A column flex container's decided main size for one item (its border
    /// box): unlike a stretched cross size, it overrides a specified height.
    pub flexed: Option<(NodeId, f32)>,
    /// A row flex container's decided main size for one item (its border
    /// box): it overrides a specified width, as `flexed` does a height.
    pub flexed_width: Option<(NodeId, f32)>,
}

impl Ctx<'_> {
    /// The definite content height a percentage height resolves against, if
    /// the containing block has one.
    pub(crate) fn percent_basis(&self) -> Option<f32> {
        self.heights.last().copied().flatten()
    }

    /// How wide `text` is in `style`: after `text-transform`, plus
    /// `letter-spacing` after every character. Every text measurement in
    /// layout goes through here, so measuring and painting agree.
    pub(crate) fn advance(&self, text: &str, style: &ComputedStyle) -> f32 {
        let text = style.transformed_text(text);
        let width = self
            .shaper
            .advance(&text, style.font_request(), style.font_size);
        if style.letter_spacing == 0.0 {
            width
        } else {
            width + style.letter_spacing * text.chars().count() as f32
        }
    }

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

    /// A copy of `node`'s style on the heap. The layout functions that
    /// recurse once per nesting level hold their style this way: a style
    /// is large, and a copy in each of their frames would overflow the
    /// stack on deep trees (out of line, so the copy's temporary lives in
    /// this short frame instead).
    #[inline(never)]
    pub(crate) fn boxed_style(&self, node: NodeId) -> Box<ComputedStyle> {
        Box::new(self.style(node).clone())
    }

    pub fn style(&self, node: NodeId) -> &ComputedStyle {
        self.styles
            .get(node)
            .expect("layout only visits styled elements")
    }

    pub fn is_text(&self, node: NodeId) -> bool {
        matches!(self.document.node(node), Some(NodeData::Text(_)))
    }

    /// Whether `node` is inline-level: text, an inline element, or an atomic
    /// inline (see [`is_atomic_inline`]).
    pub fn is_inline_level(&self, node: NodeId) -> bool {
        if self.is_text(node) {
            return true;
        }
        let display = self.style(node).display;
        display == Display::Inline || is_atomic_inline(display)
    }

    /// The text a childless replaced element shows in place of its content
    /// (`<img alt>`, `<input value>`), if any.
    pub(crate) fn placeholder(&self, node: NodeId) -> Option<String> {
        let element = self.document.element(node)?;
        if !self.document.tree().children(node).is_empty() {
            return None;
        }
        let text = element.placeholder_text();
        // A text field is a line tall even when it's empty.
        (!text.is_empty() || is_text_field(element)).then(|| text.to_owned())
    }

    /// The line height `style` asks for: its own, or for `normal`, the
    /// font's natural one (ascent, descent and line gap) from the shaper.
    pub(crate) fn line_height_of(&self, style: &ComputedStyle) -> f32 {
        style.line_height.unwrap_or_else(|| {
            self.shaper
                .line_height(style.font_request(), style.font_size)
        })
    }

    /// A text field's intrinsic content width: `size` characters (20
    /// unless the attribute is a positive integer), each as wide as a "0"
    /// in its font, whatever its value (HTML's rendering of text fields).
    pub(crate) fn text_field_width(&self, node: NodeId, style: &ComputedStyle) -> Option<f32> {
        let element = self.document.element(node)?;
        if !is_text_field(element) {
            return None;
        }
        let size = element
            .attr("size")
            .and_then(|size| size.trim().parse::<u32>().ok())
            .filter(|&size| size > 0)
            .unwrap_or(20);
        let zero = self
            .shaper
            .advance("0", style.font_request(), style.font_size);
        Some(size as f32 * zero)
    }

    /// Lay out a replaced element's placeholder as its content: one line of
    /// text at the content origin. The box carries no node, so the element
    /// keeps its own entry in the node index.
    fn layout_placeholder(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        text: &str,
        content_x: f32,
        content_y: f32,
    ) -> (Vec<usize>, f32) {
        let height = self.line_height_of(style);
        if text.is_empty() {
            return (Vec::new(), height);
        }
        let width = self.advance(text, style);
        let index = self.push(LayoutBox {
            node: None,
            style_node: node,
            kind: BoxKind::Text,
            rect: Rect::new(content_x, content_y, width, height),
            text: Some(text.to_owned()),
            margin: block::no_edges(),
            padding: block::no_edges(),
            border: block::no_edges(),
            clips: false,
            scroll_max_x: 0.0,
            scroll_max_y: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            z_tier: 1,
            z_index: 0,
            children: Vec::new(),
        });
        (vec![index], height)
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
    ///
    /// While [measuring](Ctx::measure_height), the boxes are thrown away, so
    /// a box whose height is already known for these inputs isn't laid out
    /// again: nested containers each measure their children before placing
    /// them, and laying each measured subtree out anew made the work grow
    /// with the square of the depth.
    pub fn layout_element(
        &mut self,
        node: NodeId,
        border_x: f32,
        border_y: f32,
        available_width: f32,
        definite_height: Option<f32>,
    ) -> f32 {
        #[cfg(test)]
        let reuse = REUSE_MEASURED.with(std::cell::Cell::get);
        #[cfg(not(test))]
        let reuse = true;
        let key = (reuse && self.measuring > 0)
            .then(|| self.measure_key(node, available_width, definite_height));
        // (A container's decided size for `node` stays set on a hit; the
        // containers clear it after each item.)
        if let Some(height) = key.and_then(|key| self.measured.get(&key).copied()) {
            return height;
        }
        let height = self.layout_element_uncached(
            node,
            border_x,
            border_y,
            available_width,
            definite_height,
        );
        if let Some(key) = key
            && self.establishes_bfc(node, self.style(node))
        {
            self.measured.insert(key, height);
        }
        height
    }

    /// The inputs a block formatting context root's height depends on.
    fn measure_key(
        &self,
        node: NodeId,
        available_width: f32,
        definite_height: Option<f32>,
    ) -> MeasureKey {
        let decided = |slot: Option<(NodeId, f32)>| {
            slot.filter(|(flexed, _)| *flexed == node)
                .map(|(_, size)| size.to_bits())
        };
        MeasureKey {
            node,
            available_width: available_width.to_bits(),
            definite_height: definite_height.map(f32::to_bits),
            percent_basis: self.percent_basis().map(f32::to_bits),
            flexed: decided(self.flexed),
            flexed_width: decided(self.flexed_width),
        }
    }

    fn layout_element_uncached(
        &mut self,
        node: NodeId,
        border_x: f32,
        border_y: f32,
        available_width: f32,
        definite_height: Option<f32>,
    ) -> f32 {
        #[cfg(test)]
        LAYOUTS.with(|count| count.set(count.get() + 1));
        let style = self.boxed_style(node);
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
        // A row flex item's main size was decided by its container (already
        // clamped by min/max there): it replaces the specified width.
        let flexed_width = self
            .flexed_width
            .take_if(|(flexed, _)| *flexed == node)
            .map(|(_, width)| width);
        let replaced = self.image_size(node, &style, available_width, self.percent_basis());
        let border_box_width = match (flexed_width, replaced) {
            (Some(width), _) => width,
            (None, Some((width, _))) => width + horizontal,
            (None, None) => {
                let mut width = specified.unwrap_or_else(|| available_width.max(0.0));
                if let Some(maximum) = maximum {
                    width = width.min(maximum);
                }
                if let Some(minimum) = minimum {
                    width = width.max(minimum);
                }
                width
            }
        };
        let content_width = (border_box_width - horizontal).max(0.0);

        let (mut border_x, mut border_y) = (border_x, border_y);

        // `relative` shifts the box and its whole subtree, but leaves the
        // box's flow position — and thus its siblings — untouched. (A
        // `sticky` box stays in flow; scrolling moves it, see
        // `LayoutTree::update_sticky`.)
        if style.position == Position::Relative {
            // The containing block's height isn't known in flow; keep
            // resolving vertical percentages against the width, as before.
            let inset = resolve::insets(&style.inset, available_width, available_width);
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

        // `height`, `min-height`, and `max-height` share a box (per
        // `box-sizing`), like the widths above. A percentage resolves against
        // the containing block's definite height, and behaves as `auto` when
        // it has none.
        let to_border_box_height = |height: f32| match style.box_sizing {
            BoxSizing::BorderBox => height,
            BoxSizing::ContentBox => height + vertical,
        };
        let basis = self.percent_basis();
        let height_of = |value: &Dimension| match basis {
            Some(basis) => resolve::dimension(value.clone(), basis),
            None => resolve::definite(value),
        };
        let clamp_height = |height: f32| {
            let mut height = height;
            if let Some(maximum) = height_of(&style.max_height) {
                height = height.min(to_border_box_height(maximum));
            }
            if let Some(minimum) = height_of(&style.min_height) {
                height = height.max(to_border_box_height(minimum));
            }
            height
        };
        let specified_height = height_of(&style.height).map(to_border_box_height);
        // A specified height, or one the container decided (a stretched or
        // flexed item), is definite: the children's percentages use it.
        let flexed = self
            .flexed
            .take_if(|(flexed, _)| *flexed == node)
            .map(|(_, height)| height);
        let definite_border_height = flexed
            .or(replaced.map(|(_, height)| height + vertical))
            .or(specified_height)
            .or(definite_height)
            .map(clamp_height);
        self.heights
            .push(definite_border_height.map(|height| (height - vertical).max(0.0)));

        let establishes_bfc = self.establishes_bfc(node, &style);
        if establishes_bfc {
            self.push_bfc();
        }

        let placeholder = self.placeholder(node);
        let (child_indices, content_height) = match style.display {
            _ if replaced.is_some() => self.record_image(node),
            _ if placeholder.is_some() => self.layout_placeholder(
                node,
                &style,
                placeholder.as_deref().unwrap_or_default(),
                content_x,
                content_y,
            ),
            Display::Flex | Display::InlineFlex => flex::layout_flex_children(
                self,
                node,
                &style,
                &children_nodes,
                content_x,
                content_y,
                content_width,
            ),
            Display::Grid | Display::InlineGrid => crate::grid::layout_grid_children(
                self,
                node,
                &style,
                &children_nodes,
                content_x,
                content_y,
                content_width,
            ),
            Display::Table => table::layout_table_children(
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
        self.heights.pop();

        let border_box_height =
            definite_border_height.unwrap_or_else(|| clamp_height(content_height + vertical));

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
        if style.position == Position::Sticky {
            self.tree.sticky.push(StickyBox {
                index,
                inset: style.inset.clone(),
                offset: Point::ZERO,
            });
        }

        border_box_height
    }

    /// Measure a node's laid-out border-box height by laying it out at
    /// `(x, y)` and discarding the boxes — for flows that must know heights
    /// before they place (flex lines, grid rows). Out-of-flow descendants
    /// discovered during measurement are enqueued; call [`Ctx::prune_pending`]
    /// after a measurement pass.
    ///
    /// A box that starts a block formatting context has a height that
    /// depends only on its width, its definite height, the percentage basis
    /// and any size its container decided for it, so that is remembered for
    /// the rest of the layout (see [`Ctx::layout_element`]).
    pub fn measure_height(
        &mut self,
        node: NodeId,
        x: f32,
        y: f32,
        available_width: f32,
        definite_height: Option<f32>,
    ) -> f32 {
        let base = self.tree.boxes.len();
        self.measuring += 1;
        let height = self.layout_element(node, x, y, available_width, definite_height);
        self.measuring -= 1;
        self.tree.boxes.truncate(base);
        self.tree.index.retain(|_, &mut index| index < base);
        self.tree.sticky.retain(|item| item.index < base);
        height
    }

    /// Whether `node` starts a block formatting context. Floats inside one
    /// stay inside, and floats outside don't reach in: clipping boxes,
    /// floats, out-of-flow boxes, flex/grid containers and their items,
    /// inline-blocks and table cells. Its height is then independent of
    /// where it's placed, which [`Ctx::measure_height`]'s cache relies on.
    fn establishes_bfc(&self, node: NodeId, style: &ComputedStyle) -> bool {
        let in_flex_or_grid = self
            .document
            .tree()
            .parent(node)
            .and_then(|parent| self.styles.get(parent))
            .is_some_and(|parent| {
                matches!(
                    parent.display,
                    Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid
                )
            });
        resolve::clips(style)
            || in_flex_or_grid
            || style.float != noonoo_style::Float::None
            || matches!(style.position, Position::Absolute | Position::Fixed)
            || matches!(
                style.display,
                Display::Flex
                    | Display::InlineFlex
                    | Display::Grid
                    | Display::InlineGrid
                    | Display::InlineBlock
                    | Display::Table
                    | Display::TableCell
            )
    }

    /// Drop out-of-flow nodes enqueued more than once (measurement lays items
    /// out, then the real pass lays them again).
    pub fn prune_pending(&mut self) {
        let mut seen = HashSet::new();
        self.pending.retain(|node| seen.insert(*node));
    }

    /// Move box `index` and every box under it by `(dx, dy)`.
    pub(crate) fn shift_subtree(&mut self, index: usize, dx: f32, dy: f32) {
        let mut stack = vec![index];
        while let Some(i) = stack.pop() {
            let b = &mut self.tree.boxes[i];
            b.rect = b.rect.translate(dx, dy);
            stack.extend(b.children.iter().copied());
        }
    }

    /// Lay out an out-of-flow box (`position: absolute`/`fixed`) against its
    /// containing block, and attach it to the box tree so it paints and
    /// hit-tests alongside its siblings.
    pub fn layout_out_of_flow(&mut self, node: NodeId) -> Option<usize> {
        let style = self.boxed_style(node);
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
        let inset = resolve::insets(&style.inset, cb_width, cb_height);

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
        let replaced = self.image_size(node, &style, cb_width, Some(cb_height));
        let definite_width = replaced.map(|(width, _)| width).or(specified).or(stretched);
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

        // Height: an explicit height, or stretched between `top`+`bottom`;
        // either is definite, so the children's percentages resolve against
        // it.
        let stretched_height = match (inset.top, inset.bottom) {
            (Some(top), Some(bottom)) => {
                Some((cb_height - top - bottom - margin.top - margin.bottom - vertical).max(0.0))
            }
            _ => None,
        };
        let definite_height = replaced
            .map(|(_, height)| height)
            .or(resolve::dimension(style.height.clone(), cb_height))
            .or(stretched_height);
        self.heights.push(definite_height);

        let children_nodes = child_nodes(self.document, node);
        let placeholder = self.placeholder(node);
        let (child_indices, content_height) = match style.display {
            _ if replaced.is_some() => self.record_image(node),
            _ if placeholder.is_some() => self.layout_placeholder(
                node,
                &style,
                placeholder.as_deref().unwrap_or_default(),
                content_x,
                content_y,
            ),
            Display::Flex | Display::InlineFlex => flex::layout_flex_children(
                self,
                node,
                &style,
                &children_nodes,
                content_x,
                content_y,
                content_width,
            ),
            Display::Table => table::layout_table_children(
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
                // Shrink-to-fit: the content was laid out in the available
                // width, so it only reaches past it where it can't wrap (a
                // `nowrap` run, a wide child) — and then the box grows to it.
                used.max(0.0) + horizontal
            }
        };

        self.heights.pop();

        let border_height = match definite_height {
            Some(height) => height + vertical,
            None => content_height + vertical,
        };

        // `right`/`bottom` against an auto size: now that the box is
        // measured, move it (and what it laid out) into place.
        let dx = match (inset.left, inset.right, definite_width) {
            (None, Some(right), None) => {
                containing.right() - right - margin.right - border_width - border_x
            }
            _ => 0.0,
        };
        let dy = match (inset.top, inset.bottom, definite_height) {
            (None, Some(bottom), None) => {
                containing.bottom() - bottom - margin.bottom - border_height - border_y
            }
            _ => 0.0,
        };
        if dx != 0.0 || dy != 0.0 {
            for &child in &child_indices {
                self.shift_subtree(child, dx, dy);
            }
        }
        let (border_x, border_y) = (border_x + dx, border_y + dy);

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

pub use image::{EncodedImage, Image, ImageFormat, Images};

#[cfg(test)]
thread_local! {
    /// How many times `layout_element` ran, for tests that pin layout cost.
    static LAYOUTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Whether measuring reuses known heights; tests turn it off to get
    /// the reference layout.
    static REUSE_MEASURED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

#[cfg(test)]
mod tests;
