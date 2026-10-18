//! The document's scrollbar (#88), shown or hidden by the `scrollbar`
//! setting.
//!
//! Once the list knows every item's height, the thumb is exact. Short
//! documents are measured up front, so that's from the start. In a long
//! document only the items that have been drawn are measured, so until it
//! has all been seen the thumb is placed using [`ScrollEstimate`] instead,
//! which estimates every item from its text: steadier than the list's
//! partial measurements, which grow as it scrolls. [`Heights`] decides
//! which.

use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Context, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Role, ScrollWheelEvent, Window, canvas, div, fill, point, prelude::*, px,
    size,
};
use kertas_core::scroll_estimate::ScrollEstimate;

use super::heights::{Heights, ScrollbarMode};
use super::{DocumentView, OpenDocument};

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

/// The estimate for one version of the document at one line width.
pub(super) struct CachedEstimate {
    model: usize,
    chars_per_line: usize,
    estimate: Rc<ScrollEstimate>,
}

/// A drag of the thumb in progress.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScrollbarDrag {
    /// Where the pointer is on the thumb, from its top.
    grab: Pixels,
    /// Where the thumb's top is now, in the track.
    thumb_top: Pixels,
    /// The mode the drag started with, which it keeps.
    mode: ScrollbarMode,
}

/// Where the thumb is.
#[derive(Clone, Copy, Debug)]
struct Thumb {
    /// The track: the list's bounds.
    track: Bounds<Pixels>,
    top: Pixels,
    height: Pixels,
    mode: ScrollbarMode,
}

impl Thumb {
    /// How far the thumb can move.
    fn room(&self) -> Pixels {
        self.track.size.height - self.height
    }
}

/// What places the thumb, so it can be worked out after the list is laid
/// out in each frame.
#[derive(Clone)]
struct Placement {
    heights: Heights,
    drag: Option<ScrollbarDrag>,
}

impl Placement {
    /// Where the thumb goes, or `None` if the document fits in the window.
    fn thumb(&self) -> Option<Thumb> {
        let heights = &self.heights;
        let track = heights.viewport();
        let height = track.size.height;
        if height <= px(0.) || heights.scroll_range(ScrollbarMode::Exact) <= px(0.) {
            return None;
        }
        let mode = heights.scrollbar_mode(self.drag.map(|drag| drag.mode));
        let range = heights.scroll_range(mode);
        let total = range + height;
        let through = (heights.scroll_position(mode) / range.max(px(1.))).clamp(0., 1.);
        let shown = (height / total.max(height)).min(1.);
        let thumb = (height * shown).max(px(MIN_THUMB)).min(height);
        let room = height - thumb;
        // While dragging, the thumb follows the pointer exactly.
        let top = match self.drag {
            Some(drag) => drag.thumb_top.clamp(px(0.), room),
            None => room * through,
        };
        Some(Thumb {
            track,
            top,
            height: thumb,
            mode,
        })
    }

    /// Scrolls so the thumb's top is at `thumb_top` in the track.
    fn scroll_to(&self, thumb: Thumb, thumb_top: Pixels) {
        let room = thumb.room();
        let through = if room > px(0.) {
            (thumb_top / room).clamp(0., 1.)
        } else {
            0.
        };
        self.heights.scroll_through(thumb.mode, through);
    }
}

impl DocumentView {
    /// The estimate for the open document, at the current line width.
    fn scroll_estimate(&self, open: &OpenDocument, window: &Window) -> Rc<ScrollEstimate> {
        let rem = window.rem_size();
        let viewport = open.list_state.viewport_bounds();
        let (_, width) = self.text_column(viewport.size.width, rem);
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

    fn placement(&self, open: &OpenDocument, window: &Window) -> Placement {
        Placement {
            heights: Heights::estimated(
                &open.list_state,
                self.scroll_estimate(open, window),
                window.rem_size() * LINE_HEIGHT,
            ),
            drag: open.scrollbar_drag,
        }
    }

    fn scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thumb) = self
            .open
            .as_ref()
            .and_then(|open| self.placement(open, window).thumb())
        else {
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
                mode: thumb.mode,
            });
        } else {
            // Clicking the track scrolls a page toward the click.
            self.scroll_page(y > thumb.top, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Follows the pointer while the thumb is dragged.
    fn scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.open.as_ref().and_then(|open| open.scrollbar_drag) else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_scrollbar_drag(cx);
            return;
        }
        let Some(open) = &self.open else {
            return;
        };
        let top = open.list_state.viewport_bounds().top();
        self.drag_thumb(event.position.y - top - drag.grab, window);
        cx.notify();
    }

    /// Moves the thumb being dragged to `thumb_top`, scrolling to match.
    fn drag_thumb(&mut self, thumb_top: Pixels, window: &Window) {
        let Some(open) = &self.open else {
            return;
        };
        let placement = self.placement(open, window);
        let Some(thumb) = placement.thumb() else {
            return;
        };
        placement.scroll_to(thumb, thumb_top);
        if let Some(open) = &mut self.open
            && let Some(drag) = &mut open.scrollbar_drag
        {
            drag.thumb_top = thumb_top.clamp(px(0.), thumb.room());
        }
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
    /// shows it. The thumb is placed once the list is laid out, and only
    /// drawn if the document doesn't fit in the window.
    pub(super) fn render_scrollbar(
        &self,
        open: &OpenDocument,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let placement = self.placement(open, window);
        let color = self
            .theme
            .muted_text
            .opacity(if open.scrollbar_drag.is_some() {
                0.6
            } else {
                0.35
            });
        let list_state = open.list_state.clone();
        let dragging = open.scrollbar_drag.is_some();
        let view = cx.entity().downgrade();
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
                canvas(
                    |_, _, _| {},
                    move |_, (), window, _| {
                        // While the thumb is dragged, follow the pointer
                        // wherever it goes, even off the window (which an
                        // element's own mouse listeners don't hear), and
                        // end the drag wherever the button goes up.
                        if dragging {
                            let moved = view.clone();
                            window.on_mouse_event(
                                move |event: &MouseMoveEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Capture {
                                        moved
                                            .update(cx, |this, cx| {
                                                this.scrollbar_mouse_move(event, window, cx)
                                            })
                                            .ok();
                                    }
                                },
                            );
                            let released = view.clone();
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Capture
                                    && event.button == MouseButton::Left
                                {
                                    released
                                        .update(cx, |this, cx| this.end_scrollbar_drag(cx))
                                        .ok();
                                }
                            });
                        }
                        let Some(thumb) = placement.thumb() else {
                            return;
                        };
                        let bounds = Bounds::new(
                            point(
                                thumb.track.right() - px((TRACK_WIDTH + THUMB_WIDTH) / 2.),
                                thumb.track.top() + thumb.top,
                            ),
                            size(px(THUMB_WIDTH), thumb.height),
                        );
                        window.paint_quad(fill(bounds, color).corner_radii(px(THUMB_WIDTH / 2.)));
                    },
                )
                .size_full(),
            )
            .into_any_element()
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
        let thumb = self.placement(self.open.as_ref()?, window).thumb()?;
        Some((
            f32::from(thumb.top),
            f32::from(thumb.height),
            f32::from(thumb.track.size.height),
        ))
    }

    /// Whether the thumb is placed exactly, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn scrollbar_exact(&self, window: &Window) -> bool {
        self.open.as_ref().is_some_and(|open| {
            self.placement(open, window)
                .thumb()
                .is_some_and(|thumb| thumb.mode == ScrollbarMode::Exact)
        })
    }

    /// How far the document is scrolled, in pixels, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn scroll_offset(&self) -> f32 {
        self.open.as_ref().map_or(0., |open| {
            f32::from(-open.list_state.scroll_px_offset_for_scrollbar().y)
        })
    }

    /// Grabs the thumb, for the smoke test, as a mouse down on it would.
    #[cfg(debug_assertions)]
    pub fn grab_thumb(&mut self, window: &Window) {
        let Some(thumb) = self
            .open
            .as_ref()
            .and_then(|open| self.placement(open, window).thumb())
        else {
            return;
        };
        if let Some(open) = &mut self.open {
            open.list_state.scrollbar_drag_started();
            open.scrollbar_drag = Some(ScrollbarDrag {
                grab: px(0.),
                thumb_top: thumb.top,
                mode: thumb.mode,
            });
        }
    }

    /// Drags the grabbed thumb to `top`, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn drag_thumb_to(&mut self, top: f32, window: &Window, cx: &mut Context<Self>) {
        self.drag_thumb(px(top), window);
        cx.notify();
    }

    /// Whether the thumb is being dragged, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn scrollbar_dragging(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.scrollbar_drag.is_some())
    }

    /// Lets go of the thumb, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn release_thumb(&mut self, cx: &mut Context<Self>) {
        self.end_scrollbar_drag(cx);
    }

    /// Where the track is, for the smoke test to click it.
    #[cfg(debug_assertions)]
    pub fn scrollbar_track(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.open.as_ref()?.list_state.viewport_bounds();
        Some(Bounds::from_corners(
            point(bounds.right() - px(TRACK_WIDTH), bounds.top()),
            bounds.bottom_right(),
        ))
    }
}
