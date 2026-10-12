//! Spike A (kami story #3): a minimal multi-line editor on plain GPUI.
//!
//! The input plumbing (action set, `EntityInputHandler` impl, UTF-16
//! conversions, the custom `Element` with prepaint/paint) is adapted from
//! Zed's `crates/gpui/examples/input.rs` (Apache-2.0, Zed Industries, Inc.)
//! at rev c66312b. It's extended here to many lines with a `ropey` buffer,
//! virtualized painting (only visible lines are shaped), scrolling, and a
//! `--bench` mode.

use std::ops::Range;
use std::time::Instant;

use gpui::{
    App, Bounds, ContentMask, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent,
    ShapedLine, Style, TextRun, UTF16Selection, UnderlineStyle, Window, WindowBounds,
    WindowOptions, actions, div, fill, point, prelude::*, px, relative, rgb, rgba, size,
};
use gpui_platform::application;
use ropey::Rope;

actions!(
    spike,
    [
        Backspace,
        Delete,
        Enter,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        Home,
        End,
        Quit
    ]
);

const BENCH_INSERTS: usize = 500;
const BENCH_LINE: usize = 4999;

struct Bench {
    process_start: Instant,
    load_ms: Option<f64>,
    started: bool,
    /// Insert time of the edit awaiting the next frame callback.
    pending_frame: Option<Instant>,
    /// Insert time of the edit awaiting this element's paint.
    pending_paint: Option<Instant>,
    frame_ms: Vec<f64>,
    paint_ms: Vec<f64>,
}

struct Editor {
    focus_handle: FocusHandle,
    rope: Rope,
    /// Byte offsets.
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    scroll_y: Pixels,
    autoscroll: bool,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_height: Pixels,
    /// Shaped visible lines from the last paint, by line index.
    last_lines: Vec<(usize, ShapedLine)>,
    is_selecting: bool,
    bench: Option<Bench>,
}

impl Editor {
    // --- buffer helpers -------------------------------------------------

    fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    fn line_of(&self, offset: usize) -> usize {
        self.rope.byte_to_line(offset)
    }

    fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_byte(line)
    }

    /// A line's text without its line ending.
    fn line_text(&self, line: usize) -> String {
        let mut text = self.rope.line(line).to_string();
        while text.ends_with(['\n', '\r']) {
            text.pop();
        }
        text
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        let char_index = self.rope.byte_to_char(offset);
        if char_index == 0 {
            return 0;
        }
        let mut prev = char_index - 1;
        if prev > 0 && self.rope.char(prev) == '\n' && self.rope.char(prev - 1) == '\r' {
            prev -= 1;
        }
        self.rope.char_to_byte(prev)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let char_index = self.rope.byte_to_char(offset);
        if char_index >= self.rope.len_chars() {
            return self.len();
        }
        let mut next = char_index + 1;
        if self.rope.char(char_index) == '\r'
            && next < self.rope.len_chars()
            && self.rope.char(next) == '\n'
        {
            next += 1;
        }
        self.rope.char_to_byte(next)
    }

    /// The offset one line up or down, keeping the character column.
    fn vertical(&self, offset: usize, down: bool) -> usize {
        let line = self.line_of(offset);
        let target = if down {
            if line + 1 >= self.rope.len_lines() {
                return self.len();
            }
            line + 1
        } else {
            if line == 0 {
                return 0;
            }
            line - 1
        };
        let column = self.rope.byte_to_char(offset) - self.rope.line_to_char(line);
        let target_len = self.line_text(target).chars().count();
        self.rope
            .char_to_byte(self.rope.line_to_char(target) + column.min(target_len))
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.rope.char_to_utf16_cu(self.rope.byte_to_char(offset))
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let offset = offset.min(self.rope.len_utf16_cu());
        self.rope.char_to_byte(self.rope.utf16_cu_to_char(offset))
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn replace_bytes(&mut self, range: Range<usize>, text: &str) {
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        self.rope.remove(start..end);
        self.rope.insert(start, text);
    }

    // --- selection ------------------------------------------------------

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.autoscroll = true;
        cx.notify()
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.autoscroll = true;
        cx.notify()
    }

    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let line_height = self.last_line_height;
        let y = position.y - bounds.top() + self.scroll_y;
        if y < px(0.) {
            return 0;
        }
        let line = (y / line_height).floor() as usize;
        if line >= self.rope.len_lines() {
            return self.len();
        }
        let local = self
            .last_lines
            .iter()
            .find(|(index, _)| *index == line)
            .map(|(_, shaped)| shaped.closest_index_for_x(position.x - bounds.left()))
            .unwrap_or(0);
        self.line_start(line) + local
    }

    // --- actions --------------------------------------------------------

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical(self.cursor_offset(), false), cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical(self.cursor_offset(), true), cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical(self.cursor_offset(), false), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical(self.cursor_offset(), true), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_start(self.line_of(self.cursor_offset())), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.line_of(self.cursor_offset());
        self.move_to(self.line_start(line) + self.line_text(line).len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev {
                return;
            }
            self.select_to(prev, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                return;
            }
            self.select_to(next, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx)
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        let offset = self.index_for_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_position(event.position), cx);
        }
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_y -= event.delta.pixel_delta(self.last_line_height).y;
        cx.notify();
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.rope.byte_slice(range).to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.replace_bytes(range.clone(), new_text);
        let caret = range.start + new_text.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        self.autoscroll = true;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.replace_bytes(range.clone(), new_text);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        // The new selection is relative to the inserted text, in UTF-16.
        self.selected_range = new_selected_range_utf16
            .map(|sel| {
                let base = self.offset_to_utf16(range.start);
                self.offset_from_utf16(base + sel.start)..self.offset_from_utf16(base + sel.end)
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.autoscroll = true;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let line = self.line_of(range.start);
        let start = self.line_start(line);
        let (_, shaped) = self.last_lines.iter().find(|(index, _)| *index == line)?;
        let top = bounds.top() + self.last_line_height * line as f32 - self.scroll_y;
        let end = (range.end - start).min(shaped.len);
        Some(Bounds::from_corners(
            point(bounds.left() + shaped.x_for_index(range.start - start), top),
            point(
                bounds.left() + shaped.x_for_index(end),
                top + self.last_line_height,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_position(point)))
    }
}

struct EditorElement {
    editor: Entity<Editor>,
}

struct PrepaintState {
    lines: Vec<(usize, Point<Pixels>, ShapedLine)>,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    line_height: Pixels,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let line_height = window.line_height();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());

        // Scroll: keep the caret visible after edits/moves, then clamp.
        let (first, last, scroll_y) = self.editor.update(cx, |editor, _| {
            let height = bounds.size.height;
            if editor.autoscroll {
                editor.autoscroll = false;
                let caret_top = line_height * editor.line_of(editor.cursor_offset()) as f32;
                if caret_top < editor.scroll_y {
                    editor.scroll_y = caret_top;
                } else if caret_top + line_height > editor.scroll_y + height {
                    editor.scroll_y = caret_top + line_height - height;
                }
            }
            let content_height = line_height * editor.rope.len_lines() as f32;
            let max_scroll = (content_height - height).max(px(0.));
            editor.scroll_y = editor.scroll_y.clamp(px(0.), max_scroll);

            let first = (editor.scroll_y / line_height).floor() as usize;
            let visible = (height / line_height).ceil() as usize + 1;
            let last = (first + visible).min(editor.rope.len_lines());
            (first, last, editor.scroll_y)
        });

        let editor = self.editor.read(cx);
        let selected = editor.selected_range.clone();
        let cursor_offset = editor.cursor_offset();
        let base_run = TextRun {
            len: 0,
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let mut lines = Vec::with_capacity(last - first);
        let mut selections = Vec::new();
        let mut cursor = None;
        for line in first..last {
            let text = editor.line_text(line);
            let start = editor.line_start(line);
            let end = start + text.len();

            // Underline IME marked text on this line.
            let mut runs = vec![];
            match &editor.marked_range {
                Some(marked) if marked.start >= start && marked.end <= end => {
                    let (a, b) = (marked.start - start, marked.end - start);
                    runs.push(TextRun {
                        len: a,
                        ..base_run.clone()
                    });
                    runs.push(TextRun {
                        len: b - a,
                        underline: Some(UnderlineStyle {
                            color: Some(style.color),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..base_run.clone()
                    });
                    runs.push(TextRun {
                        len: text.len() - b,
                        ..base_run.clone()
                    });
                    runs.retain(|run| run.len > 0);
                }
                _ => runs.push(TextRun {
                    len: text.len(),
                    ..base_run.clone()
                }),
            }
            if text.is_empty() {
                runs.clear();
            }

            let shaped = window
                .text_system()
                .shape_line(text.into(), font_size, &runs, None);
            let origin = point(
                bounds.left(),
                bounds.top() + line_height * line as f32 - scroll_y,
            );

            // Selection, including a sliver for the line ending.
            if !selected.is_empty() && selected.start <= end && selected.end > start {
                let a = selected.start.max(start) - start;
                let b = selected.end.min(end) - start;
                let extra = if selected.end > end { px(6.) } else { px(0.) };
                selections.push(fill(
                    Bounds::from_corners(
                        point(origin.x + shaped.x_for_index(a), origin.y),
                        point(
                            origin.x + shaped.x_for_index(b) + extra,
                            origin.y + line_height,
                        ),
                    ),
                    rgba(0x3311ff30),
                ));
            }
            if selected.is_empty() && cursor_offset >= start && cursor_offset <= end {
                cursor = Some(fill(
                    Bounds::new(
                        point(
                            origin.x + shaped.x_for_index(cursor_offset - start),
                            origin.y,
                        ),
                        size(px(2.), line_height),
                    ),
                    gpui::blue(),
                ));
            }
            lines.push((line, origin, shaped));
        }

        PrepaintState {
            lines,
            selections,
            cursor,
            line_height,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        let line_height = prepaint.line_height;
        let lines = std::mem::take(&mut prepaint.lines);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for quad in prepaint.selections.drain(..) {
                window.paint_quad(quad);
            }
            for (_, origin, shaped) in &lines {
                shaped
                    .paint(
                        *origin,
                        line_height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .unwrap();
            }
            if focus_handle.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });

        let editor = self.editor.clone();
        self.editor.update(cx, |state, _| {
            state.last_bounds = Some(bounds);
            state.last_line_height = line_height;
            state.last_lines = lines.into_iter().map(|(i, _, s)| (i, s)).collect();
            if let Some(bench) = state.bench.as_mut() {
                let now = Instant::now();
                if bench.load_ms.is_none() {
                    bench.load_ms = Some(ms(now - bench.process_start));
                    window.on_next_frame(move |window, cx| bench_tick(editor, window, cx));
                } else if let Some(t) = bench.pending_paint.take() {
                    bench.paint_ms.push(ms(now - t));
                }
            }
        });
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .key_context("Editor")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .bg(rgb(0xffffff))
            .text_color(rgb(0x202020))
            .font_family("Menlo")
            .text_size(px(14.))
            .line_height(px(20.))
            .p(px(8.))
            .child(EditorElement {
                editor: cx.entity(),
            })
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// One benchmark step, run at the start of a frame: record the previous
/// insert's latency, then insert another "x" and schedule the next step.
fn bench_tick(editor: Entity<Editor>, window: &mut Window, cx: &mut App) {
    // Inserts go through the same `replace_text_in_range` path the OS text
    // input system uses for typed characters.
    let finished = editor.update(cx, |state, cx| {
        let now = Instant::now();
        let bench = state.bench.as_mut().unwrap();
        if let Some(t) = bench.pending_frame.take() {
            bench.frame_ms.push(ms(now - t));
        }
        if bench.frame_ms.len() >= BENCH_INSERTS {
            return true;
        }
        let first = !bench.started;
        bench.started = true;
        if first {
            let start = state.line_start(BENCH_LINE);
            state.selected_range = start..start;
        }
        let t = Instant::now();
        let bench = state.bench.as_mut().unwrap();
        bench.pending_frame = Some(t);
        bench.pending_paint = Some(t);
        state.replace_text_in_range(None, "x", window, cx);
        false
    });
    if finished {
        let bench = editor.read(cx).bench.as_ref().unwrap();
        let (p50, p95, max, mean) = stats(&bench.frame_ms);
        println!(
            "BENCH load_ms={:.2} p50_ms={p50:.2} p95_ms={p95:.2} max_ms={max:.2} mean_ms={mean:.2} n={}",
            bench.load_ms.unwrap(),
            bench.frame_ms.len()
        );
        let (p50, p95, max, mean) = stats(&bench.paint_ms);
        println!(
            "CPU insert_to_paint p50_ms={p50:.3} p95_ms={p95:.3} max_ms={max:.3} mean_ms={mean:.3} n={}",
            bench.paint_ms.len()
        );
        cx.quit();
    } else {
        window.on_next_frame(move |window, cx| bench_tick(editor, window, cx));
    }
}

fn stats(samples: &[f64]) -> (f64, f64, f64, f64) {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    (pct(0.5), pct(0.95), *sorted.last().unwrap(), mean)
}

fn main() {
    let process_start = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bench = args.iter().any(|a| a == "--bench");
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "../fixtures/10k.md".into());
    let text = std::fs::read_to_string(&path).expect("read file");

    application().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("enter", Enter, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("up", Up, None),
            KeyBinding::new("down", Down, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("shift-up", SelectUp, None),
            KeyBinding::new("shift-down", SelectDown, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|cx| Editor {
                        focus_handle: cx.focus_handle(),
                        rope: Rope::from_str(&text),
                        selected_range: 0..0,
                        selection_reversed: false,
                        marked_range: None,
                        scroll_y: px(0.),
                        autoscroll: false,
                        last_bounds: None,
                        last_line_height: px(20.),
                        last_lines: Vec::new(),
                        is_selecting: false,
                        bench: bench.then(|| Bench {
                            process_start,
                            load_ms: None,
                            started: false,
                            pending_frame: None,
                            pending_paint: None,
                            frame_ms: Vec::new(),
                            paint_ms: Vec::new(),
                        }),
                    })
                },
            )
            .unwrap();
        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.activate(true);
            })
            .unwrap();
    });
}
