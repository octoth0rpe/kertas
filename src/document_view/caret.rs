//! The caret and the selection: clicks, movement, blinking, and revealing
//! the items the selection touches.

use std::rc::Rc;

use gpui::{Context, ListOffset, Pixels, Window, point, px};
use kertas::editor::{Motion, RevealTarget, Selection};
use kertas::reveal::styled_source;

use crate::revealed_block::{CaretPlace, RevealedLayout};

use super::heights::Heights;

use super::{
    CARET_BLINK_INTERVAL, DocumentView, LineGoal, OpenDocument, Revealed, ScrollRequest, SelectAll,
    Unreveal,
};

impl DocumentView {
    /// Places the caret at document `offset` after a click.
    pub fn click(
        &mut self,
        offset: usize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        if let Some(open) = &mut self.open {
            open.goal_x = None;
            open.editor.unmark();
        }
        self.dragging = true;
        self.pressed_task = None;
        self.move_caret(offset, extend, cx);
    }

    /// Whether a block is being edited here: the document has the caret
    /// and the keyboard focus. Otherwise Tab moves the focus between the
    /// document and the buttons in its bars.
    pub(super) fn is_editing(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
            && self
                .open
                .as_ref()
                .is_some_and(|open| open.editor.selection().is_some())
    }

    /// Stops extending the selection when the mouse moves.
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Extends the selection to document `offset` while dragging.
    pub fn drag_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let unchanged = self
            .open
            .as_ref()
            .and_then(|open| open.editor.selection())
            .is_some_and(|selection| selection.head == offset);
        if self.dragging && !unchanged {
            self.set_caret(offset, true, cx);
        }
    }

    /// Page Up and Page Down: scroll by a page, leaving the caret where it
    /// is, as on macOS.
    pub(super) fn scroll_page(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        // Keep a little of the last page in view for context.
        let page = (open.list_state.viewport_bounds().size.height - px(48.)).max(px(48.));
        open.list_state.scroll_by(if down { page } else { -page });
        cx.notify();
    }

    /// Once a frame that scrolled to the caret is drawn: the fallback for
    /// a caret whose item wasn't laid out, so it couldn't ask to be
    /// scrolled to. `set_selection` makes the caret's item the list's
    /// first whenever it might not be on screen, so this shouldn't happen;
    /// if it does, the item goes to the top and asks again next frame.
    pub(super) fn after_layout(&mut self, request: Option<ScrollRequest>, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if let Some(ScrollRequest::Caret(_)) = request
            && open.scroll_request.is_none()
            && open.revealed_layout.borrow().is_none()
            && let Some(selection) = open.editor.selection()
        {
            let index = open.editor.model().item_at(selection.head);
            open.list_state.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
            open.scroll_request = Some(ScrollRequest::Caret(CaretPlace::Top(px(0.))));
            cx.notify();
        }
    }

    /// Moves the caret to `offset` as a navigation (not an edit): it ends
    /// the current undo group.
    pub(super) fn move_caret(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        if let Some(open) = &mut self.open {
            open.editor.break_group();
        }
        self.set_caret(offset, extend, cx);
    }

    /// Moves the caret to `offset`, revealing its item.
    fn set_caret(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let selection = open.editor.selection_to(offset, extend);
        self.set_selection(selection, cx);
    }

    pub(super) fn set_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.stop_line_scroll();
        let Some(open) = &mut self.open else {
            return;
        };
        // Read-only documents have no caret, so every movement, click and
        // edit stops here.
        if !open.editor.select(selection) {
            return;
        }
        let index = open.editor.model().item_at(selection.head);
        let heights = Heights::measured(&open.list_state);
        let viewport = heights.viewport();
        let on_screen = heights.on_screen(index);
        let changed = reveal(open);
        // Unless the caret stays in its item, make the item the list's
        // first, so it's laid out next frame however wrong the list is
        // about the heights before it. The caret's element then scrolls it
        // to its place in that same frame.
        let scroll_to = |offset_in_item| {
            open.list_state.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item,
            })
        };
        let place = match on_screen {
            Some(_) if !changed => CaretPlace::Nearest,
            // Where it is (C7): revealing blocks changes their heights,
            // which would otherwise move the text under the pointer.
            Some(bounds) if bounds.top() >= viewport.top() => {
                scroll_to(px(0.));
                CaretPlace::Top(bounds.top() - viewport.top())
            }
            Some(bounds) => {
                scroll_to(viewport.top() - bounds.top());
                CaretPlace::Nearest
            }
            // Off screen, below the window: its bottom at the window's
            // bottom, scrolling as little as possible. Above: at the top.
            None if index > open.list_state.logical_scroll_top().item_ix => {
                scroll_to(px(0.));
                CaretPlace::Bottom
            }
            None => {
                scroll_to(px(0.));
                CaretPlace::Top(px(0.))
            }
        };
        open.scroll_request = Some(ScrollRequest::Caret(place));
        self.restart_blink(cx);
        cx.notify();
    }

    /// Shows the caret and starts blinking it again, so it stays solid
    /// while the user is moving it.
    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_epoch += 1;
        self.caret_visible = true;
        let epoch = self.blink_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CARET_BLINK_INTERVAL).await;
                let blinking = this.update(cx, |this, cx| {
                    if this.blink_epoch != epoch {
                        return false;
                    }
                    this.caret_visible = !this.caret_visible;
                    cx.notify();
                    true
                });
                if !matches!(blinking, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn stop_blink(&mut self) {
        self.blink_epoch += 1;
    }

    pub(super) fn unreveal(&mut self, _: &Unreveal, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_editing(cx);
    }

    /// Hides the caret and shows every block rendered.
    pub fn finish_editing(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.editor.deselect();
        open.goal_x = None;
        reveal(open);
        self.stop_blink();
        cx.notify();
    }

    /// The caret's offset, placing it at the document start if there's no
    /// caret yet.
    fn caret(&self) -> Option<usize> {
        Some(self.open.as_ref()?.editor.caret())
    }

    /// Applies a layout-independent movement. `forward` is the direction,
    /// also used to collapse a selection to the matching edge.
    pub(super) fn move_by(
        &mut self,
        motion: Motion,
        forward: bool,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.has_caret() {
            match motion {
                Motion::Document if !extend => self.scroll_to_end(forward, cx),
                _ => self.place_caret_on_screen(cx),
            }
            return;
        }
        let Some(open) = &mut self.open else {
            return;
        };
        let target = open.editor.motion_target(motion, forward, extend);
        open.goal_x = None;
        self.move_caret(target, extend, cx);
    }

    /// The revealed item's layout and start offset, if it's been painted.
    pub(super) fn revealed_layout(&self) -> Option<(RevealedLayout, usize)> {
        let open = self.open.as_ref()?;
        open.editor.selection()?;
        let revealed = open.revealed.iter().find(|revealed| revealed.target.head)?;
        let layout = open.revealed_layout.borrow().clone()?;
        Some((layout, revealed.target.range.start))
    }

    /// Moves the caret up or down a visual line, into the neighbor item at
    /// the revealed item's edge.
    pub(super) fn move_vertically(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_caret() {
            if extend {
                self.place_caret_on_screen(cx);
            } else {
                self.scroll_lines(down, window, cx);
            }
            return;
        }
        let Some(caret) = self.caret() else {
            return;
        };
        let Some((layout, item_start)) = self.revealed_layout() else {
            self.move_caret(caret, extend, cx);
            return;
        };
        let Some(position) = layout.position_for_index(caret - item_start) else {
            return;
        };
        let Some(open) = &mut self.open else {
            return;
        };
        let line_height = layout.line_height();
        let goal_x = *open.goal_x.get_or_insert(position.x);
        let target_y = if down {
            position.y + line_height * 1.5
        } else {
            position.y - line_height * 0.5
        };
        let bounds = layout.bounds();

        if target_y >= bounds.top() && target_y < bounds.bottom() {
            let (Ok(index) | Err(index)) = layout.index_for_position(point(goal_x, target_y));
            self.move_caret(item_start + index, extend, cx);
            return;
        }

        // Past the edge of what's revealed: reveal the neighbor, then place
        // the caret on its first or last line once it's laid out.
        let Some(neighbor) = open.editor.vertical_neighbor(down) else {
            let edge = if down { open.editor.text().len() } else { 0 };
            self.move_caret(edge, extend, cx);
            return;
        };
        self.move_caret(neighbor, extend, cx);
        let goal = if down {
            LineGoal::First
        } else {
            LineGoal::Last
        };
        let view = cx.entity().downgrade();
        window.on_next_frame(move |_, cx| {
            view.update(cx, |this, cx| {
                this.resolve_line_goal(goal, goal_x, extend, cx)
            })
            .ok();
        });
    }

    /// Places the caret at `goal_x` on the revealed item's first or last
    /// line, after it has been laid out.
    fn resolve_line_goal(
        &mut self,
        goal: LineGoal,
        goal_x: Pixels,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((layout, item_start)) = self.revealed_layout() else {
            return;
        };
        let bounds = layout.bounds();
        let half_line = layout.line_height() * 0.5;
        let y = match goal {
            LineGoal::First => bounds.top() + half_line,
            LineGoal::Last => bounds.bottom() - half_line,
        };
        let (Ok(index) | Err(index)) = layout.index_for_position(point(goal_x, y));
        self.move_caret(item_start + index, extend, cx);
        if let Some(open) = &mut self.open {
            open.goal_x = Some(goal_x);
        }
    }

    /// The start or end of the caret's visual line.
    pub(super) fn line_edge(&self, end: bool) -> Option<usize> {
        let caret = self.caret()?;
        let (layout, item_start) = self.revealed_layout()?;
        let position = layout.position_for_index(caret - item_start)?;
        let bounds = layout.bounds();
        let y = position.y + layout.line_height() * 0.5;
        let x = if end {
            bounds.right() + px(10_000.)
        } else {
            bounds.left() - px(10_000.)
        };
        let (Ok(index) | Err(index)) = layout.index_for_position(point(x, y));
        Some(item_start + index)
    }

    pub(super) fn move_to_line_edge(&mut self, end: bool, extend: bool, cx: &mut Context<Self>) {
        if !self.has_caret() {
            self.place_caret_on_screen(cx);
            return;
        }
        let Some(target) = self.line_edge(end).or(self.caret()) else {
            return;
        };
        if let Some(open) = &mut self.open {
            open.goal_x = None;
        }
        self.move_caret(target, extend, cx);
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.editor.break_group();
        let selection = open.editor.all();
        self.set_selection(selection, cx);
    }

    // With no caret, keys read the document, as in Preview or Safari (#99):
    // ↑, ↓, ⌘↑ and ⌘↓ scroll, and keys that mean editing start at the
    // first block on screen rather than wherever the caret last was.

    /// Whether there's a caret: whether a block is being edited.
    pub fn has_caret(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.editor.selection().is_some())
    }

    /// Scrolls to the top or bottom, for ⌘↑ and ⌘↓ with no caret.
    fn scroll_to_end(&mut self, end: bool, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        open.list_state.scroll_to(ListOffset {
            item_ix: if end { open.list_state.item_count() } else { 0 },
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    /// Starts editing at the first block on screen, as Option-↓ does.
    fn place_caret_on_screen(&mut self, cx: &mut Context<Self>) {
        self.move_to_block(true, false, cx);
    }

    /// Moves the caret to the start of the next or previous block's text.
    /// With no caret, starts at the first block on screen.
    pub(super) fn move_to_block(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        // The footnotes after the items have no caret positions, so start
        // from the last item.
        let first_visible = open
            .list_state
            .logical_scroll_top()
            .item_ix
            .min(open.editor.model().items.len() - 1);
        let Some(offset) = open.editor.block_target(down, first_visible) else {
            return;
        };
        open.goal_x = None;
        self.move_caret(offset, extend, cx);
    }
}

/// Reveals the items the selection touches (or none), re-measuring the
/// items that change. Returns whether anything changed.
pub(super) fn reveal(open: &mut OpenDocument) -> bool {
    let targets = open.editor.reveal_targets();
    if open
        .revealed
        .iter()
        .map(|revealed| &revealed.target)
        .eq(&targets)
    {
        return false;
    }

    // Re-measure the items that are revealed differently now, keeping
    // their heights as the list's guesses until they're measured again.
    // Not every revealed item: extending a selection over many blocks
    // would make the list guess the heights of those scrolled out of view,
    // which it needs to scroll to the caret (#98).
    let old: Vec<&RevealTarget> = open
        .revealed
        .iter()
        .map(|revealed| &revealed.target)
        .collect();
    let removed = old
        .iter()
        .copied()
        .filter(|target| !targets.contains(target));
    let added = targets.iter().filter(|target| !old.contains(target));
    let mut changed: Vec<usize> = removed.chain(added).map(|target| target.index).collect();
    changed.sort_unstable();
    changed.dedup();
    for index in changed {
        open.list_state.remeasure_items(index..index + 1);
    }
    *open.revealed_layout.borrow_mut() = None;
    let model = open.editor.model();
    open.revealed = targets
        .into_iter()
        .map(|target| {
            let source = &model.text[target.range.clone()];
            let styled = styled_source(source, target.kind, &model.definitions);
            Revealed {
                target,
                styled: Rc::new(styled),
            }
        })
        .collect();
    true
}
