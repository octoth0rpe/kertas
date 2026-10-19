//! Popovers the host shows when the pointer rests on a link or on text it
//! marked (#126).
//!
//! The pointer has to stay on the same link or mark for [`HOVER_DELAY`]
//! before the host is asked. The popover then stays while the pointer is on
//! the text or the popover, with a moment to get from one to the other, and
//! closes when it leaves both, or on typing, clicking or scrolling.

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, MouseButton, Pixels, Point, Window, anchored, deferred, div,
    prelude::*, px, rems,
};
use kertas_core::inline::Link;

use super::DocumentView;
use crate::marks::Mark;

/// How long the pointer has to rest on a link or a mark before its popover
/// shows.
pub const HOVER_DELAY: Duration = Duration::from_millis(500);

/// How long a popover stays after the pointer leaves it or its text, so the
/// pointer can get from one to the other.
const CLOSE_DELAY: Duration = Duration::from_millis(300);

/// What the pointer is resting on, which the host shows a popover for.
#[derive(Debug, Clone, PartialEq)]
pub enum HoverTarget {
    Link(Link),
    /// A mark the host set with
    /// [`DocumentView::set_marks`](crate::DocumentView::set_marks).
    Mark(Mark),
}

/// What a host shows over a link or a mark: an element, which the view
/// draws in a frame next to the text, or nothing. It's asked each time the
/// popover is drawn.
pub type HoverPopover = Rc<dyn Fn(&HoverTarget, &mut Window, &mut App) -> Option<AnyElement>>;

/// The link or mark the pointer is on, and its popover once it shows.
pub(super) struct Hover {
    target: HoverTarget,
    /// Where the popover goes, in the window: below the target's text.
    anchor: Point<Pixels>,
    shown: bool,
    over_text: bool,
    over_popover: bool,
}

impl DocumentView {
    /// Shows what `popover` gives when the pointer rests on a link or a
    /// mark (see [`HoverPopover`]). `None`, as Kertas has, shows nothing.
    pub fn set_hover_popover(&mut self, popover: Option<HoverPopover>, cx: &mut Context<Self>) {
        self.hover_popover = popover;
        self.close_hover(cx);
    }

    /// Whether a popover is showing, and what for, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn hover_shown(&self) -> Option<HoverTarget> {
        self.hover
            .as_ref()
            .filter(|hover| hover.shown)
            .map(|hover| hover.target.clone())
    }

    /// The pointer moved onto `target`, whose popover would go at `anchor`,
    /// or onto text with no link or mark if `None`.
    pub(crate) fn hover_text(
        &mut self,
        at: Option<(HoverTarget, Point<Pixels>)>,
        cx: &mut Context<Self>,
    ) {
        if self.hover_popover.is_none() {
            return;
        }
        let Some((target, anchor)) = at else {
            if let Some(hover) = &mut self.hover
                && hover.over_text
            {
                hover.over_text = false;
                self.schedule_hover_close(cx);
            }
            return;
        };
        if let Some(hover) = &mut self.hover
            && hover.target == target
        {
            hover.over_text = true;
            return;
        }
        // Another target: a popover showing for the last one goes.
        self.hover = Some(Hover {
            target,
            anchor,
            shown: false,
            over_text: true,
            over_popover: false,
        });
        self.hover_epoch += 1;
        let epoch = self.hover_epoch;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            this.update(cx, |this, cx| {
                if this.hover_epoch == epoch
                    && let Some(hover) = &mut this.hover
                {
                    hover.shown = true;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Closes the popover, or stops it from showing.
    pub(crate) fn close_hover(&mut self, cx: &mut Context<Self>) {
        self.hover_epoch += 1;
        if self.hover.take().is_some() {
            cx.notify();
        }
    }

    /// Closes the popover after [`CLOSE_DELAY`], unless the pointer is back
    /// on it or its text by then.
    fn schedule_hover_close(&mut self, cx: &mut Context<Self>) {
        self.hover_epoch += 1;
        let epoch = self.hover_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CLOSE_DELAY).await;
            this.update(cx, |this, cx| {
                let held = this
                    .hover
                    .as_ref()
                    .is_some_and(|hover| hover.over_text || hover.over_popover);
                if this.hover_epoch == epoch && !held {
                    this.close_hover(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn popover_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        let Some(hover) = &mut self.hover else {
            return;
        };
        hover.over_popover = hovered;
        if !hovered && !hover.over_text {
            self.schedule_hover_close(cx);
        }
    }

    /// The popover, if one is showing and the host gives something for it:
    /// in a frame below its text, on top of the document.
    pub(super) fn render_hover(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let hover = self.hover.as_ref().filter(|hover| hover.shown)?;
        let popover = self.hover_popover.clone()?;
        let content = popover(&hover.target, window, cx)?;
        let theme = self.theme;
        Some(
            deferred(
                anchored()
                    .position(hover.anchor)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("hover-popover")
                            .occlude()
                            .mt(px(4.))
                            .max_w(rems(24.))
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.rule)
                            .bg(theme.background)
                            .text_color(theme.text)
                            .shadow_md()
                            .on_hover(cx.listener(|this, hovered, _, cx| {
                                this.popover_hovered(*hovered, cx)
                            }))
                            // Clicks in it don't reach the document under it.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            // It closes once the host has handled a click.
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|_, _, window, cx| {
                                    cx.defer_in(window, |this, _, cx| this.close_hover(cx))
                                }),
                            )
                            .child(content),
                    ),
            )
            .into_any_element(),
        )
    }
}
