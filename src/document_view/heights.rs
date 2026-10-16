//! What the view knows about its items' heights, in one place.
//!
//! GPUI's list measures items only as it draws them, and counts every
//! item it hasn't measured as 0 px. [`ScrollEstimate`] estimates every
//! item from its text, but the list can't be given the estimate: it has no
//! per-item size hints. So there are two sources of heights, and every
//! question about where items are goes through [`Heights`], which knows
//! when to trust which.
//!
//! Short documents have every item measured up front ([`MEASURE_ALL_LIMIT`]),
//! so their heights are all known from the start. A long document's are
//! known only once it has all been drawn.
//!
//! Owning the heights outright would take a document element of Kertas's
//! own in place of GPUI's list; `docs/planning/document-element.md` records
//! why that isn't done yet, and what would change the decision.

use std::rc::Rc;

use gpui::{Bounds, ListOffset, ListState, Pixels, point, px};
use kertas::scroll_estimate::ScrollEstimate;

/// Documents with up to this many items have every item measured up
/// front, so their heights are all known from the start. The list measures
/// them all again whenever the zoom or the window's width changes, which
/// costs about 0.5 ms per item in a release build. Measured on
/// spikes/fixtures/10k.md (2,727 items, M2 Air): with every item measured,
/// a zoom step took 1.33 s and loading 456 ms; with only the items drawn
/// measured (10k.md is over the limit), 30 ms and 161 ms
/// (architecture-review-v0.3.md §7b). So 400 costs a zoom step about
/// 200 ms at worst; 600 (about 300 ms) is as high as it should go.
pub(super) const MEASURE_ALL_LIMIT: usize = 400;

/// How the scrollbar places its thumb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScrollbarMode {
    /// From the list's own heights, once it knows them all.
    Exact,
    /// From [`ScrollEstimate`].
    Estimated,
}

/// The list's heights, and the estimate to use where they're unknown.
#[derive(Clone)]
pub(super) struct Heights {
    list: ListState,
    estimate: Option<Estimate>,
}

#[derive(Clone)]
struct Estimate {
    items: Rc<ScrollEstimate>,
    /// Body text's line height, which the estimate counts in.
    line: Pixels,
}

impl Heights {
    /// The list's heights alone.
    pub(super) fn measured(list: &ListState) -> Self {
        Self {
            list: list.clone(),
            estimate: None,
        }
    }

    /// The list's heights, with `estimate` (in lines of `line` pixels) for
    /// the items it hasn't measured.
    pub(super) fn estimated(list: &ListState, estimate: Rc<ScrollEstimate>, line: Pixels) -> Self {
        Self {
            list: list.clone(),
            estimate: Some(Estimate {
                items: estimate,
                line,
            }),
        }
    }

    /// Whether the list knows every item's height. (GPUI answers only for
    /// a document longer than the window.)
    pub(super) fn all_known(&self) -> bool {
        // `is_scrolled_to_end` is `None` until every height is known.
        self.list.is_scrolled_to_end().is_some()
    }

    /// Where the window shows the document.
    pub(super) fn viewport(&self) -> Bounds<Pixels> {
        self.list.viewport_bounds()
    }

    /// Item `index`'s bounds, if it's at least partly in the window. The
    /// list also knows where items just off screen are (it draws ahead),
    /// so having bounds doesn't mean being visible (#98).
    pub(super) fn on_screen(&self, index: usize) -> Option<Bounds<Pixels>> {
        let viewport = self.viewport();
        self.list
            .bounds_for_item(index)
            .filter(|bounds| bounds.top() < viewport.bottom() && bounds.bottom() > viewport.top())
    }

    /// Whether the window is at the document's top.
    fn at_top(&self) -> bool {
        let top = self.list.logical_scroll_top();
        top.item_ix == 0 && top.offset_in_item <= px(0.)
    }

    /// How far the document goes on below the window, if the list has
    /// drawn its last item, so it knows.
    fn left_below(&self) -> Option<Pixels> {
        let last = self.list.item_count().saturating_sub(1);
        let bounds = self.list.bounds_for_item(last)?;
        Some((bounds.bottom() - self.viewport().bottom()).max(px(0.)))
    }

    /// How the scrollbar places its thumb: exactly once every height is
    /// known, from the estimate until then. A drag keeps the mode it
    /// started with (`dragging`), even if the heights it brings into view
    /// make them all known: GPUI froze the list's height when the drag
    /// started (`scrollbar_drag_started`), and in an estimated document
    /// that was only the height of what had been drawn, so placing the
    /// thumb exactly mid-drag would throw the document back toward the
    /// top.
    pub(super) fn scrollbar_mode(&self, dragging: Option<ScrollbarMode>) -> ScrollbarMode {
        if let Some(mode) = dragging {
            mode
        } else if self.estimate.is_none() || self.all_known() {
            ScrollbarMode::Exact
        } else {
            ScrollbarMode::Estimated
        }
    }

    /// Item `index`'s height: measured if the list has drawn it, estimated
    /// otherwise.
    pub(super) fn item_height(&self, index: usize) -> Pixels {
        let measured = self
            .list
            .bounds_for_item(index)
            .map(|bounds| bounds.size.height)
            .filter(|&height| height > px(0.));
        measured.unwrap_or_else(|| {
            self.estimate.as_ref().map_or(px(0.), |estimate| {
                estimate.line
                    * (estimate.items.position(index, 1.) - estimate.items.position(index, 0.))
            })
        })
    }

    /// How far the list can scroll, as `mode` counts it.
    pub(super) fn scroll_range(&self, mode: ScrollbarMode) -> Pixels {
        match (mode, &self.estimate) {
            (ScrollbarMode::Estimated, Some(estimate)) => {
                estimate.line * estimate.items.total() - self.viewport().size.height
            }
            _ => self.list.max_offset_for_scrollbar().y,
        }
    }

    /// How far down the document the window's top is, as `mode` counts
    /// it. At either end the list knows exactly, so the estimate gives way
    /// to it there.
    pub(super) fn scroll_position(&self, mode: ScrollbarMode) -> Pixels {
        match (mode, &self.estimate) {
            (ScrollbarMode::Estimated, Some(_)) if self.at_top() => px(0.),
            (ScrollbarMode::Estimated, Some(estimate)) => {
                let top = self.list.logical_scroll_top();
                let fraction = self
                    .list
                    .bounds_for_item(top.item_ix)
                    .map(|bounds| bounds.size.height)
                    .filter(|&height| height > px(0.))
                    .map_or(0., |height| top.offset_in_item / height);
                let position = estimate.line * estimate.items.position(top.item_ix, fraction);
                // Near the end, the list has drawn the last item, so how
                // far there is still to go is known.
                match self.left_below() {
                    Some(left) => position.max(self.scroll_range(mode).max(px(1.)) - left),
                    None => position,
                }
            }
            _ => -self.list.scroll_px_offset_for_scrollbar().y,
        }
    }

    /// Scrolls `through` (0 to 1) the way down the document, as `mode`
    /// counts it.
    pub(super) fn scroll_through(&self, mode: ScrollbarMode, through: f32) {
        let list = &self.list;
        match (mode, &self.estimate) {
            (ScrollbarMode::Estimated, Some(_)) if through >= 1. => {
                // All the way down: the list's end, wherever it really is.
                list.scroll_to(ListOffset {
                    item_ix: list.item_count(),
                    offset_in_item: px(0.),
                });
            }
            (ScrollbarMode::Estimated, Some(estimate)) => {
                let visible = self.viewport().size.height / estimate.line;
                let span = (estimate.items.total() - visible).max(0.);
                let (index, fraction) = estimate.items.locate(through * span);
                list.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: self.item_height(index) * fraction,
                });
            }
            _ => {
                let max = list.max_offset_for_scrollbar().y;
                list.set_offset_from_scrollbar(point(px(0.), -(max * through)));
            }
        }
    }
}
