//! The find bar (#122): finding text in the document as it's shown, going
//! from match to match, replacing matches (#123), and regular expressions
//! (#124).

use std::rc::Rc;

use gpui::{
    AnyElement, Context, Entity, Focusable, Hsla, KeyBinding, ListOffset, Role, SharedString,
    Subscription, Toggled, Window, div, prelude::*, px,
};
use kertas_core::blocks::BlockKind;
use kertas_core::editor::Selection;
use kertas_core::find::{self, FindOptions, Match, Query};
use kertas_core::model::Model;
use kertas_core::rendered::AlertKind;

use super::heights::Heights;
use super::render::bar_button;
use super::{
    DocumentView, Find, FindAndReplace, FindNext, FindPrevious, HideFind, Replace, ReplaceAll,
    ScrollRequest, UseSelectionForFind,
};
use crate::text_field::{self, TextField};

/// The find bar's key context.
const KEY_CONTEXT: &str = "FindBar";

/// The replacement field's key context, inside the bar's.
const REPLACE_CONTEXT: &str = "ReplaceField";

/// The keys the find bar handles while it has the focus. They're part of
/// [`crate::key_bindings`].
pub(super) fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("escape", HideFind, context),
        KeyBinding::new("enter", FindNext, context),
        KeyBinding::new("shift-enter", FindPrevious, context),
        KeyBinding::new("enter", Replace, Some(REPLACE_CONTEXT)),
    ]
}

/// What the find bar is looking for, and what it found.
#[derive(Default)]
pub(super) struct FindState {
    /// The query's field, made when the bar first opens.
    field: Option<Entity<TextField>>,
    /// Whether the bar is open. The query stays when it closes, so ⌘G
    /// still finds it.
    shown: bool,
    options: FindOptions,
    /// The matches, in order, in the text `found_in` holds.
    matches: Rc<Vec<Match>>,
    found_in: Option<Rc<Model>>,
    /// The match the bar is at.
    current: Option<usize>,
    /// The query the matches were found with, if it's valid.
    query: Option<Query>,
    /// What's wrong with the query, if it's a regular expression that
    /// isn't valid (#124).
    error: Option<String>,
    /// The replacement's field, made when it's first shown (#123).
    replace_field: Option<Entity<TextField>>,
    /// Whether the bar shows the replacement too.
    replacing: bool,
    /// How many matches Replace All replaced, until the bar looks again.
    replaced: Option<usize>,
    _field_changed: Option<Subscription>,
}

impl FindState {
    fn query_text(&self, cx: &gpui::App) -> String {
        self.field
            .as_ref()
            .map(|field| field.read(cx).text().to_owned())
            .unwrap_or_default()
    }
}

impl DocumentView {
    /// ⌘F: opens the find bar, with its query selected to type over.
    pub(super) fn show_find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.is_none() {
            return;
        }
        let field = self.find_field(cx);
        let reopened = !self.find.shown;
        self.find.shown = true;
        self.find.replacing = false;
        field.update(cx, |field, cx| field.select_all_text(cx));
        field.read(cx).focus_handle(cx).focus(window, cx);
        if reopened {
            self.find_from_caret(cx);
        }
        cx.notify();
    }

    /// ⌥⌘F: opens the find bar with the replacement shown, to type the
    /// query or, if there is one, the replacement.
    pub(super) fn show_replace(
        &mut self,
        _: &FindAndReplace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.open else {
            return;
        };
        // A read-only document can't be changed.
        if open.editor.read_only() {
            return self.show_find(&Find, window, cx);
        }
        let field = self.find_field(cx);
        if self.find.replace_field.is_none() {
            let replace = cx.new(|cx| TextField::new("Replace", "", "Replace with", cx));
            self.find.replace_field = Some(replace);
        }
        let reopened = !self.find.shown;
        self.find.shown = true;
        self.find.replacing = true;
        let focused = match &self.find.replace_field {
            Some(replace) if !field.read(cx).text().is_empty() => replace.clone(),
            _ => field,
        };
        focused.update(cx, |field, cx| field.select_all_text(cx));
        focused.read(cx).focus_handle(cx).focus(window, cx);
        if reopened {
            self.find_from_caret(cx);
        }
        cx.notify();
    }

    /// Every match, with the text the replacement replaces it with: as
    /// typed, or with a regular expression's groups filled in.
    fn replacements(&self, cx: &gpui::App) -> Vec<(Match, String)> {
        let (Some(query), Some(model)) = (&self.find.query, &self.find.found_in) else {
            return Vec::new();
        };
        let template = self
            .find
            .replace_field
            .as_ref()
            .map(|field| field.read(cx).text().to_owned())
            .unwrap_or_default();
        find::replacements(model, query, &template)
    }

    /// Replace, or ↩ in the replacement: replaces the match the bar is at,
    /// if it's selected, and goes to the next. If it isn't selected, just
    /// goes to it first, so nothing changes out of sight.
    pub(super) fn replace(&mut self, _: &Replace, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh_matches(cx);
        let Some(open) = &mut self.open else {
            return;
        };
        let selection = open.editor.selection().map(|selection| selection.range());
        let Some(found) = self
            .current_match()
            .filter(|found| Some(&found.range) == selection.as_ref())
        else {
            match self.current_match() {
                Some(found) => self.select_match(&found, cx),
                None => self.step_match(true, cx),
            }
            return cx.notify();
        };
        let Some((_, replacement)) = self
            .replacements(cx)
            .into_iter()
            .find(|(each, _)| each.range == found.range)
        else {
            return;
        };
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(change) = open.editor.replace_ranges(found.replacement(&replacement)) else {
            return;
        };
        open.goal_x = None;
        self.apply(change, cx);
        // The next match after the new text, not in it.
        self.search(cx);
        let after = found.range.start + replacement.len();
        self.find.current = find::first_from(&self.find.matches, after);
        if let Some(next) = self.current_match() {
            self.select_match(&next, cx);
        }
        cx.notify();
    }

    /// Replace All: replaces every match, as one undo step.
    pub(super) fn replace_all(&mut self, _: &ReplaceAll, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh_matches(cx);
        let replacements = self.replacements(cx);
        let count = replacements.len();
        let edits: Vec<_> = replacements
            .iter()
            .flat_map(|(found, replacement)| found.replacement(replacement))
            .collect();
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(change) = open.editor.replace_ranges(edits) else {
            return;
        };
        open.goal_x = None;
        self.apply(change, cx);
        self.search(cx);
        self.find.replaced = Some(count);
        cx.notify();
    }

    /// Forgets the matches, which were in the document another replaced.
    pub(super) fn forget_matches(&mut self) {
        self.find.matches = Rc::default();
        self.find.found_in = None;
        self.find.current = None;
    }

    /// The query's field, made the first time it's needed.
    fn find_field(&mut self, cx: &mut Context<Self>) -> Entity<TextField> {
        if let Some(field) = &self.find.field {
            return field.clone();
        }
        let field = cx.new(|cx| TextField::new("Find", "", "Find", cx));
        self.find._field_changed = Some(
            cx.subscribe(&field, |this, _, _: &text_field::Changed, cx| {
                this.find_from_caret(cx)
            }),
        );
        self.find.field = Some(field.clone());
        field
    }

    /// Esc in the bar: closes it, leaving the match it was at selected.
    pub(super) fn hide_find(&mut self, _: &HideFind, window: &mut Window, cx: &mut Context<Self>) {
        self.find.shown = false;
        self.find.replacing = false;
        if let Some(found) = self.current_match() {
            self.select_match(&found, cx);
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// ⌘E: looks for the selected text, opening the bar to show it.
    pub(super) fn use_selection_for_find(
        &mut self,
        _: &UseSelectionForFind,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.open else {
            return;
        };
        let Some(text) = open.editor.selected_text().map(str::to_owned) else {
            return;
        };
        // The field holds one line.
        let text = text.lines().next().unwrap_or_default().to_owned();
        let field = self.find_field(cx);
        field.update(cx, |field, cx| field.set_text(text, cx));
        self.find.shown = true;
        self.find_from_caret(cx);
    }

    /// ⌘G or ↩ in the bar: goes to the next match, wrapping around to the
    /// first.
    pub(super) fn find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_match(true, cx);
    }

    /// ⇧⌘G or ⇧↩ in the bar: goes to the previous match, wrapping around
    /// to the last.
    pub(super) fn find_previous(
        &mut self,
        _: &FindPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_match(false, cx);
    }

    /// Looks for the query again, after it or its options change, and
    /// scrolls to the first match after the caret, or after the top of the
    /// window with no caret.
    fn find_from_caret(&mut self, cx: &mut Context<Self>) {
        self.search(cx);
        let Some(open) = &self.open else {
            return;
        };
        let from = match open.editor.selection() {
            Some(selection) => selection.range().start,
            None => {
                let model = open.editor.model();
                let top = open.list_state.logical_scroll_top().item_ix;
                model.items.get(top).map_or(0, |item| item.range.start)
            }
        };
        self.find.current = find::first_from(&self.find.matches, from);
        if let Some(found) = self.current_match() {
            self.scroll_to_match(&found);
        }
        cx.notify();
    }

    /// Finds the matches for the query in the text as it is now.
    fn search(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let model = open.editor.model().clone();
        let query = Query::new(&self.find.query_text(cx), self.find.options);
        (self.find.query, self.find.error) = match query {
            Ok(query) => (query, None),
            Err(error) => (None, Some(error)),
        };
        self.find.matches = Rc::new(match &self.find.query {
            Some(query) => find::find(&model, query),
            None => Vec::new(),
        });
        self.find.found_in = Some(model);
        self.find.current = None;
        self.find.replaced = None;
    }

    /// Finds the matches again if the text changed since they were found,
    /// staying at the match where the current one was.
    pub(super) fn refresh_matches(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let current = match &self.find.found_in {
            Some(found_in) => Rc::ptr_eq(found_in, open.editor.model()),
            // Another document was opened, or nothing was looked for yet.
            None => !self.find.shown,
        };
        if current {
            return;
        }
        let at = self.current_match().map(|found| found.range.start);
        self.search(cx);
        self.find.current = at.and_then(|at| find::first_from(&self.find.matches, at));
    }

    fn current_match(&self) -> Option<Match> {
        self.find
            .current
            .and_then(|index| self.find.matches.get(index))
            .cloned()
    }

    /// Goes to the next or previous match: from the current one if it's
    /// selected, or else from the selection.
    fn step_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.find.found_in.is_none() {
            self.search(cx);
        }
        self.refresh_matches(cx);
        let count = self.find.matches.len();
        let Some(open) = &self.open else {
            return;
        };
        if count == 0 {
            return;
        }
        let selection = open.editor.selection().map(|selection| selection.range());
        let current = self.current_match().map(|found| found.range);
        let at_current = selection.is_none() || selection == current;
        let index = match (self.find.current, selection) {
            (Some(index), _) if at_current => {
                if forward {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                }
            }
            (_, Some(selection)) if forward => {
                find::first_from(&self.find.matches, selection.end).unwrap_or(0)
            }
            (_, Some(selection)) => {
                let before = self
                    .find
                    .matches
                    .partition_point(|found| found.range.start < selection.start);
                before.checked_sub(1).unwrap_or(count - 1)
            }
            _ if forward => 0,
            _ => count - 1,
        };
        self.find.current = Some(index);
        let found = self.find.matches[index].clone();
        self.select_match(&found, cx);
        cx.notify();
    }

    /// Selects `found`, revealing its block as a click there would, but
    /// leaving the focus where it is. A read-only document just scrolls to
    /// it.
    fn select_match(&mut self, found: &Match, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if open.editor.read_only() {
            self.scroll_to_match(found);
            return;
        }
        open.goal_x = None;
        open.editor.unmark();
        open.editor.break_group();
        self.set_selection(
            Selection {
                anchor: found.range.start,
                head: found.range.end,
            },
            cx,
        );
    }

    /// Scrolls `found` into view without selecting it.
    fn scroll_to_match(&mut self, found: &Match) {
        let Some(open) = &mut self.open else {
            return;
        };
        let model = open.editor.model();
        let index = model.item_at(found.range.start);
        // A footnote is drawn in the rows after the items (#90).
        let row = match model.items[index].kind {
            BlockKind::FootnoteDefinition => model
                .notes
                .iter()
                .position(|note| note.item == index)
                .map_or(index, |note| model.items.len() + note),
            _ => index,
        };
        // An item off screen isn't laid out, so it couldn't ask to be
        // scrolled to.
        if Heights::measured(&open.list_state).on_screen(row).is_none() {
            open.list_state.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        }
        open.scroll_request = Some(ScrollRequest::Match(found.range.start));
    }

    /// The matches to draw, as source ranges in order with their
    /// backgrounds: none while the bar is closed.
    pub(super) fn find_highlights(&self) -> Vec<(std::ops::Range<usize>, Hsla)> {
        if !self.find.shown {
            return Vec::new();
        }
        let theme = self.theme;
        self.find
            .matches
            .iter()
            .enumerate()
            .flat_map(|(index, found)| {
                let color = if Some(index) == self.find.current {
                    theme.find_current
                } else {
                    theme.find_match
                };
                found.pieces.iter().map(move |piece| (piece.clone(), color))
            })
            .collect()
    }

    /// Turns a find option on or off, and looks again.
    fn toggle_option(&mut self, toggle: fn(&mut FindOptions), cx: &mut Context<Self>) {
        toggle(&mut self.find.options);
        self.find_from_caret(cx);
    }

    /// The find bar, if it's open.
    pub(super) fn render_find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.find.shown || self.open.is_none() {
            return None;
        }
        let field = self.find.field.clone()?;
        let theme = self.theme;
        field.update(cx, |field, cx| field.set_theme(theme, cx));
        let count = self.find.matches.len();
        let status: SharedString = match (self.find.current, count) {
            _ if let Some(error) = &self.find.error => error.clone().into(),
            _ if let Some(replaced) = self.find.replaced => format!("Replaced {replaced}").into(),
            _ if field.read(cx).text().is_empty() => "".into(),
            (_, 0) => "Not found".into(),
            (Some(index), count) => format!("{} of {count}", index + 1).into(),
            (None, 1) => "1 match".into(),
            (None, count) => format!("{count} matches").into(),
        };
        let options = self.find.options;
        let toggle = |id: &'static str, label: &'static str, on: bool| {
            bar_button(theme, id, label, label)
                .role(Role::CheckBox)
                .aria_toggled(if on { Toggled::True } else { Toggled::False })
                .when(on, |button| button.bg(theme.selection))
        };
        let find_row = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().max_w(px(320.)).min_w(px(120.)).child(field))
            .child(
                div()
                    .id("find-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .min_w(px(64.))
                    .text_color(match self.find.error {
                        Some(_) => theme.alert(AlertKind::Caution),
                        None => theme.muted_text,
                    })
                    .child(status),
            )
            .child(
                bar_button(theme, "find-previous", "‹", "Previous Match")
                    .on_click(cx.listener(|this, _, _, cx| this.step_match(false, cx))),
            )
            .child(
                bar_button(theme, "find-next", "›", "Next Match")
                    .on_click(cx.listener(|this, _, _, cx| this.step_match(true, cx))),
            )
            .child(
                toggle("find-match-case", "Match Case", options.match_case).on_click(cx.listener(
                    |this, _, _, cx| this.toggle_option(|options| options.match_case ^= true, cx),
                )),
            )
            .child(
                toggle("find-whole-word", "Whole Words", options.whole_word).on_click(cx.listener(
                    |this, _, _, cx| this.toggle_option(|options| options.whole_word ^= true, cx),
                )),
            )
            .child(
                toggle("find-regex", "Regex", options.regex).on_click(cx.listener(
                    |this, _, _, cx| this.toggle_option(|options| options.regex ^= true, cx),
                )),
            )
            .child(div().flex_1())
            .child(bar_button(theme, "find-done", "Done", "Done").on_click(
                cx.listener(|this, _, window, cx| this.hide_find(&HideFind, window, cx)),
            ));
        let replace_row =
            self.find
                .replace_field
                .clone()
                .filter(|_| self.find.replacing)
                .map(|replace| {
                    replace.update(cx, |field, cx| field.set_theme(theme, cx));
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .key_context(REPLACE_CONTEXT)
                                .flex_1()
                                .max_w(px(320.))
                                .min_w(px(120.))
                                .child(replace),
                        )
                        .child(bar_button(theme, "replace", "Replace", "Replace").on_click(
                            cx.listener(|this, _, window, cx| this.replace(&Replace, window, cx)),
                        ))
                        .child(
                            bar_button(theme, "replace-all", "Replace All", "Replace All")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.replace_all(&ReplaceAll, window, cx)
                                })),
                        )
                });
        Some(
            div()
                .id("find-bar")
                .key_context(KEY_CONTEXT)
                .role(Role::Group)
                .aria_label(if replace_row.is_some() {
                    "Find and Replace"
                } else {
                    "Find"
                })
                .flex()
                .flex_col()
                .gap(px(6.))
                .px_3()
                .py(px(6.))
                .border_b_1()
                .border_color(theme.rule)
                .bg(theme.table_header_background)
                .text_color(theme.text)
                .text_size(px(12.))
                .line_height(px(16.))
                .child(find_row)
                .children(replace_row)
                .into_any_element(),
        )
    }

    /// Whether the find bar is open.
    pub fn find_shown(&self) -> bool {
        self.find.shown
    }

    /// The find bar's status: the number of the match it's at, if any, and
    /// how many there are, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn find_status(&self) -> (Option<usize>, usize) {
        (self.find.current, self.find.matches.len())
    }

    /// Turns the Regex option on or off, as its button does, for the smoke
    /// test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn set_find_regex(&mut self, on: bool, cx: &mut Context<Self>) {
        self.find.options.regex = on;
        self.find_from_caret(cx);
    }

    /// What's wrong with the find bar's query, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn find_error(&self) -> Option<String> {
        self.find.error.clone()
    }

    /// Types `text` into the find bar's replacement, as the smoke test does.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn type_replacement(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(field) = &self.find.replace_field {
            field.update(cx, |field, cx| field.set_text(text, cx));
        }
    }

    /// Types `text` into the find bar's query, as the smoke test does.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn type_find_query(&mut self, text: &str, cx: &mut Context<Self>) {
        let field = self.find_field(cx);
        field.update(cx, |field, cx| {
            field.set_text(text, cx);
            cx.emit(text_field::Changed);
        });
    }
}
