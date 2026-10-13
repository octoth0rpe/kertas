//! Drawing the document view, its actions, and the settings problems bar.

use gpui::{
    AnyElement, Context, Div, Rems, Role, SharedString, Stateful, SystemWindowTabController,
    Window, div, list, prelude::*, px, relative, rems,
};
use kertas::editor::{Motion, RevealTarget};
use kertas::settings::TextWidth;

use super::{
    Backspace, Delete, DeleteWordLeft, DeleteWordRight, DocumentView, IndentLines, IndentTab,
    InsertTab, LineBreak, MoveBlockDown, MoveBlockUp, MoveDocumentEnd, MoveDocumentStart, MoveDown,
    MoveLeft, MoveLineEnd, MoveLineStart, MoveRight, MoveUp, MoveWordLeft, MoveWordRight, Newline,
    NextTab, OpenDocument, OutdentLines, OutdentTab, PageDown, PageUp, PreviousTab, ResetZoom,
    Revealed, SelectBlockDown, SelectBlockUp, SelectDocumentEnd, SelectDocumentStart, SelectDown,
    SelectLeft, SelectLineEnd, SelectLineStart, SelectRight, SelectTab1, SelectTab2, SelectTab3,
    SelectTab4, SelectTab5, SelectTab6, SelectTab7, SelectTab8, SelectTab9, SelectUp,
    SelectWordLeft, SelectWordRight, ZoomIn, ZoomOut,
};
use crate::app_settings::{self, AppSettings};
use crate::rendered_block::{ClickTarget, render_block};
use crate::revealed_block::{RevealedBlock, TextInput, render_gap, render_revealed};
use crate::tab_strip::{render_tab_strip, select_tab};
use crate::text_style::Look;
use crate::zoom;

/// The text column's maximum width for each `text_width` setting (A1), or
/// `None` to use the whole window.
/// Widths are in rems, so zooming keeps the same number of characters per
/// line: 45 rems is 720 px at the default 16 px.
fn column_width(width: TextWidth) -> Option<Rems> {
    match width {
        TextWidth::Standard => Some(rems(45.)),
        TextWidth::Wide => Some(rems(60.)),
        TextWidth::Full => None,
    }
}

/// A button in one of the bars at the top and bottom of the window,
/// showing `label`. `name` is what it's called for VoiceOver. It's
/// reachable with Tab when no block is being edited, and ringed when it
/// has keyboard focus.
pub(super) fn bar_button(
    theme: crate::theme::Theme,
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    name: impl Into<SharedString>,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .focusable()
        .tab_stop(true)
        .role(Role::Button)
        .aria_label(name)
        .flex_none()
        .px_2()
        .py(px(2.))
        .rounded_md()
        .cursor_pointer()
        .border_1()
        .border_color(theme.rule)
        .bg(theme.background)
        .hover(|style| style.bg(theme.rule))
        .focus_visible(move |style| style.border_color(theme.caret))
        .child(label.into())
}

impl DocumentView {
    /// Where item `index`'s text starts in the window: the top left of its
    /// text column, for the smoke test to click on.
    #[cfg(debug_assertions)]
    pub fn text_origin(&self, index: usize, window: &Window) -> Option<gpui::Point<gpui::Pixels>> {
        let bounds = self.open.as_ref()?.list_state.bounds_for_item(index)?;
        // The frame's padding (see `block_frame`).
        let padding = px(32.);
        let available = bounds.size.width - padding * 2.;
        let column = column_width(self.text_settings.text_width).map_or(available, |width| {
            width.to_pixels(window.rem_size()).min(available)
        });
        let top = if index == 0 { padding } else { px(0.) };
        Some(gpui::point(
            bounds.left() + padding + (available - column) / 2.,
            bounds.top() + top,
        ))
    }

    /// A bar at the bottom of the window reporting problems in the settings
    /// file (G4), until the user closes it.
    fn render_settings_problems(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.settings_problems_dismissed {
            return None;
        }
        let problems = &cx.try_global::<AppSettings>()?.problems;
        let first = problems.first()?;
        let mut message = format!("Settings file, {first}");
        if problems.len() > 1 {
            message.push_str(&format!(" (and {} more)", problems.len() - 1));
        }
        let theme = self.theme;
        Some(
            div()
                .id("settings-problems")
                .role(Role::Status)
                .aria_label(message.clone())
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py(px(6.))
                .border_t_1()
                .border_color(theme.rule)
                .bg(theme.table_header_background)
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(theme.muted_text)
                .child(div().flex_1().child(message))
                .child(
                    bar_button(
                        theme,
                        "dismiss-settings-problems",
                        "×",
                        "Dismiss settings problems",
                    )
                    .px_1()
                    .py_0()
                    .border_color(gpui::transparent_black())
                    .bg(gpui::transparent_black())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.settings_problems_dismissed = true;
                        // Focus would otherwise be left on nothing.
                        this.focus_handle.focus(window, cx);
                        cx.notify();
                    })),
                )
                .into_any_element(),
        )
    }

    /// The document's blocks: each rendered, except the revealed ones,
    /// which show their source.
    fn render_document(
        &self,
        open: &OpenDocument,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = open.editor.model().clone();
        let selection = open.editor.selection();
        let marked = open.editor.marked();
        let revealed = open.revealed.clone();
        let layout_slot = open.revealed_layout.clone();
        let caret_visible = self.caret_visible;
        let column_width = column_width(self.text_settings.text_width);
        let look = Look::new(window.text_style().font(), self.theme, &self.text_settings);
        let view = cx.entity();
        let focus_handle = self.focus_handle.clone();
        let hint = app_settings::get(cx).editing_hint;
        let editable = !open.editor.read_only();
        list(open.list_state.clone(), move |index, _, _| {
            let item = &model.items[index];
            match revealed
                .iter()
                .find(|revealed| revealed.target.index == index)
            {
                Some(Revealed {
                    target:
                        RevealTarget {
                            range, gap, head, ..
                        },
                    styled,
                }) => {
                    let range = range.clone();
                    let selection = selection.unwrap_or_default();
                    let local = |offset: usize| offset.clamp(range.start, range.end) - range.start;
                    let selected = selection.range();
                    let caret = (caret_visible
                        && *head
                        && (range.contains(&selection.head) || selection.head == range.end))
                        .then(|| selection.head - range.start);
                    let marked = marked
                        .as_ref()
                        .map(|marked| local(marked.start)..local(marked.end));
                    let block = RevealedBlock {
                        source: &model.text[range.clone()],
                        styled,
                        selection: local(selected.start)..local(selected.end),
                        caret,
                        marked,
                        first: index == 0,
                    };
                    let input = head.then(|| TextInput {
                        focus_handle: focus_handle.clone(),
                        view: view.clone(),
                        layout_slot: layout_slot.clone(),
                        hint,
                    });
                    match gap {
                        Some(gap) if let Some(input) = input => render_gap(
                            &item.rendered,
                            *gap,
                            block,
                            &look,
                            // Clicks on the block place the caret in it.
                            ClickTarget {
                                view: view.downgrade(),
                                block_start: item.block.start,
                                column_width,
                                editable,
                            },
                            input,
                        ),
                        _ => render_revealed(
                            block,
                            &look,
                            ClickTarget {
                                view: view.downgrade(),
                                block_start: range.start,
                                column_width,
                                editable,
                            },
                            input,
                        ),
                    }
                }
                _ => render_block(
                    &item.rendered,
                    index == 0,
                    &look,
                    ClickTarget {
                        view: view.downgrade(),
                        block_start: item.block.start,
                        column_width,
                        editable,
                    },
                ),
            }
        })
        .size_full()
        .into_any_element()
    }

    /// The view's actions: files, tabs, zoom, movement and editing.
    fn with_actions(&self, element: Div, cx: &mut Context<Self>) -> Div {
        element
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::print))
            .on_action(cx.listener(Self::close_window))
            .on_action(|_: &NextTab, window, cx| {
                SystemWindowTabController::select_next_tab(cx, window.window_handle().window_id())
            })
            .on_action(|_: &PreviousTab, window, cx| {
                SystemWindowTabController::select_previous_tab(
                    cx,
                    window.window_handle().window_id(),
                )
            })
            .on_action(|_: &SelectTab1, window, cx| select_tab(0, window, cx))
            .on_action(|_: &SelectTab2, window, cx| select_tab(1, window, cx))
            .on_action(|_: &SelectTab3, window, cx| select_tab(2, window, cx))
            .on_action(|_: &SelectTab4, window, cx| select_tab(3, window, cx))
            .on_action(|_: &SelectTab5, window, cx| select_tab(4, window, cx))
            .on_action(|_: &SelectTab6, window, cx| select_tab(5, window, cx))
            .on_action(|_: &SelectTab7, window, cx| select_tab(6, window, cx))
            .on_action(|_: &SelectTab8, window, cx| select_tab(7, window, cx))
            .on_action(|_: &SelectTab9, window, cx| select_tab(8, window, cx))
            // Only while possible, so the menu items are disabled at the
            // limits.
            .when_some(zoom::larger(cx), |this, size| {
                this.on_action(move |_: &ZoomIn, _, cx| zoom::set(size, cx))
            })
            .when_some(zoom::smaller(cx), |this, size| {
                this.on_action(move |_: &ZoomOut, _, cx| zoom::set(size, cx))
            })
            .when(zoom::is_zoomed(cx), |this| {
                this.on_action(|_: &ResetZoom, _, cx| zoom::reset(cx))
            })
            .on_action(cx.listener(Self::unreveal))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.scroll_page(false, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.scroll_page(true, cx)))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.end_drag()),
            )
            .on_action(cx.listener(|this, _: &MoveLeft, _, cx| {
                this.move_by(Motion::Grapheme, false, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveRight, _, cx| {
                this.move_by(Motion::Grapheme, true, false, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| {
                this.move_by(Motion::Grapheme, false, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.move_by(Motion::Grapheme, true, true, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveWordLeft, _, cx| {
                this.move_by(Motion::Word, false, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveWordRight, _, cx| {
                this.move_by(Motion::Word, true, false, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| {
                this.move_by(Motion::Word, false, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| {
                this.move_by(Motion::Word, true, true, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentStart, _, cx| {
                this.move_by(Motion::Document, false, false, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentEnd, _, cx| {
                this.move_by(Motion::Document, true, false, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentStart, _, cx| {
                this.move_by(Motion::Document, false, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentEnd, _, cx| {
                this.move_by(Motion::Document, true, true, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveUp, window, cx| {
                this.move_vertically(false, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDown, window, cx| {
                this.move_vertically(true, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectUp, window, cx| {
                this.move_vertically(false, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectDown, window, cx| {
                this.move_vertically(true, true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveLineStart, _, cx| {
                this.move_to_line_edge(false, false, cx)
            }))
            .on_action(
                cx.listener(|this, _: &MoveLineEnd, _, cx| this.move_to_line_edge(true, false, cx)),
            )
            .on_action(cx.listener(|this, _: &SelectLineStart, _, cx| {
                this.move_to_line_edge(false, true, cx)
            }))
            .on_action(
                cx.listener(|this, _: &SelectLineEnd, _, cx| {
                    this.move_to_line_edge(true, true, cx)
                }),
            )
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(|this, _: &Backspace, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Grapheme, false))
            }))
            .on_action(cx.listener(|this, _: &Delete, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Grapheme, true))
            }))
            .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Word, false))
            }))
            .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| {
                this.edit(cx, |editor| editor.delete(Motion::Word, true))
            }))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(
                cx.listener(|this, _: &Newline, _, cx| {
                    this.edit(cx, |editor| editor.newline(false))
                }),
            )
            .on_action(cx.listener(|this, _: &LineBreak, _, cx| {
                this.edit(cx, |editor| editor.newline(true))
            }))
            .on_action(cx.listener(|this, _: &IndentTab, window, cx| {
                if this.is_editing(window) {
                    this.edit(cx, |editor| editor.indent(false, false));
                } else {
                    window.focus_next(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OutdentTab, window, cx| {
                if this.is_editing(window) {
                    this.edit(cx, |editor| editor.indent(true, false));
                } else {
                    window.focus_prev(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &IndentLines, _, cx| {
                this.edit(cx, |editor| editor.indent(false, true))
            }))
            .on_action(cx.listener(|this, _: &OutdentLines, _, cx| {
                this.edit(cx, |editor| editor.indent(true, true))
            }))
            .on_action(
                cx.listener(|this, _: &InsertTab, _, cx| {
                    this.edit(cx, |editor| editor.insert("\t"))
                }),
            )
            .on_action(
                cx.listener(|this, _: &MoveBlockUp, _, cx| this.move_to_block(false, false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveBlockDown, _, cx| this.move_to_block(true, false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectBlockUp, _, cx| this.move_to_block(false, true, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectBlockDown, _, cx| this.move_to_block(true, true, cx)),
            )
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Document text is sized in rems, so this sets the zoom.
        window.set_rem_size(px(zoom::size(cx) as f32));
        if let Some(open) = &self.open
            && (open.anchor.is_some() || open.reveal_caret)
        {
            let view = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                view.update(cx, |this, cx| this.after_layout(cx)).ok();
            });
        }
        // Show unsaved changes in the window's close button.
        let edited = self.is_dirty();
        if self.shown_edited != Some(edited) {
            window.set_window_edited(edited);
            self.shown_edited = Some(edited);
        }
        let tab_strip = render_tab_strip(window, cx, self.theme, edited);
        let disk_bar = self.render_disk_change(cx);
        let problems_bar = self.render_settings_problems(cx);

        let name = match self.builtin {
            Some(builtin) => builtin.title().to_owned(),
            None => self
                .open
                .as_ref()
                .map(|open| open.editor.document().display_name())
                .unwrap_or_default(),
        };
        let content = match &self.open {
            Some(open) => self.render_document(open, window, cx),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(self.theme.muted_text)
                .child("Open a Markdown file with ⌘O, or start a new one with ⌘N")
                .into_any_element(),
        };

        self.with_actions(div().track_focus(&self.focus_handle), cx)
            .size_full()
            .bg(self.theme.background)
            .text_color(self.theme.text)
            .text_size(rems(1.))
            .line_height(relative(1.6))
            .flex()
            .flex_col()
            .children(tab_strip)
            .children(disk_bar)
            .child(
                div()
                    .id("document")
                    .when(self.open.is_some(), |this| {
                        this.role(Role::Document).aria_label(name)
                    })
                    .flex_1()
                    .min_h(px(0.))
                    // List markers and the like follow the body font too.
                    .when_some(self.text_settings.body_font.clone(), |this, font| {
                        this.font_family(font)
                    })
                    // Clicks below the last block finish editing.
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.finish_editing(cx)),
                    )
                    .child(content),
            )
            .children(problems_bar)
    }
}
