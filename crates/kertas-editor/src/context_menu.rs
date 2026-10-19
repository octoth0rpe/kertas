//! A context menu, drawn in GPUI like the pop-up lists in the settings
//! window, opened by a secondary click: a two-finger click, a Control-click
//! or the right mouse button ([`on_secondary_click`]). The document view
//! shows one for a misspelled word (#142), and a host can use it for its
//! own.
//!
//! It opens at the pointer and takes the keyboard. ↑ and ↓ move between its
//! items, ↩ or Space chooses one, and Esc or a click outside closes it,
//! giving the keyboard back. Its owner keeps it while it's open, renders it
//! as a child, and lets go of it when it's [`Dismissed`]. VoiceOver reads
//! it as a menu of items, each of which it can press. Its keys are among
//! [`crate::key_bindings`].

use std::rc::Rc;

use gpui::{
    self, AccessibleAction, App, Context, Div, ElementId, EventEmitter, FocusHandle, Focusable,
    KeyBinding, MouseButton, MouseDownEvent, Pixels, Point, Role, SharedString, Stateful, Window,
    actions, anchored, deferred, div, prelude::*, px,
};

use crate::Theme;

actions!(
    context_menu,
    [
        SelectNext,
        SelectPrevious,
        SelectFirst,
        SelectLast,
        Confirm,
        Cancel
    ]
);

const KEY_CONTEXT: &str = "ContextMenu";

/// The keys an open menu handles, as a native menu does.
pub fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("home", SelectFirst, context),
        KeyBinding::new("end", SelectLast, context),
        KeyBinding::new("enter", Confirm, context),
        KeyBinding::new("space", Confirm, context),
        KeyBinding::new("escape", Cancel, context),
    ]
}

/// The menu closed, with an item chosen or not.
pub struct Dismissed;

/// What an item does when it's chosen.
type Run = Rc<dyn Fn(&mut Window, &mut App)>;

struct Item {
    label: SharedString,
    run: Run,
    /// Whether a rule above it starts a new group of items.
    rule_above: bool,
}

pub struct ContextMenu {
    /// What VoiceOver calls the menu.
    name: SharedString,
    items: Vec<Item>,
    /// The highlighted item, by ↑ and ↓ or the pointer.
    selected: Option<usize>,
    /// Whether the pointer highlighted it, so the highlight goes when the
    /// pointer leaves the menu. One from the keyboard stays.
    selected_by_pointer: bool,
    /// Where it opens, in the window.
    position: Point<Pixels>,
    theme: Theme,
    /// Whether the next item starts a new group.
    rule_next: bool,
    focus_handle: FocusHandle,
    /// What had the keyboard before it opened, which gets it back.
    restore_focus: Option<FocusHandle>,
}

impl EventEmitter<Dismissed> for ContextMenu {}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ContextMenu {
    /// An empty menu called `name`, at `position` in the window, drawn
    /// in `theme`, which takes the keyboard.
    pub fn new(
        name: impl Into<SharedString>,
        position: Point<Pixels>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let restore_focus = window.focused(cx);
        focus_handle.focus(window, cx);
        Self {
            name: name.into(),
            items: Vec::new(),
            selected: None,
            selected_by_pointer: false,
            position,
            theme,
            rule_next: false,
            focus_handle,
            restore_focus,
        }
    }

    /// The menu with an item called `label`, which does `run` once the
    /// menu has closed.
    pub fn item(
        mut self,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(Item {
            label: label.into(),
            run: Rc::new(run),
            rule_above: std::mem::take(&mut self.rule_next) && !self.items.is_empty(),
        });
        self
    }

    /// The menu with a rule after its items so far, before the next.
    pub fn separator(mut self) -> Self {
        self.rule_next = true;
        self
    }

    /// The items' labels, in order, for the smoke test.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn labels(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| item.label.to_string())
            .collect()
    }

    /// Closes the menu, giving the keyboard back to what had it.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_handle.contains_focused(window, cx)
            && let Some(focus) = self.restore_focus.take()
        {
            focus.focus(window, cx);
        }
        cx.emit(Dismissed);
    }

    /// Closes the menu and does the item at `index`.
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.items.get(index).map(|item| item.run.clone()) else {
            return;
        };
        self.dismiss(window, cx);
        // The item's action may update what owns the menu, which is being
        // updated now.
        window.defer(cx, move |window, cx| run(window, cx));
    }

    fn select(&mut self, step: Step, cx: &mut Context<Self>) {
        self.selected = moved(self.selected, self.items.len(), step);
        self.selected_by_pointer = false;
        cx.notify();
    }

    fn render_item(&self, index: usize, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let item = &self.items[index];
        let selected = self.selected == Some(index);
        div()
            .id(("menu-item", index))
            .role(Role::MenuItem)
            .aria_label(item.label.clone())
            .aria_position_in_set(index + 1)
            .aria_size_of_set(self.items.len())
            .aria_selected(selected)
            .when(item.rule_above, |this| {
                this.mt(px(9.)).relative().child(
                    div()
                        .absolute()
                        .top(px(-5.))
                        .left_0()
                        .right_0()
                        .h(px(1.))
                        .bg(theme.rule),
                )
            })
            .px_3()
            .py(px(2.))
            .mx_1()
            .rounded_sm()
            .when(selected, |this| this.bg(theme.selection))
            .on_a11y_action(AccessibleAction::Click, {
                let this = cx.weak_entity();
                move |_, window, cx| {
                    this.update(cx, |this, cx| this.choose(index, window, cx))
                        .ok();
                }
            })
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.selected != Some(index) {
                    this.selected = Some(index);
                    this.selected_by_pointer = true;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.choose(index, window, cx)))
            .child(item.label.clone())
    }
}

impl gpui::Render for ContextMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let items: Vec<_> = (0..self.items.len())
            .map(|index| self.render_item(index, theme, cx))
            .collect();
        deferred(
            anchored()
                .position(self.position)
                .snap_to_window_with_margin(px(8.))
                .child(
                    div()
                        .id("context-menu")
                        .track_focus(&self.focus_handle)
                        .key_context(KEY_CONTEXT)
                        .role(Role::Menu)
                        .aria_label(self.name.clone())
                        .occlude()
                        .min_w(px(180.))
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.rule)
                        .bg(theme.background)
                        .text_color(theme.text)
                        .shadow_md()
                        .on_action(
                            cx.listener(|this, _: &SelectNext, _, cx| this.select(Step::Next, cx)),
                        )
                        .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| {
                            this.select(Step::Previous, cx)
                        }))
                        .on_action(
                            cx.listener(|this, _: &SelectFirst, _, cx| {
                                this.select(Step::First, cx)
                            }),
                        )
                        .on_action(
                            cx.listener(|this, _: &SelectLast, _, cx| this.select(Step::Last, cx)),
                        )
                        .on_action(cx.listener(|this, _: &Confirm, window, cx| {
                            match this.selected {
                                Some(index) => this.choose(index, window, cx),
                                // As in a native menu, ↩ with nothing
                                // highlighted just closes it.
                                None => this.dismiss(window, cx),
                            }
                        }))
                        .on_action(
                            cx.listener(|this, _: &Cancel, window, cx| this.dismiss(window, cx)),
                        )
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if !hovered && this.selected_by_pointer {
                                this.selected_by_pointer = false;
                                this.selected = None;
                                cx.notify();
                            }
                        }))
                        // A click on the menu isn't a click on what's under
                        // it, and one anywhere else closes it.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down_out(
                            cx.listener(|this, _, window, cx| this.dismiss(window, cx)),
                        )
                        .children(items),
                ),
        )
        .with_priority(1)
    }
}

/// A way of moving the highlight through a menu's items.
#[derive(Debug, Clone, Copy)]
enum Step {
    Next,
    Previous,
    First,
    Last,
}

/// The item highlighted after `step` from `selected`, among `count`. As in
/// a native menu, ↓ with nothing highlighted goes to the first and ↑ to the
/// last, and they stop at the ends.
fn moved(selected: Option<usize>, count: usize, step: Step) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match (step, selected) {
        (Step::First, _) | (Step::Next, None) => 0,
        (Step::Last, _) | (Step::Previous, None) => last,
        (Step::Next, Some(index)) => (index + 1).min(last),
        (Step::Previous, Some(index)) => index.saturating_sub(1),
    })
}

/// Has `element` do `open` on a secondary click: the right mouse button, a
/// two-finger click, or a Control-click. `open` is given where the click
/// was, and the click goes no further, so a row's menu opens rather than
/// its list's.
pub fn on_secondary_click<E: InteractiveElement, T: 'static>(
    element: E,
    cx: &mut Context<T>,
    open: impl Fn(&mut T, Point<Pixels>, &mut Window, &mut Context<T>) + Clone + 'static,
) -> E {
    let control_open = open.clone();
    element
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                open(this, event.position, window, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if event.modifiers.control {
                    cx.stop_propagation();
                    control_open(this, event.position, window, cx);
                }
            }),
        )
}

/// A button only VoiceOver finds, at the end of a row named `name`, which
/// opens the row's context menu with `open`, at the pointer. VoiceOver has
/// no way to make a secondary click.
pub fn a11y_menu_button<T: 'static>(
    id: impl Into<ElementId>,
    name: &str,
    cx: &mut Context<T>,
    open: impl Fn(&mut T, Point<Pixels>, &mut Window, &mut Context<T>) + 'static,
) -> Stateful<Div> {
    let this = cx.weak_entity();
    div()
        .id(id)
        .absolute()
        .right_0()
        .top_0()
        .size(px(1.))
        .overflow_hidden()
        .role(Role::Button)
        .aria_label(SharedString::from(format!("Actions for {name}")))
        .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
            let position = window.mouse_position();
            this.update(cx, |this, cx| open(this, position, window, cx))
                .ok();
        })
}

/// Whether a click is a Control-click, which opens a context menu instead
/// of being a click.
pub fn is_control_click(event: &gpui::ClickEvent) -> bool {
    event.modifiers().control
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_and_up_start_at_the_ends() {
        assert_eq!(moved(None, 3, Step::Next), Some(0));
        assert_eq!(moved(None, 3, Step::Previous), Some(2));
    }

    #[test]
    fn down_and_up_stop_at_the_ends() {
        assert_eq!(moved(Some(0), 3, Step::Next), Some(1));
        assert_eq!(moved(Some(2), 3, Step::Next), Some(2));
        assert_eq!(moved(Some(1), 3, Step::Previous), Some(0));
        assert_eq!(moved(Some(0), 3, Step::Previous), Some(0));
    }

    #[test]
    fn home_and_end_go_to_the_ends() {
        assert_eq!(moved(Some(1), 3, Step::First), Some(0));
        assert_eq!(moved(Some(1), 3, Step::Last), Some(2));
    }

    #[test]
    fn an_empty_menu_highlights_nothing() {
        assert_eq!(moved(None, 0, Step::Next), None);
    }
}
