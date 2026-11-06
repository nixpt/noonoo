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
//!   `align-items`, gaps, and row `flex-wrap`.
//! - **Positioning** — `position: relative` (offsets its subtree without moving
//!   siblings) and `position: absolute` / `fixed` (out of flow, placed against
//!   the nearest positioned ancestor's padding box — the viewport for `fixed`).
//!   `sticky` is treated as `relative`.
//! - Width: `auto` / px / percentage (and stretch between `left`+`right`);
//!   height: `auto` / px / percentage of the containing block.
//! - `box-sizing`, margins, padding, and borders, resolved to absolute pixels.
//!
//! ## What it does not (yet)
//!
//! Margin collapsing, floats, `grid`, `z-index` paint ordering (positioned boxes
//! paint in tree order), `overflow` clipping, writing modes. Absolute boxes whose
//! width or height is `auto` fall back to the containing block when only the
//! trailing inset (`right`/`bottom`) is set. Text is measured through the
//! [`Shaper`] seam — a heuristic stands in until `noonoo-text` brings real
//! shaping (`docs/ROADMAP.md` Phase C).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod block;
mod flex;
mod resolve;

use std::collections::HashMap;

use noonoo_dom::{Document, NodeData, NodeId};
use noonoo_geometry::{Point, Rect, Size};
use noonoo_style::{BoxSizing, ComputedStyle, Display, Edges, Position, Styles};

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
        for &root in self.roots.iter().rev() {
            self.hit_box(root, point, 0, &mut best);
        }
        best.map(|(_, node)| node)
    }

    fn hit_box(
        &self,
        index: usize,
        point: Point,
        depth: usize,
        best: &mut Option<(usize, NodeId)>,
    ) {
        let box_ = &self.boxes[index];
        // A positioned child may sit outside its parent's box, so we always
        // descend; we only *record* a box whose rect actually contains the point.
        if box_.rect.contains(point)
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
        for &child in &box_.children {
            self.hit_box(child, point, depth + 1, best);
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
        tree: LayoutTree::default(),
    };

    // Pass 1 — normal flow. Out-of-flow boxes are queued, not placed: they do
    // not contribute to their siblings' positions or the parent's height.
    let mut y = 0.0;
    for child in child_nodes(document, document.root()) {
        if ctx.is_hidden(child) {
            continue;
        }
        if ctx.is_out_of_flow(child) {
            ctx.pending.push(child);
            continue;
        }
        if ctx.is_block_level(child) {
            y += ctx.layout_element(child, 0.0, y, viewport.width);
            if let Some(index) = ctx.tree_index(child) {
                ctx.tree.roots.push(index);
            }
        } else {
            let (indices, height) = block::layout_inline_run(
                &mut ctx,
                child,
                &[child],
                0.0,
                y,
                viewport.width,
                &ComputedStyle::default(),
            );
            ctx.tree.roots.extend(indices);
            y += height;
        }
    }

    // Pass 2 — out-of-flow boxes, against their now-laid-out containing block.
    // Laying one out may enqueue nested out-of-flow descendants, so this is a
    // growing, FIFO sweep rather than a fixed loop.
    let mut cursor = 0;
    while cursor < ctx.pending.len() {
        let node = ctx.pending[cursor];
        cursor += 1;
        ctx.layout_out_of_flow(node);
    }

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
    pub tree: LayoutTree,
}

impl Ctx<'_> {
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

    pub fn is_block_level(&self, node: NodeId) -> bool {
        !self.is_inline_level(node) && self.style(node).display != Display::None
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

    /// The border-box height of the box at `index`.
    pub fn box_height(&self, index: usize) -> f32 {
        self.tree.boxes[index].rect.height
    }

    pub fn push(&mut self, b: LayoutBox) -> usize {
        let index = self.tree.boxes.len();
        if let Some(node) = b.node {
            self.tree.index.insert(node, index);
        }
        self.tree.boxes.push(b);
        index
    }

    /// Lay out one element and return its box index and outer height.
    pub fn layout_element(&mut self, node: NodeId, x: f32, y: f32, available_width: f32) -> f32 {
        let style = self.style(node).clone();
        if style.display == Display::None {
            return 0.0;
        }

        let margin = resolve::margins(&style.margin, available_width);
        let padding = resolve::paddings(&style.padding, available_width);
        let border = resolve::borders(&style);

        let horizontal = padding.left + padding.right + border.left + border.right;
        let vertical = padding.top + padding.bottom + border.top + border.bottom;

        let specified_width = resolve::dimension(style.width, available_width);
        let (content_width, border_box_width) = match (specified_width, style.box_sizing) {
            (Some(w), BoxSizing::BorderBox) => ((w - horizontal).max(0.0), w),
            (Some(w), BoxSizing::ContentBox) => (w, w + horizontal),
            (None, _) => {
                let inner = (available_width - horizontal).max(0.0);
                (inner, inner + horizontal)
            }
        };

        let mut origin_x = x + margin.left;
        let mut origin_y = y + margin.top;

        // `relative` (and `sticky`, unsupported as such) shifts the box and its
        // whole subtree, but leaves the box's flow position — and thus its
        // siblings — untouched.
        if matches!(style.position, Position::Relative | Position::Sticky) {
            let inset = resolve::insets(&style.inset, available_width);
            origin_x += inset
                .left
                .or(inset.right.map(|right| -right))
                .unwrap_or(0.0);
            origin_y += inset
                .top
                .or(inset.bottom.map(|bottom| -bottom))
                .unwrap_or(0.0);
        }

        let content_x = origin_x + border.left + padding.left;
        let content_y = origin_y + border.top + padding.top;

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
            _ => block::layout_block_children(
                self,
                node,
                &children_nodes,
                content_x,
                content_y,
                content_width,
                &style,
            ),
        };

        let border_box_height = match resolve::dimension(style.height, f32::INFINITY) {
            Some(h) => h + vertical,
            None => content_height + vertical,
        };

        let index = self.push(LayoutBox {
            node: Some(node),
            style_node: node,
            kind: match style.display {
                Display::Flex | Display::InlineFlex => BoxKind::Flex,
                Display::Inline | Display::InlineBlock => BoxKind::Inline,
                _ => BoxKind::Block,
            },
            rect: Rect::new(origin_x, origin_y, border_box_width, border_box_height),
            text: None,
            margin,
            padding,
            border,
            children: child_indices,
        });

        let _ = index;
        margin.top + border_box_height + margin.bottom
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
            resolve::dimension(style.width, cb_width).map(|width| match style.box_sizing {
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
            match resolve::dimension(style.height, cb_height) {
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
            _ => block::layout_block_children(
                self,
                node,
                &children_nodes,
                content_x,
                content_y,
                content_width,
                &style,
            ),
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

        let border_height = match resolve::dimension(style.height, cb_height) {
            Some(height) => height + vertical,
            None => content_height + vertical,
        };

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
