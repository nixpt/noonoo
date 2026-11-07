//! # noonoo-accesskit
//!
//! The opt-in bridge from Noonoo's owned accessibility tree to **AccessKit**.
//!
//! Everything semantic lives in [`noonoo_a11y`]: roles, names, descriptions,
//! values, states, bounds. This crate is a projection — it maps that model onto
//! AccessKit's protocol types so a platform adapter (AT-SPI on Linux, UiKit on
//! macOS, WinRT on Windows) can consume it. It is excluded from the workspace
//! so the engine's default build and lockfile stay dependency-free.
//!
//! The platform adapters are host concerns, not engine concerns: a window
//! backend calls [`tree_update`] whenever the tree changes and hands the result
//! to `accesskit_unix`/`accesskit_macos`/`accesskit_windows`. That keeps the
//! "a11y works by default" promise about the *tree* without making every
//! build pay for the borrowed protocol stack.
//!
//! ```ignore
//! let tree = host.a11y();
//! let update = noonoo_accesskit::tree_update(&tree);
//! // adapter.process_update(update);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use accesskit::{Action, Node, NodeId, Rect, Role, Toggled, Tree, TreeId, TreeUpdate};
use noonoo_a11y::{A11yNode, A11yTree, Checked, Role as NoonooRole};

/// Project a Noonoo accessibility tree into one AccessKit tree update.
///
/// The update is a full snapshot: every node is included, so applying it to a
/// fresh adapter yields the current tree. Focus falls back to the root when no
/// node is focused, as AccessKit requires.
pub fn tree_update(tree: &A11yTree) -> TreeUpdate {
    let mut nodes = Vec::with_capacity(tree.count());
    collect(&tree.root, &mut nodes);
    let root = NodeId(tree.root.id);
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(root)),
        tree_id: TreeId::ROOT,
        focus: tree.focus.map_or(root, NodeId),
    }
}

fn collect(node: &A11yNode, out: &mut Vec<(NodeId, Node)>) {
    out.push((NodeId(node.id), project(node)));
    for child in &node.children {
        collect(child, out);
    }
}

fn project(node: &A11yNode) -> Node {
    let mut out = Node::new(role(node.role));

    if !node.name.is_empty() {
        out.set_label(node.name.clone());
    }
    if !node.description.is_empty() {
        out.set_description(node.description.clone());
    }
    if let Some(value) = &node.value {
        out.set_value(value.clone());
    }
    if let Some(level) = node.level {
        out.set_level(usize::from(level));
    }
    if let Some(bounds) = node.bounds {
        out.set_bounds(Rect {
            x0: f64::from(bounds.x),
            y0: f64::from(bounds.y),
            x1: f64::from(bounds.right()),
            y1: f64::from(bounds.bottom()),
        });
    }

    let states = node.states;
    if states.disabled {
        out.set_disabled();
    }
    if states.selected {
        out.set_selected(true);
    }
    if states.required {
        out.set_required();
    }
    if states.read_only {
        out.set_read_only();
    }
    if states.busy {
        out.set_busy();
    }
    if let Some(expanded) = states.expanded {
        out.set_expanded(expanded);
    }
    if let Some(checked) = states.checked {
        out.set_toggled(match checked {
            Checked::False => Toggled::False,
            Checked::True => Toggled::True,
            Checked::Mixed => Toggled::Mixed,
        });
    }

    if is_actionable(node.role) {
        out.add_action(Action::Focus);
        out.add_action(Action::Click);
    }

    for child in &node.children {
        out.push_child(NodeId(child.id));
    }

    out
}

fn is_actionable(role: NoonooRole) -> bool {
    matches!(
        role,
        NoonooRole::Button
            | NoonooRole::Link
            | NoonooRole::CheckBox
            | NoonooRole::RadioButton
            | NoonooRole::Switch
            | NoonooRole::Tab
            | NoonooRole::MenuItem
            | NoonooRole::ListBoxOption
            | NoonooRole::TextBox
            | NoonooRole::ComboBox
            | NoonooRole::ListBox
            | NoonooRole::Slider
            | NoonooRole::SpinButton
    )
}

fn role(role: NoonooRole) -> Role {
    match role {
        NoonooRole::Window => Role::Window,
        NoonooRole::Document => Role::Document,
        NoonooRole::Generic => Role::GenericContainer,
        NoonooRole::Heading => Role::Heading,
        NoonooRole::Paragraph => Role::Paragraph,
        NoonooRole::StaticText => Role::TextRun,
        NoonooRole::Link => Role::Link,
        NoonooRole::Button => Role::Button,
        NoonooRole::Image => Role::Image,
        NoonooRole::TextBox => Role::TextInput,
        NoonooRole::CheckBox => Role::CheckBox,
        NoonooRole::RadioButton => Role::RadioButton,
        NoonooRole::Slider => Role::Slider,
        NoonooRole::SpinButton => Role::SpinButton,
        NoonooRole::ComboBox => Role::ComboBox,
        NoonooRole::ListBox => Role::ListBox,
        NoonooRole::ListBoxOption => Role::ListBoxOption,
        NoonooRole::List => Role::List,
        NoonooRole::ListItem => Role::ListItem,
        NoonooRole::Table => Role::Table,
        NoonooRole::TableRow => Role::Row,
        NoonooRole::TableCell => Role::Cell,
        NoonooRole::ColumnHeader => Role::ColumnHeader,
        NoonooRole::RowHeader => Role::RowHeader,
        NoonooRole::RowGroup => Role::RowGroup,
        NoonooRole::Navigation => Role::Navigation,
        NoonooRole::Main => Role::Main,
        NoonooRole::Banner => Role::Banner,
        NoonooRole::ContentInfo => Role::ContentInfo,
        NoonooRole::Complementary => Role::Complementary,
        NoonooRole::Region => Role::Region,
        NoonooRole::Article => Role::Article,
        NoonooRole::Form => Role::Form,
        NoonooRole::Label => Role::Label,
        NoonooRole::Group => Role::Group,
        NoonooRole::Legend => Role::Legend,
        NoonooRole::Dialog => Role::Dialog,
        NoonooRole::Alert => Role::Alert,
        NoonooRole::Status => Role::Status,
        NoonooRole::ProgressBar => Role::ProgressIndicator,
        NoonooRole::Separator => Role::Splitter,
        NoonooRole::LineBreak => Role::LineBreak,
        NoonooRole::TabList => Role::TabList,
        NoonooRole::Tab => Role::Tab,
        NoonooRole::TabPanel => Role::TabPanel,
        NoonooRole::Toolbar => Role::Toolbar,
        NoonooRole::Menu => Role::Menu,
        NoonooRole::MenuItem => Role::MenuItem,
        NoonooRole::Switch => Role::Switch,
        NoonooRole::Search => Role::Search,
        NoonooRole::Figure => Role::Figure,
        NoonooRole::Caption => Role::FigureCaption,
    }
}
