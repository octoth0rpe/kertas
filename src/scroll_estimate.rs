//! Estimated item heights for the scrollbar (#88).
//!
//! The view only measures the items it draws, so the true height of a long
//! document isn't known until it has all been on screen. The scrollbar
//! works from an estimate instead, made from each item's text at the
//! current line width: rough, but known for every item from the start and
//! unchanged by scrolling, so the thumb doesn't jump or resize as items are
//! measured. Heights are in lines of body text.

use crate::blocks::BlockKind;
use crate::model::Model;
use crate::rendered::heading_scale;

/// The space after each block, in lines (16 px at the default 25.6 px
/// line height).
const BLOCK_SPACING: f32 = 0.625;

/// The estimated height of every item in a document.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollEstimate {
    /// Where each item starts, from the top of the document, followed by
    /// the document's total height.
    starts: Vec<f32>,
}

impl ScrollEstimate {
    /// Estimates `model`'s items, with text wrapping after about
    /// `chars_per_line` characters.
    pub fn new(model: &Model, chars_per_line: usize) -> Self {
        let chars_per_line = chars_per_line.max(1);
        let mut starts = Vec::with_capacity(model.items.len() + 1);
        let mut top = 0.;
        for item in &model.items {
            starts.push(top);
            top += item_height(&model.text[item.block.clone()], item.kind, chars_per_line);
        }
        starts.push(top);
        Self { starts }
    }

    /// The document's estimated height.
    pub fn total(&self) -> f32 {
        self.starts.last().copied().unwrap_or(0.)
    }

    /// How far down the document a point `fraction` (0 to 1) of the way
    /// through item `index` is.
    pub fn position(&self, index: usize, fraction: f32) -> f32 {
        let Some(&start) = self.starts.get(index) else {
            return self.total();
        };
        let end = self.starts.get(index + 1).copied().unwrap_or(start);
        start + (end - start) * fraction.clamp(0., 1.)
    }

    /// The item at `position` from the top, and how far through it (0 to
    /// 1) the position is. Past the end, the last item's end.
    pub fn locate(&self, position: f32) -> (usize, f32) {
        let items = self.starts.len().saturating_sub(1);
        if items == 0 {
            return (0, 0.);
        }
        // The last item starting at or before the position.
        let index = self.starts[..items]
            .partition_point(|&start| start <= position)
            .saturating_sub(1);
        let (start, end) = (self.starts[index], self.starts[index + 1]);
        let fraction = if end > start {
            ((position - start) / (end - start)).clamp(0., 1.)
        } else {
            0.
        };
        (index, fraction)
    }
}

/// An item's estimated height: its source lines, wrapped at
/// `chars_per_line`, plus the space after it.
fn item_height(source: &str, kind: BlockKind, chars_per_line: usize) -> f32 {
    let wrapped = |line: &str| line.chars().count().div_ceil(chars_per_line).max(1);
    let lines: usize = match kind {
        // Code doesn't wrap, and has a line of padding.
        BlockKind::CodeBlock { .. } => source.lines().count() + 1,
        _ => source.lines().map(wrapped).sum(),
    };
    let scale = match kind {
        BlockKind::Heading { level } => heading_scale(level) as f32,
        _ => 1.,
    };
    lines.max(1) as f32 * scale + BLOCK_SPACING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_items_are_taller() {
        let model = Model::new("# Title\n\nShort.\n\nA much longer paragraph that wraps.\n");
        let estimate = ScrollEstimate::new(&model, 10);
        let heights: Vec<f32> = (0..3)
            .map(|index| estimate.position(index + 1, 0.) - estimate.position(index, 0.))
            .collect();
        assert_eq!(heights, [2.0 + 0.625, 1.0 + 0.625, 4.0 + 0.625]);
        assert_eq!(estimate.total(), heights.iter().sum::<f32>());
    }

    #[test]
    fn code_does_not_wrap() {
        let model = Model::new("```\na very long line of code indeed\nx\n```\n");
        let estimate = ScrollEstimate::new(&model, 5);
        // Four source lines, plus padding and spacing.
        assert_eq!(estimate.total(), 5. + 0.625);
    }

    #[test]
    fn locate_inverts_position() {
        let model = Model::new("One.\n\nTwo.\n\nThree.\n");
        let estimate = ScrollEstimate::new(&model, 80);
        for (index, fraction) in [(0, 0.), (1, 0.5), (2, 0.25)] {
            let position = estimate.position(index, fraction);
            let (found, found_fraction) = estimate.locate(position);
            assert_eq!(found, index);
            assert!((found_fraction - fraction).abs() < 1e-5);
        }
        assert_eq!(estimate.locate(-1.), (0, 0.));
        assert_eq!(estimate.locate(1000.), (2, 1.));
    }

    #[test]
    fn empty_document() {
        // One empty item, a line high.
        let estimate = ScrollEstimate::new(&Model::new(""), 80);
        assert_eq!(estimate.total(), 1.625);
        assert_eq!(estimate.locate(0.), (0, 0.));
        assert_eq!(estimate.locate(3.), (0, 1.));
    }
}
