//! Controls for settings windows and other forms, drawn as Kertas's
//! settings window draws its: a [`labelled`] row for each setting, holding
//! [`choices`] (a segmented control), a [`checkbox`], a [`stepper`] or a
//! [`button`], with a [`note`] beneath if it needs one.
//!
//! Each control is reachable with Tab, says what it is to VoiceOver, and
//! has a ring while it has keyboard focus. What choosing does is the
//! app's: each control takes a callback.

use gpui::{
    App, Div, ElementId, Role, SharedString, Stateful, Toggled, Window, div, prelude::*, px,
};

use crate::Theme;

/// The width of the labels on the left of each row.
pub const LABEL_WIDTH: f32 = 150.;

/// A label on the left, `LABEL_WIDTH` wide, and `content` on the right.
pub fn labelled(label: impl Into<SharedString>, content: impl IntoElement) -> Div {
    div()
        .flex()
        .gap_4()
        .child(
            div()
                .w(px(LABEL_WIDTH))
                .flex_none()
                .text_right()
                .pt(px(4.))
                .child(label.into()),
        )
        // Take the remaining width, so long notes wrap.
        .child(div().flex_1().min_w(px(0.)).child(content))
}

/// `content` with `note`, if any, beneath it.
pub fn with_note(content: impl IntoElement, note: Option<SharedString>, theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        // Keep the content as wide as it needs, not the note.
        .items_start()
        .gap_1()
        .child(content)
        .children(note.map(|note| self::note(note, theme)))
}

/// Small explanatory text.
pub fn note(text: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .text_size(px(11.))
        .text_color(theme.muted_text)
        .child(text.into())
}

/// `element` made into a control: reachable with Tab, with an accessible
/// `role` and `name`, and a ring when it has keyboard focus. Its border
/// shows the ring, so give it one.
pub fn control(
    element: Stateful<Div>,
    role: Role,
    name: impl Into<SharedString>,
    theme: Theme,
) -> Stateful<Div> {
    element
        .focusable()
        .tab_stop(true)
        .role(role)
        .aria_label(name)
        .cursor_pointer()
        .focus_visible(move |style| style.border_color(theme.caret))
}

/// The accessible state of a checkbox or a choice.
pub fn toggled(on: bool) -> Toggled {
    if on { Toggled::True } else { Toggled::False }
}

/// A segmented control: a button for each of `labels`, with the
/// `selected` one standing out, as radio buttons for VoiceOver, named
/// `name`. Choosing one calls `on_choose` with its index.
pub fn choices(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    labels: &[&'static str],
    selected: Option<usize>,
    theme: Theme,
    on_choose: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let count = labels.len();
    let on_choose = std::rc::Rc::new(on_choose);
    let buttons = labels.iter().enumerate().map(|(index, &label)| {
        let chosen = selected == Some(index);
        let on_choose = on_choose.clone();
        control(
            div().id(ElementId::NamedChild(
                std::sync::Arc::new(id.clone()),
                SharedString::from(label),
            )),
            Role::RadioButton,
            label,
            theme,
        )
        .aria_toggled(toggled(chosen))
        .aria_position_in_set(index + 1)
        .aria_size_of_set(count)
        .px_3()
        .py(px(3.))
        .rounded_md()
        .border_1()
        .when(chosen, |this| {
            this.bg(theme.background)
                .text_color(theme.text)
                .border_color(theme.rule)
        })
        .when(!chosen, |this| {
            this.text_color(theme.muted_text)
                .border_color(gpui::transparent_black())
                .hover(|style| style.text_color(theme.text))
        })
        .on_click(move |_, window, cx| {
            if !chosen {
                on_choose(index, window, cx);
            }
        })
        .child(label)
    });
    let buttons: Vec<_> = buttons.collect();
    div()
        .id(id)
        .role(Role::RadioGroup)
        .aria_label(name)
        .flex()
        .gap_1()
        .p(px(2.))
        .rounded_lg()
        .bg(theme.code_background)
        .children(buttons)
}

/// A checkbox labelled `label`, checked or not. Clicking it calls
/// `on_toggle` with what it should become.
pub fn checkbox(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    theme: Theme,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label = label.into();
    control(div().id(id), Role::CheckBox, label.clone(), theme)
        .aria_toggled(toggled(checked))
        .flex()
        .items_center()
        .gap_2()
        .mt(px(2.))
        .px_1()
        .rounded_md()
        .border_1()
        .border_color(gpui::transparent_black())
        .on_click(move |_, window, cx| on_toggle(!checked, window, cx))
        .child(
            div()
                .flex_none()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .border_1()
                .border_color(if checked {
                    theme.caret
                } else {
                    theme.muted_text
                })
                .when(checked, |this| {
                    this.bg(theme.caret)
                        .text_color(theme.accent_text)
                        .text_size(px(10.))
                        .child("✓")
                }),
        )
        .child(label)
}

/// A button showing `label`, named `name` for VoiceOver.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    name: impl Into<SharedString>,
    theme: Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    control(div().id(id), Role::Button, name, theme)
        .px_3()
        .py(px(3.))
        .rounded_md()
        .border_1()
        .border_color(theme.rule)
        .bg(theme.code_background)
        .hover(|style| style.bg(theme.rule))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(label.into())
}

/// The window's default button, such as Done, in the accent color.
pub fn default_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label = label.into();
    let accent_text = theme.accent_text;
    control(div().id(id), Role::DefaultButton, label.clone(), theme)
        .px_4()
        .py(px(4.))
        .rounded_md()
        .border_1()
        .border_color(theme.caret)
        .focus_visible(move |style| style.border_color(accent_text))
        .bg(theme.caret)
        .text_color(accent_text)
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(label)
}

/// The numbers a [`stepper`] goes through: `step` at a time within
/// `range`, shown with `unit`, such as " pt".
pub struct Steps {
    pub range: std::ops::RangeInclusive<u32>,
    pub step: u32,
    pub unit: &'static str,
}

/// A number with − and + buttons, named `name` for VoiceOver: `value`,
/// which − and + change through `steps`. Changing it calls `on_change`
/// with the new value.
pub fn stepper(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    value: u32,
    steps: Steps,
    theme: Theme,
    on_change: impl Fn(u32, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let Steps { range, step, unit } = steps;
    let id = id.into();
    let name = name.into();
    let on_change = std::rc::Rc::new(on_change);
    let (min, max) = (*range.start(), *range.end());
    let smaller = value.saturating_sub(step).max(min);
    let larger = value.saturating_add(step).min(max);
    let step_button = |child: &'static str, label: String, to: u32, enabled: bool| {
        let on_change = on_change.clone();
        control(
            div().id(ElementId::NamedChild(
                std::sync::Arc::new(id.clone()),
                child.into(),
            )),
            Role::Button,
            label,
            theme,
        )
        .w(px(26.))
        .py(px(1.))
        .text_center()
        .rounded_md()
        .border_1()
        .border_color(theme.rule)
        .bg(theme.code_background)
        .when(enabled, |this| {
            this.hover(|style| style.bg(theme.rule))
                .on_click(move |_, window, cx| on_change(to, window, cx))
        })
        .when(!enabled, |this| {
            this.text_color(theme.muted_text.opacity(0.5))
                .cursor_default()
        })
        .child(child)
    };
    div()
        .id(id.clone())
        .role(Role::Group)
        .aria_label(name.clone())
        .flex()
        .items_center()
        .gap_2()
        .child(step_button(
            "−",
            format!("Smaller {name}"),
            smaller,
            value > min,
        ))
        .child(
            div()
                .min_w(px(56.))
                .text_center()
                .child(format!("{value}{unit}")),
        )
        .child(step_button(
            "+",
            format!("Larger {name}"),
            larger,
            value < max,
        ))
}
