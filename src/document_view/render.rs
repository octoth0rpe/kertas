//! Drawing the document view, its actions, and the settings problems bar.

use std::path::Path;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, Div, ImgResourceLoader, Pixels, Rems, Role, SharedString, Stateful,
    SystemWindowTabController, Window, div, list, prelude::*, px, relative, rems,
};
use kertas::blocks::BlockKind;
use kertas::editor::{Motion, RevealTarget};
use kertas::settings::TextWidth;

use super::heights::Heights;
use super::{
    Backspace, Delete, DeleteWordLeft, DeleteWordRight, DocumentView, IndentLines, IndentTab,
    InsertTab, LineBreak, MoveBlockDown, MoveBlockUp, MoveDocumentEnd, MoveDocumentStart, MoveDown,
    MoveLeft, MoveLineEnd, MoveLineStart, MoveRight, MoveUp, MoveWordLeft, MoveWordRight, Newline,
    NextTab, OpenDocument, OutdentLines, OutdentTab, PageDown, PageUp, PreviousTab, ResetZoom,
    Revealed, ScrollRequest, SelectBlockDown, SelectBlockUp, SelectDocumentEnd,
    SelectDocumentStart, SelectDown, SelectLeft, SelectLineEnd, SelectLineStart, SelectRight,
    SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5, SelectTab6, SelectTab7, SelectTab8,
    SelectTab9, SelectUp, SelectWordLeft, SelectWordRight, WebImages, ZoomIn, ZoomOut,
};
use crate::app_settings::{self, AppSettings};
use crate::rendered_block::{
    ClickTarget, ReferenceTarget, note_marker_column, render_block, render_note,
};
use crate::revealed_block::{
    CaretPlace, RevealedBlock, ScrollToCaret, TextInput, render_gap, render_revealed,
};
use crate::tab_strip::{render_tab_strip, select_tab};
use crate::text_style::Look;
use crate::zoom;

/// The text column's maximum width for each `text_width` setting (A1), or
/// `None` to use the whole window.
/// Widths are in rems, so zooming keeps the same number of characters per
/// line: 45 rems is 720 px at the default 16 px.
pub(super) fn column_width(width: TextWidth) -> Option<Rems> {
    match width {
        TextWidth::Standard => Some(rems(45.)),
        TextWidth::Wide => Some(rems(60.)),
        TextWidth::Full => None,
    }
}

/// The space between the window's sides and the text column, at its
/// narrowest (`block_frame`'s padding).
const SIDE_PADDING: Pixels = px(32.);

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
    /// Where the text column is across a list `width` wide: how far its
    /// left edge is from the list's, and how wide it is.
    pub(super) fn text_column(&self, width: Pixels, rem: Pixels) -> (Pixels, Pixels) {
        let available = width - SIDE_PADDING * 2.;
        let column = column_width(self.text_settings.text_width)
            .map_or(available, |column| column.to_pixels(rem).min(available));
        (SIDE_PADDING + (available - column) / 2., column)
    }

    /// Where item `index`'s text starts in the window: the top left of its
    /// text column, for the smoke test to click on.
    #[cfg(debug_assertions)]
    pub fn text_origin(&self, index: usize, window: &Window) -> Option<gpui::Point<gpui::Pixels>> {
        let bounds = self.open.as_ref()?.list_state.bounds_for_item(index)?;
        let (left, _) = self.text_column(bounds.size.width, window.rem_size());
        // The first item has the side padding above it too.
        let top = if index == 0 { SIDE_PADDING } else { px(0.) };
        Some(gpui::point(bounds.left() + left, bounds.top() + top))
    }

    /// A bar at the bottom of the window reporting problems in the settings
    /// file (G4) and theme files, until the user closes it.
    fn render_settings_problems(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.settings_problems_dismissed {
            return None;
        }
        let state = cx.try_global::<AppSettings>()?;
        let count = state.problems.len() + state.theme_problems.len();
        let mut message = match state.problems.first() {
            Some(first) => format!("Settings file, {first}"),
            None => state.theme_problems.first()?.to_string(),
        };
        if count > 1 {
            message.push_str(&format!(" (and {} more)", count - 1));
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

    /// A bar at the top of the window asking whether to load the
    /// document's web images (#97), if it has any and the user hasn't
    /// answered.
    fn render_web_images_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.open.as_ref()?;
        if self.web_images != WebImages::Ask || !open.editor.model().web_images {
            return None;
        }
        let message = "This document has images from the web.";
        let theme = self.theme;
        Some(
            div()
                .id("web-images")
                .role(Role::Group)
                .aria_label(message)
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py(px(6.))
                .border_b_1()
                .border_color(theme.rule)
                .bg(theme.table_header_background)
                .text_size(px(12.))
                .line_height(px(16.))
                .child(div().flex_1().child(message))
                .child(
                    bar_button(theme, "load-web-images", "Load Images", "Load Images").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.answer_web_images(WebImages::Load, window, cx)
                        }),
                    ),
                )
                .child(
                    bar_button(theme, "close-web-images", "×", "Don't load images")
                        .px_1()
                        .py_0()
                        .border_color(gpui::transparent_black())
                        .bg(gpui::transparent_black())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.answer_web_images(WebImages::Declined, window, cx)
                        })),
                )
                .into_any_element(),
        )
    }

    /// Whether the bar is asking whether to load web images, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    pub fn web_images_asked(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| self.web_images == WebImages::Ask && open.editor.model().web_images)
    }

    /// Answers the web images bar as its buttons do, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn answer_web_images_bar(
        &mut self,
        load: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = if load {
            WebImages::Load
        } else {
            WebImages::Declined
        };
        self.answer_web_images(answer, window, cx);
    }

    /// Loads the document's web images or not, as the user answered.
    fn answer_web_images(
        &mut self,
        answer: WebImages,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.web_images = answer;
        // The bar closes, so focus would be left on nothing.
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The document's blocks: each rendered, except the revealed ones,
    /// which show their source.
    fn render_document(
        &self,
        open: &OpenDocument,
        request: Option<ScrollRequest>,
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
        let document: Option<Rc<Path>> = open.editor.document().path().map(Rc::from);
        let loading_images = self.loading_images.clone();
        let web_images = self.web_images == WebImages::Load;
        // What this frame scrolls to (see `ScrollRequest`).
        let viewport_height = open.list_state.viewport_bounds().size.height;
        let reference = match request {
            Some(ScrollRequest::Position(offset)) => Some(ReferenceTarget {
                offset,
                viewport_height,
            }),
            _ => None,
        };
        let caret_place = match request {
            Some(ScrollRequest::Caret(place)) => Some(place),
            _ => None,
        };
        let weak_view = view.downgrade();
        let click_target = move |block_start: usize| ClickTarget {
            view: weak_view.clone(),
            block_start,
            column_width,
            editable,
            document: document.clone(),
            loading_images: loading_images.clone(),
            web_images,
            reference,
        };
        let note_column = note_marker_column(look.list_indent, &model.notes);
        let list = list(open.list_state.clone(), move |index, _, _| {
            // The footnotes come after the items (#90).
            let Some(item) = model.items.get(index) else {
                let number = index - model.items.len();
                let note = &model.notes[number];
                return render_note(
                    note,
                    number == 0,
                    note_column,
                    &look,
                    click_target(note.block_start),
                );
            };
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
                    let scroll_to_caret = caret_place.filter(|_| *head).map(|place| {
                        ScrollToCaret::new(local(selection.head), place, viewport_height)
                    });
                    let input = head.then(|| TextInput {
                        focus_handle: focus_handle.clone(),
                        view: view.clone(),
                        layout_slot: layout_slot.clone(),
                        hint,
                        scroll_to_caret: scroll_to_caret.clone(),
                    });
                    let element = match gap {
                        // Clicks on the block place the caret in it.
                        Some(gap) if let Some(input) = input => render_gap(
                            &item.rendered,
                            *gap,
                            block,
                            &look,
                            click_target(item.block.start),
                            input,
                        ),
                        _ => render_revealed(block, &look, click_target(range.start), input),
                    };
                    match scroll_to_caret {
                        Some(scroll) => scroll.track_item(element),
                        None => element,
                    }
                }
                _ => render_block(
                    &item.rendered,
                    index == 0,
                    &look,
                    click_target(item.block.start),
                ),
            }
        })
        .size_full();
        let scrollbar = app_settings::get(cx)
            .scrollbar
            .then(|| self.render_scrollbar(open, window, cx));
        div()
            .relative()
            .size_full()
            .child(list)
            .children(scrollbar)
            .into_any_element()
    }

    /// Re-measures the blocks whose images have arrived since they were
    /// drawn (#88): they were measured without them, so the scrollbar
    /// would be wrong about the document's length. A caret that was on
    /// screen stays there, though the blocks above it grow.
    fn remeasure_loaded_images(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let caret_on_screen = open.editor.selection().is_some_and(|selection| {
            let index = open.editor.model().item_at(selection.head);
            Heights::measured(&open.list_state)
                .on_screen(index)
                .is_some()
        });
        let mut remeasured = false;
        self.loading_images
            .borrow_mut()
            .retain(|(block_start, resource)| {
                if cx.fetch_asset::<ImgResourceLoader>(resource).is_none() {
                    return true;
                }
                let model = open.editor.model();
                let index = model.item_at(*block_start);
                open.list_state.remeasure_items(index..index + 1);
                // The image may be in a footnote, drawn after the items.
                if model.items[index].kind == BlockKind::FootnoteDefinition {
                    let start = model.items.len();
                    open.list_state
                        .remeasure_items(start..start + model.notes.len());
                }
                remeasured = true;
                false
            });
        if remeasured && caret_on_screen && open.scroll_request.is_none() {
            open.scroll_request = Some(ScrollRequest::Caret(CaretPlace::Nearest));
        }
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
            .capture_key_down(cx.listener(|this, event, _, _| this.line_scroll_key_down(event)))
            .on_key_up(cx.listener(|this, event, _, _| this.line_scroll_key_up(event)))
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
        self.remeasure_loaded_images(cx);
        // This frame's elements ask the list to scroll to what was
        // requested as it's laid out.
        let request = self
            .open
            .as_mut()
            .and_then(|open| open.scroll_request.take());
        if let Some(ScrollRequest::Caret(_)) = request {
            let view = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                view.update(cx, |this, cx| this.after_layout(request, cx))
                    .ok();
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
        let web_images_bar = self.render_web_images_bar(cx);
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
            Some(open) => self.render_document(open, request, window, cx),
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
            .children(web_images_bar)
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
