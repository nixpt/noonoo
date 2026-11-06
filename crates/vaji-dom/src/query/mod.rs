//! Queries against the tree.
//!
//! Reads are separated from writes on purpose. A query never mutates, so it can
//! run against a shared tree while a mutation session is closed — the split the
//! replaced engine blurred, where reads and writes lived on the same handle and
//! a read could trigger a style snapshot that panicked on a never-styled node.
//!
//! Selector matching itself arrives with [`vaji-style`](../../vaji-style), which
//! owns the CSS engine; this module will expose the Vaji-shaped query surface
//! (`query_selector`, `query_selector_all`, `get_element_by_id`, ...) over it.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.
