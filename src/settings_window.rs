//! The settings window (G5), opened with Kami > Settings… (⌘,).
//!
//! It's a sheet on the front document window, so it can't get lost behind
//! it. It shows the same settings as the settings file and the View menu,
//! and changing one here writes it to the file, so all three always agree.

use std::ops::RangeInclusive;

use gpui::{
    AnyElement, App, Bounds, Context, Div, FocusHandle, Global, KeyBinding, MouseButton, Role,
    SharedString, Stateful, Subscription, Toggled, Window, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, actions, anchored, deferred, div, prelude::*, px, size,
};
use kami::settings::{Kind, SETTINGS, Setting, Settings};

use crate::app_settings::{self, AppSettings};
use crate::document_view::{CloseWindow, DocumentView};
use crate::theme::Theme;

/// The settings window, if it's open.
#[derive(Default)]
struct OpenSettingsWindow(Option<WindowHandle<SettingsWindow>>);

impl Global for OpenSettingsWindow {}

/// Opens the settings window, or brings it to the front if it's open.
pub fn open(cx: &mut App) {
    let existing = cx.default_global::<OpenSettingsWindow>().0;
    if let Some(window) = existing
        && window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    // A sheet attaches to the main window, so make sure one is in front.
    if let Some(document) = cx
        .window_stack()
        .unwrap_or_default()
        .into_iter()
        .find_map(|window| window.downcast::<DocumentView>())
    {
        document
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    }
    // The sheet then fits itself to its rows (see `fit_to_rows`).
    cx.set_global(RowsFit::default());
    let height = px(INITIAL_HEIGHT).min(max_height(cx));
    let bounds = Bounds::centered(None, size(px(540.), height), cx);
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::Dialog,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| SettingsWindow::new(window, cx)),
        )
        .ok();
    cx.set_global(OpenSettingsWindow(window));
    cx.activate(true);
}

actions!(settings_window, [FocusNext, FocusPrevious]);

/// Keys for the settings window. Esc and Return close it, as for any
/// sheet (Return is Done, the default button), and Tab and ⇧Tab move
/// between its controls. Space activates the focused control.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", CloseWindow, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", CloseWindow, Some(KEY_CONTEXT)),
        KeyBinding::new("tab", FocusNext, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", FocusPrevious, Some(KEY_CONTEXT)),
    ]
}

const KEY_CONTEXT: &str = "SettingsWindow";

/// The width of the labels on the left of each row.
const LABEL_WIDTH: f32 = 150.;

/// One of a setting's choices in the settings sheet.
struct Choice {
    label: SharedString,
    /// The setting's value in the file when it's chosen.
    value: toml_edit::Value,
    selected: bool,
}

/// About how tall the settings sheet is, before it fits itself to its
/// rows.
const INITIAL_HEIGHT: f32 = 604.;

/// The space between the last row and the Done button's row.
const ROWS_GAP: f32 = 20.;

/// The tallest the sheet gets: the screen's height, less some room. Taller
/// rows scroll.
fn max_height(cx: &App) -> gpui::Pixels {
    cx.primary_display().map_or(px(INITIAL_HEIGHT), |display| {
        display.bounds().size.height - px(80.)
    })
}

/// Where the rows end and the Done button's row starts, when the settings
/// sheet was last laid out. The sheet fits itself to its rows with these,
/// and the smoke test checks they fit without scrolling.
#[derive(Default)]
pub struct RowsFit {
    pub rows_bottom: gpui::Pixels,
    pub footer_top: gpui::Pixels,
    /// The sheet's height then.
    pub height: gpui::Pixels,
}

impl gpui::Global for RowsFit {}

/// An empty element that records where it's laid out in [`RowsFit`].
fn measure(record: fn(&mut RowsFit, gpui::Pixels)) -> impl IntoElement {
    gpui::canvas(
        move |bounds, window, cx| {
            let fit = cx.default_global::<RowsFit>();
            fit.height = window.viewport_size().height;
            record(fit, bounds.top());
        },
        |_, _, _, _| {},
    )
}

pub struct SettingsWindow {
    focus_handle: FocusHandle,
    theme: Theme,
    _settings_observer: Subscription,
    _appearance_observer: Subscription,
    /// The font setting whose list of fonts is open.
    font_menu: Option<&'static str>,
}

impl SettingsWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            focus_handle,
            theme: Theme::current(window, cx),
            _settings_observer: cx.observe_global_in::<AppSettings>(window, |this, window, cx| {
                this.theme = Theme::current(window, cx);
                cx.notify();
            }),
            font_menu: None,
            _appearance_observer: cx.observe_window_appearance(window, |this, window, cx| {
                this.theme = Theme::current(window, cx);
                cx.notify();
            }),
        }
    }

    /// A row for a whole-number setting: the number, with − and +, and
    /// Reset when it isn't the default.
    fn integer(
        &self,
        setting: &'static Setting,
        range: &RangeInclusive<u32>,
        unit: &'static str,
        current: u32,
        default: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let key = setting.key;
        let range = range.clone();
        let set = move |n: u32| {
            let range = range.clone();
            move |_: &mut Self, cx: &mut Context<Self>| {
                if range.contains(&n) {
                    cx.defer(move |cx| app_settings::set_or_report(key, i64::from(n), cx));
                }
            }
        };
        let name = setting.label.to_lowercase();
        let value = format!("{current} {unit}");
        let content = div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.button(
                format!("{key}-smaller"),
                "−",
                format!("Decrease {name}, {value}"),
                cx,
                set(current.saturating_sub(1)),
            ))
            .child(div().w(px(44.)).text_center().child(value.clone()))
            .child(self.button(
                format!("{key}-larger"),
                "+",
                format!("Increase {name}, {value}"),
                cx,
                set(current + 1),
            ))
            .when(current != default, |this| {
                this.child(self.button(
                    format!("{key}-reset"),
                    "Reset",
                    format!("Reset {name} to {default} {unit}"),
                    cx,
                    set(default),
                ))
            });
        self.labelled(setting.label, self.with_note(content, setting.note))
    }

    /// A font setting's pop-up button, labelled, showing the font. It
    /// opens a list of the installed fonts, with the default first.
    fn font(
        &self,
        setting: &'static Setting,
        default: &'static str,
        current: Option<&str>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let key = setting.key;
        let open = self.font_menu == Some(key);
        let shown = current.unwrap_or(default).to_owned();
        let button = self
            .control(
                div().id(key),
                Role::Button,
                format!("{} font, {shown}", setting.label),
            )
            .aria_expanded(open)
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py(px(3.))
            .rounded_md()
            .border_1()
            .border_color(theme.rule)
            .bg(theme.code_background)
            .hover(|style| style.bg(theme.rule))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.font_menu = if open { None } else { Some(key) };
                cx.notify();
            }))
            .child(shown)
            .child(div().text_color(theme.muted_text).child("▾"));

        let item = |id: SharedString,
                    label: SharedString,
                    font: Option<String>,
                    cx: &mut Context<Self>| {
            let selected = font.as_deref() == current;
            self.control(div().id(id), Role::MenuItemRadio, label.clone())
                .aria_toggled(toggled(selected))
                .flex()
                .gap_2()
                .px_3()
                .py(px(2.))
                .border_1()
                .border_color(gpui::transparent_black())
                .hover(|style| style.bg(theme.rule))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.font_menu = None;
                    let font = font.clone();
                    cx.defer(move |cx| match font {
                        Some(font) => app_settings::set_or_report(key, font, cx),
                        None => app_settings::remove_or_report(key, cx),
                    });
                    cx.notify();
                }))
                .child(div().w(px(12.)).child(if selected { "✓" } else { "" }))
                .child(label)
        };
        let menu = open.then(|| {
            let mut items = vec![item(
                format!("{key}-default").into(),
                default.into(),
                None,
                cx,
            )];
            items.extend(
                app_settings::installed_fonts(cx)
                    .to_vec()
                    .into_iter()
                    .map(|font| {
                        item(
                            format!("{key}-{font}").into(),
                            font.clone().into(),
                            Some(font),
                            cx,
                        )
                    }),
            );
            deferred(
                anchored().snap_to_window().child(
                    div()
                        .id(SharedString::from(format!("{key}-fonts")))
                        .role(Role::Menu)
                        .aria_label(format!("{} fonts", setting.label))
                        .occlude()
                        .mt_1()
                        .w(px(260.))
                        .max_h(px(280.))
                        .overflow_y_scroll()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.rule)
                        .bg(theme.background)
                        .shadow_md()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .children(items),
                ),
            )
            .with_priority(1)
        });
        let picker = div()
            .flex()
            .flex_col()
            .items_start()
            .child(button)
            .children(menu);
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(setting.label)
            .child(picker)
    }

    /// The fonts row: a pop-up button for each font setting, side by side.
    fn fonts(&self, pickers: Vec<AnyElement>) -> impl IntoElement {
        self.labelled(
            "Fonts",
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_x_4()
                .gap_y_2()
                .children(pickers),
        )
    }

    /// A row for a `true`/`false` setting: a checkbox labelled with the
    /// setting's note.
    fn checkbox(&self, setting: &'static Setting, checked: bool) -> impl IntoElement {
        let theme = self.theme;
        let key = setting.key;
        let checkbox = self
            .control(div().id(key), Role::CheckBox, setting.label)
            .aria_toggled(toggled(checked))
            .when_some(setting.note, |this, note| this.aria_description(note))
            .flex()
            .items_center()
            .gap_2()
            .mt(px(2.))
            .px_1()
            .rounded_md()
            .border_1()
            .border_color(gpui::transparent_black())
            .on_click(move |_, _, cx| {
                cx.defer(move |cx| app_settings::set_or_report(key, !checked, cx));
            })
            .child(
                div()
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
                            .text_color(gpui::white())
                            .text_size(px(10.))
                            .child("✓")
                    }),
            )
            .children(setting.note);
        self.labelled(setting.label, checkbox)
    }

    /// `content` with `note`, if any, beneath it.
    fn with_note(
        &self,
        content: impl IntoElement,
        note: Option<impl Into<SharedString>>,
    ) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            // Keep the content as wide as it needs, not the note.
            .items_start()
            .gap_1()
            .child(content)
            .children(note.map(|note| self.note(note)))
    }

    /// A label on the left, and `content` on the right.
    fn labelled(&self, label: &'static str, content: impl IntoElement) -> impl IntoElement {
        div()
            .flex()
            .gap_4()
            .child(
                div()
                    .w(px(LABEL_WIDTH))
                    .flex_none()
                    .text_right()
                    .pt(px(4.))
                    .child(label),
            )
            // Take the remaining width, so long notes wrap.
            .child(div().flex_1().min_w(px(0.)).child(content))
    }

    /// Small explanatory text.
    fn note(&self, text: impl Into<SharedString>) -> impl IntoElement {
        div()
            .text_size(px(11.))
            .text_color(self.theme.muted_text)
            .child(text.into())
    }

    /// `control` made into a control: reachable with Tab, with an
    /// accessible `role` and `name`, and a ring when it has keyboard focus.
    /// Its border shows the ring, so give it one.
    fn control(
        &self,
        control: Stateful<Div>,
        role: Role,
        name: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        control
            .focusable()
            .tab_stop(true)
            .role(role)
            .aria_label(name)
            .cursor_pointer()
            .focus_visible(move |style| style.border_color(theme.caret))
    }

    /// A button showing `label` that runs `action` when clicked. `name` is
    /// what it's called for VoiceOver.
    fn button(
        &self,
        id: impl Into<SharedString>,
        label: &'static str,
        name: impl Into<SharedString>,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let theme = self.theme;
        self.control(div().id(id.into()), Role::Button, name)
            .px_3()
            .py(px(3.))
            .rounded_md()
            .border_1()
            .border_color(theme.rule)
            .bg(theme.code_background)
            .hover(|style| style.bg(theme.rule))
            .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
            .child(label)
    }

    /// A row: a label, then buttons for the choices with the current one
    /// selected. Choosing one sets the setting in the settings file.
    /// `note` is shown beneath them, if any.
    fn choices(
        &self,
        setting: &'static Setting,
        choices: Vec<Choice>,
        note: Option<SharedString>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let key = setting.key;
        let count = choices.len();
        let buttons = choices.into_iter().enumerate().map(|(index, choice)| {
            let Choice {
                label,
                value,
                selected,
            } = choice;
            self.control(
                div().id(SharedString::from(format!("{key}-{label}"))),
                Role::RadioButton,
                label.clone(),
            )
            .aria_toggled(toggled(selected))
            .aria_position_in_set(index + 1)
            .aria_size_of_set(count)
            .px_3()
            .py(px(3.))
            .rounded_md()
            .when(selected, |this| {
                this.bg(theme.background)
                    .text_color(theme.text)
                    .border_1()
                    .border_color(theme.rule)
            })
            .when(!selected, |this| {
                this.text_color(theme.muted_text)
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .hover(|style| style.text_color(theme.text))
            })
            .on_click(move |_, _, cx| {
                // Applying a setting updates this window, so wait until
                // this event is handled.
                let value = value.clone();
                cx.defer(move |cx| app_settings::set_or_report(key, value, cx));
            })
            .child(label)
        });
        let buttons = div()
            .id(SharedString::from(format!("{key}-choices")))
            .role(Role::RadioGroup)
            .aria_label(setting.label)
            .flex()
            .gap_1()
            .p(px(2.))
            .rounded_lg()
            .bg(theme.code_background)
            .children(buttons);
        self.labelled(setting.label, self.with_note(buttons, note))
    }

    /// The row for `setting`, showing its value in `settings`. For a font,
    /// just its pop-up button, for the fonts row.
    fn row(
        &self,
        setting: &'static Setting,
        settings: &Settings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match &setting.kind {
            Kind::Choice { choices, get, .. } => {
                let current = get(settings);
                let choices = choices
                    .iter()
                    .map(|&(value, label)| Choice {
                        label: label.into(),
                        value: value.into(),
                        selected: value == current,
                    })
                    .collect();
                self.choices(setting, choices, setting.note.map(Into::into))
                    .into_any_element()
            }
            Kind::Numbers { choices, get, .. } => {
                let current = get(settings);
                let choices = choices
                    .iter()
                    .map(|&n| Choice {
                        label: n.to_string().into(),
                        value: i64::from(n).into(),
                        selected: n == current,
                    })
                    .collect();
                self.choices(setting, choices, setting.note.map(Into::into))
                    .into_any_element()
            }
            Kind::Integer {
                range, unit, get, ..
            } => self
                .integer(
                    setting,
                    range,
                    unit,
                    get(settings),
                    get(&Settings::default()),
                    cx,
                )
                .into_any_element(),
            Kind::Font { default, get, .. } => self
                .font(setting, default, get(settings), cx)
                .into_any_element(),
            Kind::Bool { get, .. } => self.checkbox(setting, get(settings)).into_any_element(),
            Kind::OffOrInteger { on, get, .. } => {
                let current = get(settings);
                let choices = vec![
                    Choice {
                        label: "Off".into(),
                        value: "off".into(),
                        selected: current.is_none(),
                    },
                    Choice {
                        label: "After a pause".into(),
                        value: i64::from(current.unwrap_or(*on)).into(),
                        selected: current.is_some(),
                    },
                ];
                let note = current.map(|seconds| {
                    let unit = if seconds == 1 { "second" } else { "seconds" };
                    format!("Saves {seconds} {unit} after you stop typing.").into()
                });
                self.choices(setting, choices, note).into_any_element()
            }
        }
    }
}

/// Resizes the sheet so the Done button's row is [`ROWS_GAP`] below the
/// last row, as last laid out: no more, so there's no empty space, and no
/// less, so nothing scrolls (unless the screen is too short).
fn fit_to_rows(window: &mut Window, cx: &mut App) {
    let Some(fit) = cx.try_global::<RowsFit>() else {
        return;
    };
    if fit.rows_bottom <= px(0.) {
        return;
    }
    // Below the footer's top is the same whatever the height.
    let footer = fit.height - fit.footer_top;
    let height = (fit.rows_bottom + px(ROWS_GAP) + footer).min(max_height(cx));
    let current = window.viewport_size();
    if (height - current.height).abs() > px(1.) {
        window.resize(size(current.width, height));
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Once this frame is laid out, so the rows are measured.
        window.on_next_frame(fit_to_rows);
        let settings = app_settings::get(cx);
        // The font settings share one row, where the first of them is.
        let mut rows = Vec::new();
        let mut fonts = Vec::new();
        let mut fonts_at = None;
        for setting in SETTINGS {
            let row = self.row(setting, &settings, cx);
            if matches!(setting.kind, Kind::Font { .. }) {
                fonts_at.get_or_insert(rows.len());
                fonts.push(row);
            } else {
                rows.push(row);
            }
        }
        if let Some(index) = fonts_at {
            rows.insert(index, self.fonts(fonts).into_any_element());
        }
        let done = self
            .control(div().id("done"), Role::DefaultButton, "Done")
            .px_4()
            .py(px(4.))
            .rounded_md()
            .border_1()
            .border_color(self.theme.caret)
            .focus_visible(|style| style.border_color(gpui::white()))
            .bg(self.theme.caret)
            .text_color(gpui::white())
            .on_click(|_, window, cx| window.dispatch_action(Box::new(CloseWindow), cx))
            .child("Done");
        div()
            .id("settings")
            .track_focus(&self.focus_handle)
            // A click anywhere else closes the list of fonts.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.font_menu.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .key_context(KEY_CONTEXT)
            .role(Role::Dialog)
            .aria_label("Settings")
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrevious, window, cx| window.focus_prev(cx))
            // Esc closes the list of fonts, if it's open, then the window.
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                if this.font_menu.take().is_some() {
                    cx.notify();
                } else {
                    window.remove_window();
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .gap_5()
            .p_6()
            .bg(self.theme.background)
            .text_color(self.theme.text)
            .text_size(px(13.))
            .child(
                div()
                    .id("rows")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(div().flex().flex_col().gap_5().children(rows))
                    .child(measure(|fit, top| fit.rows_bottom = top)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(measure(|fit, top| fit.footer_top = top))
                    .child(done),
            )
    }
}

fn toggled(on: bool) -> Toggled {
    if on { Toggled::True } else { Toggled::False }
}
