//! Tabbed documents: several document views in one window, one shown at a
//! time, with a strip of tabs above it while there are two or more.
//!
//! An app puts a [`DocumentTabs`] in its window, adds document views to it
//! with [`DocumentTabs::add`], and binds its keys with [`key_bindings`]:
//! the next and previous tab, ⌘1 to ⌘9, and ⌘W to close a tab. Tabs can
//! be reordered by dragging them along the strip.
//!
//! It tells the app what happens with [`TabsEvent`]s, passing on its
//! documents' [`DocumentEvent`]s. Closing the last tab leaves it empty;
//! the app decides what that means, such as closing the window. To close
//! the window, the app asks [`DocumentTabs::ready_to_close`] and, if that
//! says no, [`DocumentTabs::confirm_close`], which asks about each tab
//! with unsaved changes in turn.

use gpui::{
    AccessibleAction, AnyElement, App, Context, Entity, EventEmitter, Focusable, KeyBinding,
    MouseButton, Render, Role, ScrollHandle, SharedString, Subscription, Task, Window, actions,
    div, prelude::*, px,
};

use crate::{DocumentEvent, DocumentView, Theme};
use kertas_core::titles;

const STRIP_HEIGHT: f32 = 30.;
const MIN_TAB_WIDTH: f32 = 100.;
const MAX_TAB_WIDTH: f32 = 240.;

actions!(
    kertas,
    [
        NextTab,
        PreviousTab,
        CloseTab,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectTab9,
    ]
);

/// Key bindings for tabs (macOS conventions).
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-shift-]", NextTab, None),
        KeyBinding::new("cmd-shift-[", PreviousTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-1", SelectTab1, None),
        KeyBinding::new("cmd-2", SelectTab2, None),
        KeyBinding::new("cmd-3", SelectTab3, None),
        KeyBinding::new("cmd-4", SelectTab4, None),
        KeyBinding::new("cmd-5", SelectTab5, None),
        KeyBinding::new("cmd-6", SelectTab6, None),
        KeyBinding::new("cmd-7", SelectTab7, None),
        KeyBinding::new("cmd-8", SelectTab8, None),
        KeyBinding::new("cmd-9", SelectTab9, None),
    ]
}

/// What [`DocumentTabs`] tells the app.
#[derive(Debug, Clone)]
pub enum TabsEvent {
    /// Another tab is now the active one, or a tab was added (and made
    /// active).
    ActiveChanged,
    /// A tab's document sent this event.
    Document(Entity<DocumentView>, DocumentEvent),
    /// A tab was closed. If it was the last one, the tabs are now empty.
    Closed(Entity<DocumentView>),
    /// The tabs were reordered by dragging.
    Reordered,
}

struct Tab {
    view: Entity<DocumentView>,
    /// Shown after the document's name, to tell it from other open
    /// documents with the same name.
    title_hint: Option<String>,
    _subscription: Subscription,
}

pub struct DocumentTabs {
    tabs: Vec<Tab>,
    active: usize,
    /// Whether a close is waiting for its "save changes?" answer, so
    /// another isn't started.
    confirming: bool,
    scroll_handle: ScrollHandle,
    /// Whether the strip is hidden, whatever the number of tabs.
    strip_hidden: bool,
}

impl EventEmitter<TabsEvent> for DocumentTabs {}

impl Default for DocumentTabs {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentTabs {
    /// No tabs yet.
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            confirming: false,
            scroll_handle: ScrollHandle::new(),
            strip_hidden: false,
        }
    }

    /// Hides the strip of tabs, or shows it again, as when a host app
    /// hides everything but the document. The tabs' keys still work.
    pub fn set_strip_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden != self.strip_hidden {
            self.strip_hidden = hidden;
            cx.notify();
        }
    }

    /// Adds `view` as the last tab, and makes it the active one.
    pub fn add(&mut self, view: Entity<DocumentView>, window: &mut Window, cx: &mut Context<Self>) {
        let subscription = cx.subscribe_in(&view, window, |_, view, event, _, cx| {
            // The strip shows titles and unsaved changes.
            cx.notify();
            cx.emit(TabsEvent::Document(view.clone(), event.clone()));
        });
        self.tabs.push(Tab {
            view,
            title_hint: None,
            _subscription: subscription,
        });
        let index = self.tabs.len() - 1;
        if index == 0 {
            self.show_active(window, cx);
        } else {
            self.activate(index, window, cx);
        }
    }

    /// The documents, in tab order.
    pub fn views(&self) -> impl Iterator<Item = &Entity<DocumentView>> {
        self.tabs.iter().map(|tab| &tab.view)
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// The position of the active tab. It's 0 when there are no tabs.
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// The active tab's document, unless there are no tabs.
    pub fn active_view(&self) -> Option<&Entity<DocumentView>> {
        self.tabs.get(self.active).map(|tab| &tab.view)
    }

    /// The position of `view`'s tab, if it's in one.
    pub fn position(&self, view: &Entity<DocumentView>) -> Option<usize> {
        self.tabs.iter().position(|tab| &tab.view == view)
    }

    /// Sets the text shown after `view`'s name in its tab, to tell it from
    /// other open documents with the same name (see
    /// [`kertas_core::titles`]).
    pub fn set_title_hint(
        &mut self,
        view: &Entity<DocumentView>,
        hint: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| &tab.view == view)
            && tab.title_hint != hint
        {
            tab.title_hint = hint;
            cx.notify();
        }
    }

    /// The title of the tab at `index`: its document's name, and its title
    /// hint if it has one.
    pub fn title(&self, index: usize, cx: &App) -> Option<String> {
        let tab = self.tabs.get(index)?;
        let name = tab.view.read(cx).display_name()?;
        Some(titles::title(&name, tab.title_hint.as_deref()))
    }

    /// Makes the tab at `index` the active one, and focuses its document.
    /// Does nothing if there's no such tab.
    pub fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        if index == self.active {
            tab.view.focus_handle(cx).focus(window, cx);
            return;
        }
        if let Some(previous) = self.tabs.get(self.active) {
            // Leaving a tab autosaves it, as leaving a window does.
            previous.view.update(cx, |view, cx| view.hidden(window, cx));
        }
        self.active = index;
        self.show_active(window, cx);
    }

    /// Focuses the active tab's document, scrolls the strip to its tab, and
    /// reports it.
    fn show_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active) {
            tab.view.focus_handle(cx).focus(window, cx);
            self.scroll_handle.scroll_to_item(self.active);
        }
        cx.emit(TabsEvent::ActiveChanged);
        cx.notify();
    }

    /// Moves the tab at `from` to `to`, keeping the same tab active.
    pub fn move_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let active = self.tabs[self.active].view.clone();
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = self.position(&active).unwrap_or(0);
        self.scroll_handle.scroll_to_item(self.active);
        cx.emit(TabsEvent::Reordered);
        cx.notify();
    }

    /// Closes the tab at `index`, asking about unsaved changes first.
    pub fn close(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.tabs.get(index).map(|tab| tab.view.clone()) else {
            return;
        };
        if self.confirming {
            return;
        }
        if view.update(cx, |view, cx| view.ready_to_close(window, cx)) {
            self.remove(&view, window, cx);
            return;
        }
        // Ask on the tab being closed.
        self.activate(index, window, cx);
        self.confirming = true;
        let confirmed = view.update(cx, |view, cx| view.confirm_close(window, cx));
        cx.spawn_in(window, async move |this, cx| {
            let confirmed = confirmed.await;
            this.update_in(cx, |this, window, cx| {
                this.confirming = false;
                if confirmed {
                    this.remove(&view, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Removes `view`'s tab without asking. The tab to its right becomes
    /// the active one, or the one to its left if it was the last.
    fn remove(&mut self, view: &Entity<DocumentView>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.position(view) else {
            return;
        };
        let active = self.tabs.get(self.active).map(|tab| tab.view.clone());
        self.tabs.remove(index);
        cx.emit(TabsEvent::Closed(view.clone()));
        if self.tabs.is_empty() {
            self.active = 0;
        } else if active.as_ref() == Some(view) {
            self.active = index.min(self.tabs.len() - 1);
            self.show_active(window, cx);
        } else {
            self.active = active
                .and_then(|active| self.position(&active))
                .unwrap_or(0);
        }
        cx.notify();
    }

    /// Whether every tab can close straight away: none has unsaved
    /// changes, or autosave just saved them. If not, ask with
    /// [`DocumentTabs::confirm_close`].
    pub fn ready_to_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // Every tab gets the chance to autosave, so no `all`.
        self.tabs.iter().fold(true, |ready, tab| {
            let tab_ready = tab
                .view
                .update(cx, |view, cx| view.ready_to_close(window, cx));
            ready && tab_ready
        })
    }

    /// Asks about each tab with unsaved changes in turn, switching to it
    /// first. Resolves to whether they may all close: each one was saved,
    /// or the user chose not to save it. Stops at the first Cancel. The
    /// tabs stay open; closing them, with the window, is up to the app.
    pub fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        if self.confirming {
            return Task::ready(false);
        }
        self.confirming = true;
        cx.spawn_in(window, async move |this, cx| {
            let mut asked = Vec::new();
            let all = loop {
                // The next tab, in order, that can't close yet.
                let next = this.update_in(cx, |this, window, cx| {
                    let index = (0..this.tabs.len()).find(|&index| {
                        let view = this.tabs[index].view.clone();
                        !asked.contains(&view)
                            && !view.update(cx, |view, cx| view.ready_to_close(window, cx))
                    })?;
                    let view = this.tabs[index].view.clone();
                    this.activate(index, window, cx);
                    asked.push(view.clone());
                    Some(view.update(cx, |view, cx| view.confirm_close(window, cx)))
                });
                match next {
                    Ok(Some(confirmed)) => {
                        if !confirmed.await {
                            break false;
                        }
                    }
                    Ok(None) => break true,
                    Err(_) => break false,
                }
            };
            this.update(cx, |this, _| this.confirming = false).ok();
            all
        })
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.activate(index, window, cx);
    }

    fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            self.activate((self.active + 1) % self.tabs.len(), window, cx);
        }
    }

    fn previous_tab(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            let count = self.tabs.len();
            self.activate((self.active + count - 1) % count, window, cx);
        }
    }

    fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.close(self.active, window, cx);
    }

    fn render_strip(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let count = self.tabs.len();
        let strip = cx.entity().entity_id();
        let this = cx.weak_entity();
        let tabs = self.tabs.iter().enumerate().map(|(index, tab)| {
            let active = index == self.active;
            let dirty = tab.view.read(cx).is_dirty();
            let title: SharedString = self.title(index, cx).unwrap_or_default().into();
            let close_name = if dirty {
                format!("Close {title}, which has unsaved changes")
            } else {
                format!("Close {title}")
            };
            let dragged = DraggedTab {
                strip,
                index,
                title: title.clone(),
                theme,
            };
            div()
                .id(("tab", index))
                .role(Role::Tab)
                .aria_label(title.clone())
                .aria_selected(active)
                .aria_position_in_set(index + 1)
                .aria_size_of_set(count)
                .when(dirty, |this| this.aria_description("Edited"))
                .on_a11y_action(AccessibleAction::Click, {
                    let this = this.clone();
                    move |_, window, cx| {
                        this.update(cx, |this, cx| this.select(index, window, cx))
                            .ok();
                    }
                })
                // As wide as they can be, shrinking as tabs are added,
                // then the strip scrolls.
                .flex_shrink(1.)
                .flex_basis(px(MAX_TAB_WIDTH))
                .min_w(px(MIN_TAB_WIDTH))
                .max_w(px(MAX_TAB_WIDTH))
                .h_full()
                .px_3()
                .flex()
                .items_center()
                .gap_2()
                .border_r_1()
                .border_color(theme.rule)
                .text_size(px(12.))
                .line_height(px(STRIP_HEIGHT))
                .when(active, |this| {
                    this.bg(theme.background).text_color(theme.text)
                })
                .when(!active, |this| {
                    this.text_color(theme.muted_text)
                        .hover(|style| style.text_color(theme.text))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if index != this.active {
                            this.select(index, window, cx);
                        }
                    }),
                )
                .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                .can_drop(move |dragged, _, _| {
                    dragged
                        .downcast_ref::<DraggedTab>()
                        .is_some_and(|dragged| dragged.strip == strip)
                })
                .drag_over::<DraggedTab>(move |style, _, _, _| {
                    style.border_l_2().border_color(theme.text)
                })
                .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| {
                    this.move_tab(dragged.index, index, cx);
                }))
                .child(div().flex_1().min_w(px(0.)).truncate().child(title))
                .child(
                    div()
                        .id(("close-tab", index))
                        .role(Role::Button)
                        .aria_label(SharedString::from(close_name))
                        .on_a11y_action(AccessibleAction::Click, {
                            let this = this.clone();
                            move |_, window, cx| {
                                this.update(cx, |this, cx| this.close(index, window, cx))
                                    .ok();
                            }
                        })
                        .flex_none()
                        .w(px(14.))
                        .text_center()
                        .rounded_sm()
                        .hover(|style| style.bg(theme.rule))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.close(index, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(if dirty { "●" } else { "×" }),
                )
        });

        div()
            .id("tabs")
            .role(Role::TabList)
            .aria_label("Documents")
            .flex_none()
            .flex()
            .h(px(STRIP_HEIGHT))
            .w_full()
            .overflow_x_scroll()
            .track_scroll(&self.scroll_handle)
            .bg(theme.table_header_background)
            .border_b_1()
            .border_color(theme.rule)
            .children(tabs)
            // Dropping past the last tab moves the tab to the end.
            .child(
                div()
                    .id("tabs-end")
                    .flex_1()
                    .min_w(px(24.))
                    .h_full()
                    .can_drop(move |dragged, _, _| {
                        dragged
                            .downcast_ref::<DraggedTab>()
                            .is_some_and(|dragged| dragged.strip == strip)
                    })
                    .drag_over::<DraggedTab>(move |style, _, _, _| {
                        style.border_l_2().border_color(theme.text)
                    })
                    .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| {
                        let last = this.tabs.len().saturating_sub(1);
                        this.move_tab(dragged.index, last, cx);
                    })),
            )
            .into_any_element()
    }
}

impl Focusable for DocumentTabs {
    /// The active document's focus.
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        match self.active_view() {
            Some(view) => view.focus_handle(cx),
            None => cx.focus_handle(),
        }
    }
}

impl Render for DocumentTabs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_view().cloned();
        let strip = match &active {
            Some(view) if self.tabs.len() > 1 && !self.strip_hidden => {
                let theme = view.read(cx).theme();
                Some(self.render_strip(theme, cx))
            }
            _ => None,
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(|this, _: &SelectTab1, window, cx| this.select(0, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab2, window, cx| this.select(1, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab3, window, cx| this.select(2, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab4, window, cx| this.select(3, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab5, window, cx| this.select(4, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab6, window, cx| this.select(5, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab7, window, cx| this.select(6, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab8, window, cx| this.select(7, window, cx)))
            .on_action(cx.listener(|this, _: &SelectTab9, window, cx| this.select(8, window, cx)))
            .children(strip)
            .child(div().flex_1().min_h(px(0.)).children(active))
    }
}

/// A tab being dragged along the strip, and the image that follows the
/// mouse.
#[derive(Clone)]
struct DraggedTab {
    /// The strip it came from, so it can't be dropped on another.
    strip: gpui::EntityId,
    index: usize,
    title: SharedString,
    theme: Theme,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(STRIP_HEIGHT))
            .max_w(px(MAX_TAB_WIDTH))
            .px_3()
            .bg(self.theme.background)
            .text_color(self.theme.text)
            .border_1()
            .border_color(self.theme.rule)
            .text_size(px(12.))
            .line_height(px(STRIP_HEIGHT))
            .truncate()
            .child(self.title.clone())
    }
}
