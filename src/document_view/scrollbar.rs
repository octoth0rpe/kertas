//! The document's scrollbar (#88), shown or hidden by the `scrollbar`
//! setting.
//!
//! The list only knows the heights of items it has drawn, so the thumb is
//! placed using [`ScrollEstimate`] instead, which estimates every item from
//! its text. That keeps the thumb steady as items are measured; dragging it
//! scrolls to the item the estimate puts there.

use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Context, ListOffset, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels,
    Role, ScrollWheelEvent, Window, div, prelude::*, px,
};
use kertas::scroll_estimate::ScrollEstimate;

use super::{DocumentView, OpenDocument};
use crate::theme::Theme;

/// The width of the scrollbar's track, which sits in the list's right
/// padding, clear of the text.
const TRACK_WIDTH: f32 = 12.;
/// The thumb's width, and its smallest height.
const THUMB_WIDTH: f32 = 6.;
const MIN_THUMB: f32 = 24.;

/// Body text's line height, relative to the text size (see the view's
/// `line_height`).
const LINE_HEIGHT: f32 = 1.6;
/// Roughly how wide a character of body text is, relative to the text
/// size, for estimating where lines wrap.
const CHAR_WIDTH: f32 = 0.5;
/// The list's padding on each side of the text, in pixels.
const SIDE_PADDING: f32 = 32.;

/// The estimate for one version of the document at one line width.
pub(super) struct CachedEstimate {
    model: usize,
    chars_per_line: usize,
    estimate: Rc<ScrollEstimate>,
}

/// Where the thumb is drawn.
#[derive(Clone, Copy, Debug)]
pub(super) struct Thumb {
    /// The track: the list's bounds.
    track: Bounds<Pixels>,
    top: Pixels,
    height: Pixels,
}

impl DocumentView {
    /// The estimate for the open document, at the current line width.
    fn scroll_estimate(&self, open: &OpenDocument, window: &Window) -> Rc<ScrollEstimate> {
        let rem = window.rem_size();
        let viewport = open.list_state.viewport_bounds();
        let available = viewport.size.width - px(2. * SIDE_PADDING);
        let width = match super::render::column_width(self.text_settings.text_width) {
            Some(column) => column.to_pixels(rem).min(available),
            None => available,
        };
        let chars_per_line = (width / (rem * CHAR_WIDTH)).max(1.) as usize;
        let model = Rc::as_ptr(open.editor.model()) as usize;
        let mut cache = open.scroll_estimate.borrow_mut();
        match &*cache {
            Some(cached) if cached.model == model && cached.chars_per_line == chars_per_line => {
                cached.estimate.clone()
            }
            _ => {
                let estimate = Rc::new(ScrollEstimate::new(open.editor.model(), chars_per_line));
                *cache = Some(CachedEstimate {
                    model,
                    chars_per_line,
                    estimate: estimate.clone(),
                });
                estimate
            }
        }
    }

    /// Where the thumb goes, or `None` if the document fits in the window.
    pub(super) fn thumb(&self, open: &OpenDocument, window: &Window) -> Option<Thumb> {
        let track = open.list_state.viewport_bounds();
        let height = track.size.height;
        let line = window.rem_size() * LINE_HEIGHT;
        let estimate = self.scroll_estimate(open, window);
        let total = line * estimate.total();
        let scrollable = open.list_state.max_offset_for_scrollbar().y > px(0.);
        if height <= px(0.) || !scrollable {
            return None;
        }
        let thumb = (height * (height / total.max(height)))
            .max(px(MIN_THUMB))
            .min(height);
        let room = height - thumb;

        // While dragging, the thumb follows the pointer exactly.
        if let Some(top) = open.scrollbar_drag.map(|drag| drag.thumb_top) {
            return Some(Thumb {
                track,
                top: top.clamp(px(0.), room),
                height: thumb,
            });
        }

        let top = open.list_state.logical_scroll_top();
        let item_height = open
            .list_state
            .bounds_for_item(top.item_ix)
            .map(|bounds| bounds.size.height)
            .filter(|&height| height > px(0.));
        let fraction = item_height.map_or(0., |item| top.offset_in_item / item);
        let position = line * estimate.position(top.item_ix, fraction);
        let range = (total - height).max(px(1.));
        let mut through = (position / range).clamp(0., 1.);
        // Near the end, the last item has been drawn, so how far there is
        // still to go is known exactly; trust it over the estimate.
        let last = open.list_state.item_count().saturating_sub(1);
        if let Some(bounds) = open.list_state.bounds_for_item(last) {
            let remaining = (bounds.bottom() - track.bottom()).max(px(0.));
            through = through.max(1. - (remaining / range).min(1.));
        }
        if top.item_ix == 0 && top.offset_in_item <= px(0.) {
            through = 0.;
        }
        Some(Thumb {
            track,
            top: room * through,
            height: thumb,
        })
    }

    /// Scrolls so the thumb's top is at `thumb_top` in the track.
    fn scroll_to_thumb(&mut self, thumb_top: Pixels, window: &Window) {
        let Some(open) = &self.open else {
            return;
        };
        let Some(thumb) = self.thumb(open, window) else {
            return;
        };
        let room = thumb.track.size.height - thumb.height;
        let through = if room > px(0.) {
            (thumb_top / room).clamp(0., 1.)
        } else {
            0.
        };
        if through >= 1. {
            // All the way down: the list's end, wherever it really is.
            open.list_state.scroll_to(ListOffset {
                item_ix: open.list_state.item_count(),
                offset_in_item: px(0.),
            });
            return;
        }
        let line = window.rem_size() * LINE_HEIGHT;
        let estimate = self.scroll_estimate(open, window);
        let visible = thumb.track.size.height / line;
        let (index, fraction) = estimate.locate(through * (estimate.total() - visible).max(0.));
        let item_height = open
            .list_state
            .bounds_for_item(index)
            .map(|bounds| bounds.size.height)
            .unwrap_or(line * (estimate.position(index, 1.) - estimate.position(index, 0.)));
        open.list_state.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: item_height * fraction,
        });
    }

    fn scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thumb) = self.open.as_ref().and_then(|open| self.thumb(open, window)) else {
            return;
        };
        let y = event.position.y - thumb.track.top();
        if y >= thumb.top
            && y <= thumb.top + thumb.height
            && let Some(open) = &mut self.open
        {
            open.list_state.scrollbar_drag_started();
            open.scrollbar_drag = Some(ScrollbarDrag {
                grab: y - thumb.top,
                thumb_top: thumb.top,
            });
        } else {
            // Clicking the track scrolls a page toward the click.
            self.scroll_page(y > thumb.top, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Follows the pointer while the thumb is dragged.
    pub(super) fn scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(drag) = open.scrollbar_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_scrollbar_drag(cx);
            return;
        }
        let top = open.list_state.viewport_bounds().top();
        let thumb_top = event.position.y - top - drag.grab;
        open.scrollbar_drag = Some(ScrollbarDrag { thumb_top, ..drag });
        self.scroll_to_thumb(thumb_top, window);
        cx.notify();
    }

    /// Ends dragging the thumb, if it was being dragged.
    pub(super) fn end_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = &mut self.open
            && open.scrollbar_drag.take().is_some()
        {
            open.list_state.scrollbar_drag_ended();
            cx.notify();
        }
    }

    /// The scrollbar, laid over the list's right padding, if the setting
    /// shows it and the document doesn't fit in the window.
    pub(super) fn render_scrollbar(
        &self,
        open: &OpenDocument,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let thumb = self.thumb(open, window)?;
        let theme: Theme = self.theme;
        let dragging = open.scrollbar_drag.is_some();
        let list_state = open.list_state.clone();
        Some(
            div()
                .id("scrollbar")
                .role(Role::ScrollBar)
                .aria_label("Scrollbar")
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(TRACK_WIDTH))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::scrollbar_mouse_down))
                // The wheel and trackpad still scroll over the scrollbar.
                .on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(window.line_height());
                    list_state.scroll_by(-delta.y);
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .absolute()
                        .top(thumb.top)
                        .h(thumb.height)
                        .right(px((TRACK_WIDTH - THUMB_WIDTH) / 2.))
                        .w(px(THUMB_WIDTH))
                        .rounded_full()
                        .bg(theme.muted_text.opacity(if dragging { 0.6 } else { 0.35 })),
                )
                .into_any_element(),
        )
    }
}

impl DocumentView {
    /// The thumb's top and height, and the track's height, if the
    /// scrollbar is shown, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn scrollbar_thumb(&self, window: &Window, cx: &gpui::App) -> Option<(f32, f32, f32)> {
        if !crate::app_settings::get(cx).scrollbar {
            return None;
        }
        let thumb = self.thumb(self.open.as_ref()?, window)?;
        Some((
            f32::from(thumb.top),
            f32::from(thumb.height),
            f32::from(thumb.track.size.height),
        ))
    }

    /// Drags the thumb to `top`, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn drag_thumb_to(&mut self, top: f32, window: &Window, cx: &mut Context<Self>) {
        self.scroll_to_thumb(px(top), window);
        cx.notify();
    }

    /// Where the track is, for the smoke test to click it.
    #[cfg(debug_assertions)]
    pub fn scrollbar_track(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.open.as_ref()?.list_state.viewport_bounds();
        Some(Bounds::from_corners(
            gpui::point(bounds.right() - px(TRACK_WIDTH), bounds.top()),
            bounds.bottom_right(),
        ))
    }
}

/// A drag of the thumb in progress.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScrollbarDrag {
    /// Where the pointer is on the thumb, from its top.
    grab: Pixels,
    /// Where the thumb's top is now, in the track.
    thumb_top: Pixels,
}
