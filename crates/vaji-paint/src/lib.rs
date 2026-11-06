//! # vaji-paint
//!
//! Turns layout output into a stream of render commands: fill a box, stroke an
//! edge, draw a glyph run, apply a shadow or gradient, clip, composite.
//!
//! ## Why a command stream
//!
//! The stream is backend-neutral. It is produced once and may be consumed by a
//! GPU rasterizer, an SVG writer, or a test that only records what *would* be
//! drawn. Making the diff-able artifact (the commands) the interface means a
//! rendering bug can be caught without a screenshot and a pixel diff.
//!
//! This crate also owns the **scene-effects / dirty-region** seam that the
//! replaced engine carried in a forked `anyrender_vello`: a consumer compositor
//! needs to hear "this region changed" to re-render cheaply. Here that is a
//! first-class concept rather than a fork.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A single drawing instruction in a backend-neutral command stream.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintCommand {
    /// Fill an axis-aligned rectangle, in device pixels.
    FillRect {
        /// `[x, y, width, height]`.
        rect: [f32; 4],
        /// Premultiplied RGBA.
        color: [u8; 4],
    },
    /// Push a clip rectangle.
    PushClip([f32; 4]),
    /// Pop the most recent clip.
    PopClip,
}

/// An ordered list of [`PaintCommand`]s.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CommandList(pub Vec<PaintCommand>);

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_list_records_order() {
        let mut list = CommandList::default();
        list.0.push(PaintCommand::PushClip([0.0, 0.0, 10.0, 10.0]));
        list.0.push(PaintCommand::FillRect {
            rect: [1.0, 1.0, 2.0, 2.0],
            color: [0, 0, 0, 255],
        });
        list.0.push(PaintCommand::PopClip);
        assert_eq!(list.0.len(), 3);
    }
}
