//! Smooth scrolling with ↑ and ↓ when there's no caret (#100): a press
//! glides a few lines, and holding the key scrolls at a steady speed until
//! it's let go, as in Safari.
//!
//! A press is told from a key repeat by the key going up: an ↑ or ↓ that
//! arrives while the last one is still down is the key repeating.

use std::time::{Duration, Instant};

use gpui::{Context, KeyDownEvent, KeyUpEvent, ListOffset, Window, px};

use super::DocumentView;

/// How far one press of ↑ or ↓ scrolls, about as far as in Safari.
const LINE_SCROLL: f32 = 40.;
/// How quickly a press's scroll settles, in seconds: it eases out, with
/// most of it done in three times this.
const SETTLE: f32 = 0.04;
/// How fast holding the key scrolls before its repeat rate is known, in
/// pixels a second: `LINE_SCROLL` every 50 ms.
const HELD_SPEED: f32 = LINE_SCROLL / 0.05;
/// Repeats further apart than this aren't used to measure the repeat
/// rate: the first comes after the longer delay before repeating starts.
const MAX_REPEAT: Duration = Duration::from_millis(150);
/// Holding stops if the key stops repeating for this long without being
/// seen going up.
const HELD_TIMEOUT: Duration = Duration::from_millis(300);

/// A scroll in progress from ↑ or ↓.
pub(super) struct LineScroll {
    down: bool,
    /// Whether the key is down (the last ↑ or ↓ hasn't gone up yet).
    pressed: bool,
    /// How far presses still have to scroll, in pixels.
    remaining: f32,
    /// While the key is held: how fast to scroll, in pixels a second.
    held: Option<f32>,
    /// When the last ↑ or ↓ came.
    last_key: Instant,
    /// When the last frame scrolled.
    last_frame: Instant,
    /// Where the scroll left the list, to notice anything else moving it:
    /// the list stopping at the document's end, the wheel, the scrollbar.
    left_at: Option<ListOffset>,
}

impl DocumentView {
    /// ↑ or ↓ with no caret: starts or continues a smooth scroll.
    pub(super) fn scroll_lines(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let animating = self.line_scroll.is_some();
        let scroll = match self.line_scroll.take() {
            // The key repeating: scroll steadily, as fast as it repeats.
            Some(mut scroll) if scroll.down == down && scroll.pressed => {
                let interval = now - scroll.last_key;
                let speed = scroll.held.unwrap_or(HELD_SPEED);
                scroll.held = Some(if interval <= MAX_REPEAT {
                    let measured = LINE_SCROLL / interval.as_secs_f32().max(0.008);
                    // Smoothed, so an uneven repeat doesn't jolt it.
                    speed + (measured - speed) * 0.3
                } else {
                    speed
                });
                scroll.last_key = now;
                scroll
            }
            // Another press the same way adds to what's left.
            Some(mut scroll) if scroll.down == down => {
                scroll.remaining += LINE_SCROLL;
                scroll.pressed = true;
                scroll.last_key = now;
                scroll
            }
            _ => LineScroll {
                down,
                pressed: true,
                remaining: LINE_SCROLL,
                held: None,
                last_key: now,
                last_frame: now,
                left_at: None,
            },
        };
        self.line_scroll = Some(scroll);
        if !animating {
            self.schedule_line_scroll(window, cx);
        }
    }

    /// Notes ↑ or ↓ going up, which ends holding it.
    pub(super) fn line_scroll_key_up(&mut self, event: &KeyUpEvent) {
        let key = event.keystroke.key.as_str();
        if let Some(scroll) = &mut self.line_scroll
            && key == if scroll.down { "down" } else { "up" }
        {
            scroll.pressed = false;
            release(scroll);
        }
    }

    /// Stops scrolling if another key is pressed.
    pub(super) fn line_scroll_key_down(&mut self, event: &KeyDownEvent) {
        let keystroke = &event.keystroke;
        let arrow = matches!(keystroke.key.as_str(), "up" | "down");
        if !(arrow && keystroke.modifiers.number_of_modifiers() == 0) {
            self.stop_line_scroll();
        }
    }

    /// Stops scrolling where it is.
    pub(super) fn stop_line_scroll(&mut self) {
        self.line_scroll = None;
    }

    /// Whether ↑ or ↓ is scrolling, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn line_scrolling(&self) -> bool {
        self.line_scroll.is_some()
    }

    fn schedule_line_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            view.update(cx, |this, cx| this.step_line_scroll(window, cx))
                .ok();
        });
    }

    /// Scrolls by however far this frame's share is, and asks for another
    /// frame until the scroll is done.
    fn step_line_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(open), Some(scroll)) = (&self.open, &mut self.line_scroll) else {
            self.line_scroll = None;
            return;
        };
        let list = &open.list_state;
        if scroll
            .left_at
            .is_some_and(|left| !same_place(list.logical_scroll_top(), left))
        {
            // Something else moved it, or the list stopped at the end.
            self.line_scroll = None;
            return;
        }
        let now = Instant::now();
        let elapsed = (now - scroll.last_frame).as_secs_f32().min(0.1);
        scroll.last_frame = now;
        if scroll.pressed && now - scroll.last_key > HELD_TIMEOUT {
            // The key went up unseen (the window lost focus, say).
            scroll.pressed = false;
            release(scroll);
        }
        let held = scroll.held.map_or(0., |speed| speed * elapsed);
        let eased = if scroll.remaining < 0.5 {
            scroll.remaining
        } else {
            scroll.remaining * (1. - (-elapsed / SETTLE).exp())
        };
        scroll.remaining -= eased;
        let distance = held + eased;
        list.scroll_by(px(if scroll.down { distance } else { -distance }));
        let top = list.logical_scroll_top();
        let at_top = top.item_ix == 0 && top.offset_in_item <= px(0.);
        scroll.left_at = Some(top);
        cx.notify();
        if (scroll.held.is_none() && scroll.remaining <= 0.) || (!scroll.down && at_top) {
            self.line_scroll = None;
        } else {
            self.schedule_line_scroll(window, cx);
        }
    }
}

fn same_place(a: ListOffset, b: ListOffset) -> bool {
    a.item_ix == b.item_ix && a.offset_in_item == b.offset_in_item
}

/// Lets go of a held key: glides on a little, then stops.
fn release(scroll: &mut LineScroll) {
    if let Some(speed) = scroll.held.take() {
        scroll.remaining += speed * SETTLE;
    }
}
