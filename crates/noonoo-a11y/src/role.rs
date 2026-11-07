//! The role vocabulary and the HTML/ARIA mapping.
//!
//! The roles are Noonoo's own small set, chosen to cover the elements a page
//! actually uses and to map cleanly onto a platform bridge (see
//! `integrations/noonoo-accesskit`). Explicit `role=` values win over the tag
//! mapping; an unknown role falls back to the tag rather than hiding content.

use noonoo_dom::ElementData;

/// What an accessible node *is*.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// The window root composed from several documents.
    Window,
    /// A document root.
    Document,
    /// A container with no more specific role.
    Generic,
    /// A heading; its level is on the node.
    Heading,
    /// A paragraph.
    Paragraph,
    /// A run of static text.
    StaticText,
    /// A link.
    Link,
    /// A button.
    Button,
    /// An image.
    Image,
    /// A single-line text field.
    TextBox,
    /// A checkbox.
    CheckBox,
    /// A radio button.
    RadioButton,
    /// A slider.
    Slider,
    /// A number field with spin buttons.
    SpinButton,
    /// A combo box (`<select>`).
    ComboBox,
    /// A list box.
    ListBox,
    /// One option in a list box.
    ListBoxOption,
    /// A list.
    List,
    /// One list item.
    ListItem,
    /// A table.
    Table,
    /// A table row.
    TableRow,
    /// A table cell.
    TableCell,
    /// A column header cell.
    ColumnHeader,
    /// A row header cell.
    RowHeader,
    /// A row group (`<thead>`/`<tbody>`/`<tfoot>`).
    RowGroup,
    /// A navigation landmark.
    Navigation,
    /// The main landmark.
    Main,
    /// A banner landmark.
    Banner,
    /// A content-info landmark.
    ContentInfo,
    /// A complementary landmark.
    Complementary,
    /// A named region.
    Region,
    /// An article.
    Article,
    /// A form landmark.
    Form,
    /// A label.
    Label,
    /// A group (`<fieldset>`).
    Group,
    /// A group's legend.
    Legend,
    /// A dialog.
    Dialog,
    /// An alert.
    Alert,
    /// A status message.
    Status,
    /// A progress indicator.
    ProgressBar,
    /// A separator.
    Separator,
    /// A hard line break.
    LineBreak,
    /// A tab list.
    TabList,
    /// One tab.
    Tab,
    /// A tab panel.
    TabPanel,
    /// A toolbar.
    Toolbar,
    /// A menu.
    Menu,
    /// One menu item.
    MenuItem,
    /// A switch.
    Switch,
    /// A search landmark.
    Search,
    /// A figure.
    Figure,
    /// A figure caption.
    Caption,
}

impl Role {
    /// The stable JSON/bridge spelling of the role.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Document => "document",
            Self::Generic => "generic",
            Self::Heading => "heading",
            Self::Paragraph => "paragraph",
            Self::StaticText => "static-text",
            Self::Link => "link",
            Self::Button => "button",
            Self::Image => "image",
            Self::TextBox => "text-box",
            Self::CheckBox => "check-box",
            Self::RadioButton => "radio-button",
            Self::Slider => "slider",
            Self::SpinButton => "spin-button",
            Self::ComboBox => "combo-box",
            Self::ListBox => "list-box",
            Self::ListBoxOption => "list-box-option",
            Self::List => "list",
            Self::ListItem => "list-item",
            Self::Table => "table",
            Self::TableRow => "table-row",
            Self::TableCell => "table-cell",
            Self::ColumnHeader => "column-header",
            Self::RowHeader => "row-header",
            Self::RowGroup => "row-group",
            Self::Navigation => "navigation",
            Self::Main => "main",
            Self::Banner => "banner",
            Self::ContentInfo => "content-info",
            Self::Complementary => "complementary",
            Self::Region => "region",
            Self::Article => "article",
            Self::Form => "form",
            Self::Label => "label",
            Self::Group => "group",
            Self::Legend => "legend",
            Self::Dialog => "dialog",
            Self::Alert => "alert",
            Self::Status => "status",
            Self::ProgressBar => "progress-bar",
            Self::Separator => "separator",
            Self::LineBreak => "line-break",
            Self::TabList => "tab-list",
            Self::Tab => "tab",
            Self::TabPanel => "tab-panel",
            Self::Toolbar => "toolbar",
            Self::Menu => "menu",
            Self::MenuItem => "menu-item",
            Self::Switch => "switch",
            Self::Search => "search",
            Self::Figure => "figure",
            Self::Caption => "caption",
        }
    }

    /// Whether an element with this role may take its name from its contents.
    ///
    /// Container roles (`generic`, landmarks) do not: their text is exposed as
    /// static-text children instead, so naming them too would duplicate it.
    pub fn names_from_contents(self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Link
                | Self::Heading
                | Self::ListItem
                | Self::TableCell
                | Self::ColumnHeader
                | Self::RowHeader
                | Self::ListBoxOption
                | Self::Tab
                | Self::MenuItem
                | Self::Label
                | Self::Caption
                | Self::Legend
        )
    }

    /// Whether the role is a checkable control.
    pub fn is_checkable(self) -> bool {
        matches!(self, Self::CheckBox | Self::RadioButton | Self::Switch)
    }

    /// Map an explicit ARIA `role` token, if Noonoo models it.
    pub fn from_aria(value: &str) -> Option<Self> {
        Some(match value.trim().to_ascii_lowercase().as_str() {
            "window" => Self::Window,
            "document" => Self::Document,
            "generic" => Self::Generic,
            "heading" => Self::Heading,
            "paragraph" => Self::Paragraph,
            "link" => Self::Link,
            "button" => Self::Button,
            "img" | "image" => Self::Image,
            "textbox" | "searchbox" => Self::TextBox,
            "checkbox" => Self::CheckBox,
            "radio" => Self::RadioButton,
            "slider" => Self::Slider,
            "spinbutton" => Self::SpinButton,
            "combobox" => Self::ComboBox,
            "listbox" => Self::ListBox,
            "option" => Self::ListBoxOption,
            "list" => Self::List,
            "listitem" => Self::ListItem,
            "table" => Self::Table,
            "row" => Self::TableRow,
            "cell" => Self::TableCell,
            "columnheader" => Self::ColumnHeader,
            "rowheader" => Self::RowHeader,
            "rowgroup" => Self::RowGroup,
            "navigation" => Self::Navigation,
            "main" => Self::Main,
            "banner" => Self::Banner,
            "contentinfo" => Self::ContentInfo,
            "complementary" => Self::Complementary,
            "region" => Self::Region,
            "article" => Self::Article,
            "form" => Self::Form,
            "label" => Self::Label,
            "group" => Self::Group,
            "dialog" | "alertdialog" => Self::Dialog,
            "alert" => Self::Alert,
            "status" => Self::Status,
            "progressbar" => Self::ProgressBar,
            "separator" => Self::Separator,
            "tablist" => Self::TabList,
            "tab" => Self::Tab,
            "tabpanel" => Self::TabPanel,
            "toolbar" => Self::Toolbar,
            "menu" | "menubar" => Self::Menu,
            "menuitem" => Self::MenuItem,
            "switch" => Self::Switch,
            "search" => Self::Search,
            "figure" => Self::Figure,
            "caption" => Self::Caption,
            _ => return None,
        })
    }
}

/// Tags whose content never reaches the accessibility tree.
pub const EXCLUDED_TAGS: &[&str] = &[
    "head", "title", "meta", "link", "base", "script", "style", "template", "noscript", "param",
    "source", "track",
];

/// Whether `tag` is excluded from the accessibility tree entirely.
pub fn is_excluded_tag(tag: &str) -> bool {
    EXCLUDED_TAGS.contains(&tag)
}

/// The role an element carries, before name/state computation.
///
/// An explicit `role=` wins when Noonoo models it. `role="none"` and
/// `role="presentation"` are *not* roles here — [`is_presentational`] reports
/// them so the builder can promote the element's children.
pub fn role_for(element: &ElementData) -> Role {
    if let Some(role) = element.attr("role").and_then(Role::from_aria) {
        return role;
    }

    match element.name.as_str() {
        "html" | "body" => Role::Generic,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Role::Heading,
        "p" => Role::Paragraph,
        "a" | "area" => {
            if element.attr("href").is_some() {
                Role::Link
            } else {
                Role::Generic
            }
        }
        "button" | "summary" => Role::Button,
        "img" | "svg" => Role::Image,
        "input" => match input_type(element) {
            "checkbox" => Role::CheckBox,
            "radio" => Role::RadioButton,
            "range" => Role::Slider,
            "number" => Role::SpinButton,
            "button" | "submit" | "reset" | "image" | "file" | "color" => Role::Button,
            "hidden" => Role::Generic,
            _ => Role::TextBox,
        },
        "textarea" => Role::TextBox,
        "select" => {
            if element.attr("multiple").is_some() {
                Role::ListBox
            } else {
                Role::ComboBox
            }
        }
        "option" => Role::ListBoxOption,
        "ul" | "ol" | "menu" => Role::List,
        "li" => Role::ListItem,
        "table" => Role::Table,
        "thead" | "tbody" | "tfoot" => Role::RowGroup,
        "tr" => Role::TableRow,
        "td" => Role::TableCell,
        "th" => {
            if element.attr("scope") == Some("row") {
                Role::RowHeader
            } else {
                Role::ColumnHeader
            }
        }
        "nav" => Role::Navigation,
        "main" => Role::Main,
        "header" => Role::Banner,
        "footer" => Role::ContentInfo,
        "aside" => Role::Complementary,
        "section" => {
            if element.attr("aria-label").is_some()
                || element.attr("aria-labelledby").is_some()
                || element.attr("title").is_some()
            {
                Role::Region
            } else {
                Role::Generic
            }
        }
        "article" => Role::Article,
        "form" => Role::Form,
        "label" => Role::Label,
        "fieldset" => Role::Group,
        "legend" => Role::Legend,
        "dialog" => Role::Dialog,
        "hr" => Role::Separator,
        "br" => Role::LineBreak,
        "progress" => Role::ProgressBar,
        "search" => Role::Search,
        "figure" => Role::Figure,
        "figcaption" => Role::Caption,
        _ => Role::Generic,
    }
}

/// Whether the element is presentational: it contributes no node, and its
/// children are promoted to the nearest exposed ancestor.
pub fn is_presentational(element: &ElementData) -> bool {
    match element.attr("role") {
        Some("none" | "presentation") => true,
        Some(_) => false,
        None => {
            element.name == "img"
                && element.attr("alt") == Some("")
                && element.attr("aria-label").is_none()
                && element.attr("aria-labelledby").is_none()
        }
    }
}

/// The `type` of an `<input>`, defaulting to `text`.
pub fn input_type(element: &ElementData) -> &str {
    element.attr("type").unwrap_or("text")
}
