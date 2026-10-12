//! The view that shows a document in a window.
//!
//! Every block is rendered except the one containing the caret, which is
//! revealed as styled Markdown source (C4). See
//! `docs/planning/caret-and-reveal.md`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, FocusHandle, KeyBinding, ListAlignment, ListState, PathPromptOptions, Pixels,
    PromptLevel, Subscription, TextLayout, Window, actions, div, list, point, prelude::*, px,
    relative,
};
use kami::blocks::{BlockKind, Blocks};
use kami::document::{Document, OpenError};
use kami::movement::{self, Selection};
use kami::rendered::{RenderedBlock, render_blocks};
use kami::reveal::{StyledSource, styled_source};

use crate::rendered_block::{ClickTarget, render_block};
use crate::revealed_block::{RevealedBlock, render_revealed};
use crate::theme::Theme;

actions!(
    kami,
    [
        Open,
        CloseWindow,
        Unreveal,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveWordLeft,
        MoveWordRight,
        MoveLineStart,
        MoveLineEnd,
        MoveDocumentStart,
        MoveDocumentEnd,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
    ]
);

/// Key bindings for caret movement and selection (macOS conventions).
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", Unreveal, None),
        KeyBinding::new("left", MoveLeft, None),
        KeyBinding::new("right", MoveRight, None),
        KeyBinding::new("up", MoveUp, None),
        KeyBinding::new("down", MoveDown, None),
        KeyBinding::new("alt-left", MoveWordLeft, None),
        KeyBinding::new("alt-right", MoveWordRight, None),
        KeyBinding::new("cmd-left", MoveLineStart, None),
        KeyBinding::new("cmd-right", MoveLineEnd, None),
        KeyBinding::new("home", MoveLineStart, None),
        KeyBinding::new("end", MoveLineEnd, None),
        KeyBinding::new("cmd-up", MoveDocumentStart, None),
        KeyBinding::new("cmd-down", MoveDocumentEnd, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("shift-up", SelectUp, None),
        KeyBinding::new("shift-down", SelectDown, None),
        KeyBinding::new("alt-shift-left", SelectWordLeft, None),
        KeyBinding::new("alt-shift-right", SelectWordRight, None),
        KeyBinding::new("cmd-shift-left", SelectLineStart, None),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, None),
        KeyBinding::new("shift-home", SelectLineStart, None),
        KeyBinding::new("shift-end", SelectLineEnd, None),
        KeyBinding::new("cmd-shift-up", SelectDocumentStart, None),
        KeyBinding::new("cmd-shift-down", SelectDocumentEnd, None),
    ]
}

/// Maximum width of the text column (A1).
pub const MAX_TEXT_WIDTH: f32 = 720.;
pub const BODY_TEXT_SIZE: f32 = 16.;
pub const CODE_FONT: &str = "Menlo";

/// How long the caret stays on, and then off, while blinking.
const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// An open document and everything needed to display it.
struct OpenDocument {
    #[expect(dead_code, reason = "read by the save and editing stories")]
    document: Document,
    text: Rc<str>,
    blocks: Rc<Blocks>,
    rendered: Rc<Vec<RenderedBlock>>,
    /// The source of the document's link reference definitions.
    definitions: Rc<str>,
    list_state: ListState,
    /// The selection, or `None` before the first click or key press.
    selection: Option<Selection>,
    /// The x position to keep when moving up and down.
    goal_x: Option<Pixels>,
    revealed: Option<Revealed>,
    /// The revealed block's text layout from the last frame.
    revealed_layout: Rc<RefCell<Option<TextLayout>>>,
}

/// The block shown as source.
struct Revealed {
    index: usize,
    styled: Rc<StyledSource>,
}

/// Where to put the caret after a neighbor block is revealed and laid out.
#[derive(Clone, Copy)]
enum LineGoal {
    First,
    Last,
}

pub struct DocumentView {
    focus_handle: FocusHandle,
    open: Option<OpenDocument>,
    theme: Theme,
    /// Whether the blinking caret is currently drawn.
    caret_visible: bool,
    /// Bumped to stop the current blink loop.
    blink_epoch: usize,
    _appearance_observer: Subscription,
}

impl DocumentView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        // Follow the macOS appearance, switching live when it changes.
        let appearance_observer = cx.observe_window_appearance(window, |this, window, cx| {
            this.theme = Theme::for_appearance(window.appearance());
            cx.notify();
        });
        Self {
            focus_handle,
            open: None,
            theme: Theme::for_appearance(window.appearance()),
            caret_visible: true,
            blink_epoch: 0,
            _appearance_observer: appearance_observer,
        }
    }

    /// Opens the file at `path`, replacing the current document. Shows an
    /// alert if it can't be opened, and keeps the current document.
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match Document::open(path) {
            Ok(document) => {
                window.set_window_title(&document.display_name());
                let text: Rc<str> = document.buffer().rope().to_string().into();
                let blocks = Blocks::parse(&text);
                let rendered = render_blocks(&text, &blocks);
                let definitions = blocks
                    .as_slice()
                    .iter()
                    .filter(|block| block.kind == BlockKind::LinkReferenceDefinition)
                    .map(|block| &text[block.range.clone()])
                    .collect::<Vec<_>>()
                    .join("\n");
                let list_state = ListState::new(rendered.len(), ListAlignment::Top, px(1000.));
                self.open = Some(OpenDocument {
                    document,
                    text,
                    blocks: Rc::new(blocks),
                    rendered: Rc::new(rendered),
                    definitions: definitions.into(),
                    list_state,
                    selection: None,
                    goal_x: None,
                    revealed: None,
                    revealed_layout: Rc::default(),
                });
                cx.notify();
            }
            Err(err) => show_open_error(&err, window, cx),
        }
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            // The outer error means the dialog was dropped, the inner one that
            // the platform couldn't show it; `None` means the user cancelled.
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| this.open_path(path, window, cx))
                .ok();
        })
        .detach();
    }

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
        }
        self.set_caret(offset, extend, cx);
    }

    /// Moves the caret to `offset`, revealing its block.
    fn set_caret(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let mut selection = open.selection.unwrap_or(Selection::caret(offset));
        selection.move_to(offset, extend);
        open.selection = Some(selection);

        let index = open.blocks.index_at(offset);
        if open.revealed.as_ref().map(|revealed| revealed.index) != index {
            // Both the old and new blocks change height, so re-measure them.
            if let Some(old) = open.revealed.take() {
                open.list_state.splice(old.index..old.index + 1, 1);
            }
            *open.revealed_layout.borrow_mut() = None;
            if let Some(index) = index {
                let block = &open.blocks.as_slice()[index];
                let source = &open.text[block.range.clone()];
                let styled = styled_source(source, block.kind, &open.definitions);
                open.revealed = Some(Revealed {
                    index,
                    styled: Rc::new(styled),
                });
                open.list_state.splice(index..index + 1, 1);
            }
        }
        if let Some(index) = index {
            open.list_state.scroll_to_reveal_item(index);
        }
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

    fn stop_blink(&mut self) {
        self.blink_epoch += 1;
    }

    fn unreveal(&mut self, _: &Unreveal, _: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        open.selection = None;
        open.goal_x = None;
        if let Some(old) = open.revealed.take() {
            open.list_state.splice(old.index..old.index + 1, 1);
        }
        *open.revealed_layout.borrow_mut() = None;
        self.stop_blink();
        cx.notify();
    }

    /// The caret's offset, placing it at the document start if there's no
    /// caret yet.
    fn caret(&self) -> Option<usize> {
        let open = self.open.as_ref()?;
        Some(match open.selection {
            Some(selection) => selection.head,
            None => movement::document_start(&open.blocks),
        })
    }

    /// Applies a layout-independent movement. `forward` is the direction,
    /// used to collapse a selection to the matching edge (as on macOS).
    fn move_by(
        &mut self,
        extend: bool,
        forward: bool,
        cx: &mut Context<Self>,
        motion: impl FnOnce(&str, &Blocks, usize) -> usize,
    ) {
        let (Some(open), Some(caret)) = (&self.open, self.caret()) else {
            return;
        };
        let target = match open.selection {
            // The first key press just places the caret.
            None => caret,
            Some(selection) if !extend && !selection.is_empty() => {
                let range = selection.range();
                if forward { range.end } else { range.start }
            }
            Some(_) => motion(&open.text, &open.blocks, caret),
        };
        if let Some(open) = &mut self.open {
            open.goal_x = None;
        }
        self.set_caret(target, extend, cx);
    }

    /// Moves the caret up or down a visual line, into the neighbor block at
    /// the revealed block's edge.
    fn move_vertically(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(caret) = self.caret() else {
            return;
        };
        let Some(open) = &mut self.open else {
            return;
        };
        let layout = open.revealed_layout.borrow().clone();
        let (Some(layout), Some(revealed), Some(_)) = (layout, &open.revealed, open.selection)
        else {
            self.set_caret(caret, extend, cx);
            return;
        };
        let block_start = open.blocks.as_slice()[revealed.index].range.start;
        let Some(position) = layout.position_for_index(caret - block_start) else {
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
            self.set_caret(block_start + index, extend, cx);
            return;
        }

        // Past the block's edge: reveal the neighbor, then place the caret on
        // its first or last line once it's laid out.
        let neighbor = if down {
            movement::next_block_start(&open.blocks, caret)
        } else {
            movement::prev_block_end(&open.blocks, caret)
        };
        let Some(neighbor) = neighbor else {
            let edge = if down {
                movement::document_end(&open.blocks)
            } else {
                movement::document_start(&open.blocks)
            };
            self.set_caret(edge, extend, cx);
            return;
        };
        self.set_caret(neighbor, extend, cx);
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

    /// Places the caret at `goal_x` on the revealed block's first or last
    /// line, after it has been laid out.
    fn resolve_line_goal(
        &mut self,
        goal: LineGoal,
        goal_x: Pixels,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.open else {
            return;
        };
        let (Some(layout), Some(revealed)) =
            (open.revealed_layout.borrow().clone(), &open.revealed)
        else {
            return;
        };
        let block_start = open.blocks.as_slice()[revealed.index].range.start;
        let bounds = layout.bounds();
        let half_line = layout.line_height() * 0.5;
        let y = match goal {
            LineGoal::First => bounds.top() + half_line,
            LineGoal::Last => bounds.bottom() - half_line,
        };
        let (Ok(index) | Err(index)) = layout.index_for_position(point(goal_x, y));
        self.set_caret(block_start + index, extend, cx);
        if let Some(open) = &mut self.open {
            open.goal_x = Some(goal_x);
        }
    }

    /// Moves the caret to the start or end of its visual line.
    fn move_to_line_edge(&mut self, end: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(caret) = self.caret() else {
            return;
        };
        let Some(open) = &self.open else {
            return;
        };
        let layout = open.revealed_layout.borrow().clone();
        let (Some(layout), Some(revealed), Some(_)) = (layout, &open.revealed, open.selection)
        else {
            self.set_caret(caret, extend, cx);
            return;
        };
        let block_start = open.blocks.as_slice()[revealed.index].range.start;
        let Some(position) = layout.position_for_index(caret - block_start) else {
            return;
        };
        let bounds = layout.bounds();
        let y = position.y + layout.line_height() * 0.5;
        let x = if end {
            bounds.right() + px(10_000.)
        } else {
            bounds.left() - px(10_000.)
        };
        let (Ok(index) | Err(index)) = layout.index_for_position(point(x, y));
        if let Some(open) = &mut self.open {
            open.goal_x = None;
        }
        self.set_caret(block_start + index, extend, cx);
    }
}

fn show_open_error(err: &OpenError, window: &mut Window, cx: &mut Context<DocumentView>) {
    let message = format!("Couldn't open “{}”", err.display_name());
    let detail = capitalize(&err.source.to_string());
    let answer = window.prompt(PromptLevel::Critical, &message, Some(&detail), &["OK"], cx);
    cx.spawn(async move |_, _| answer.await.ok()).detach();
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.open {
            Some(open) => {
                let text = open.text.clone();
                let blocks = open.blocks.clone();
                let rendered = open.rendered.clone();
                let selection = open.selection;
                let revealed = open
                    .revealed
                    .as_ref()
                    .map(|revealed| (revealed.index, revealed.styled.clone()));
                let layout_slot = open.revealed_layout.clone();
                let caret_visible = self.caret_visible;
                let base_font = window.text_style().font();
                let theme = self.theme;
                let view = cx.entity().downgrade();
                list(open.list_state.clone(), move |index, _, _| {
                    let range = blocks.as_slice()[index].range.clone();
                    let click = ClickTarget {
                        view: view.clone(),
                        block_start: range.start,
                    };
                    match &revealed {
                        Some((revealed_index, styled)) if *revealed_index == index => {
                            let selection = selection.unwrap_or_default();
                            let local =
                                |offset: usize| offset.clamp(range.start, range.end) - range.start;
                            let selected = selection.range();
                            let caret = (caret_visible
                                && (range.contains(&selection.head)
                                    || selection.head == range.end))
                                .then(|| selection.head - range.start);
                            render_revealed(
                                RevealedBlock {
                                    source: &text[range.clone()],
                                    styled,
                                    selection: local(selected.start)..local(selected.end),
                                    caret,
                                    first: index == 0,
                                },
                                &base_font,
                                theme,
                                click,
                                layout_slot.clone(),
                            )
                        }
                        _ => render_block(&rendered[index], index == 0, &base_font, theme, click),
                    }
                })
                .size_full()
                .into_any_element()
            }
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(self.theme.muted_text)
                .child("Open a Markdown file with ⌘O")
                .into_any_element(),
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open))
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .on_action(cx.listener(Self::unreveal))
            .on_action(cx.listener(|this, _: &MoveLeft, _, cx| {
                this.move_by(false, false, cx, movement::move_left)
            }))
            .on_action(cx.listener(|this, _: &MoveRight, _, cx| {
                this.move_by(false, true, cx, movement::move_right)
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| {
                this.move_by(true, false, cx, movement::move_left)
            }))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.move_by(true, true, cx, movement::move_right)
            }))
            .on_action(cx.listener(|this, _: &MoveWordLeft, _, cx| {
                this.move_by(false, false, cx, movement::move_word_left)
            }))
            .on_action(cx.listener(|this, _: &MoveWordRight, _, cx| {
                this.move_by(false, true, cx, movement::move_word_right)
            }))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| {
                this.move_by(true, false, cx, movement::move_word_left)
            }))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| {
                this.move_by(true, true, cx, movement::move_word_right)
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentStart, _, cx| {
                this.move_by(false, false, cx, |_, blocks, _| {
                    movement::document_start(blocks)
                })
            }))
            .on_action(cx.listener(|this, _: &MoveDocumentEnd, _, cx| {
                this.move_by(false, true, cx, |_, blocks, _| {
                    movement::document_end(blocks)
                })
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentStart, _, cx| {
                this.move_by(true, false, cx, |_, blocks, _| {
                    movement::document_start(blocks)
                })
            }))
            .on_action(cx.listener(|this, _: &SelectDocumentEnd, _, cx| {
                this.move_by(true, true, cx, |_, blocks, _| {
                    movement::document_end(blocks)
                })
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
            .size_full()
            .bg(self.theme.background)
            .text_color(self.theme.text)
            .text_size(px(BODY_TEXT_SIZE))
            .line_height(relative(1.6))
            .child(content)
    }
}
