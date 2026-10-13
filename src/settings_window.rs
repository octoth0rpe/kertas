//! The settings window (G5), opened with Kami > Settings… (⌘,).
//!
//! It's a sheet on the front document window, so it can't get lost behind
//! it. It shows the same settings as the settings file and the View menu,
//! and changing one here writes it to the file, so all three always agree.

use std::ops::RangeInclusive;

use gpui::{
    AnyElement, App, Bounds, Context, FocusHandle, Global, KeyBinding, MouseButton, SharedString,
    Subscription, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions, anchored,
    deferred, div, prelude::*, px, size,
};
use kami::settings::{Kind, SETTINGS, Setting, Settings};

use crate::app_settings::{self, AppSettings};
use crate::document_view::{CloseWindow, DocumentView};
use crate::theme::Theme;
#[cfg(target_os = "macos")]
use crate::{cli_tool, default_app};

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
    let bounds = Bounds::centered(None, size(px(540.), px(700.)), cx);
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

/// Keys that close the settings window, as for any sheet.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", CloseWindow, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", CloseWindow, Some(KEY_CONTEXT)),
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

pub struct SettingsWindow {
    focus_handle: FocusHandle,
    theme: Theme,
    /// Who opens Markdown files, checked whenever the window is activated,
    /// since it can change in Finder.
    #[cfg(target_os = "macos")]
    markdown_app: default_app::Status,
    #[cfg(target_os = "macos")]
    cli_tool: cli_tool::Status,
    /// Why installing or uninstalling the command failed.
    #[cfg(target_os = "macos")]
    cli_error: Option<String>,
    #[cfg(target_os = "macos")]
    _activation_observer: Subscription,
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
            #[cfg(target_os = "macos")]
            markdown_app: default_app::status(),
            #[cfg(target_os = "macos")]
            cli_tool: cli_tool::status(),
            #[cfg(target_os = "macos")]
            cli_error: None,
            #[cfg(target_os = "macos")]
            _activation_observer: cx
                .observe_window_activation(window, |this, _, cx| this.refresh(cx)),
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
        let content = div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.button("smaller", "−", cx, set(current.saturating_sub(1))))
            .child(
                div()
                    .w(px(44.))
                    .text_center()
                    .child(format!("{current} {unit}")),
            )
            .child(self.button("larger", "+", cx, set(current + 1)))
            .when(current != default, |this| {
                this.child(self.button("reset-size", "Reset", cx, set(default)))
            });
        self.labelled(setting.label, self.with_note(content, setting.note))
    }

    /// A row for a font setting: a pop-up button showing the font, which
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
        let button = div()
            .id(key)
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py(px(3.))
            .rounded_md()
            .cursor_pointer()
            .border_1()
            .border_color(theme.rule)
            .bg(theme.code_background)
            .hover(|style| style.bg(theme.rule))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.font_menu = if open { None } else { Some(key) };
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(current.unwrap_or(default).to_owned())
            .child(div().text_color(theme.muted_text).child("▾"));

        let item = |id: SharedString,
                    label: SharedString,
                    font: Option<String>,
                    cx: &mut Context<Self>| {
            let selected = font.as_deref() == current;
            div()
                .id(id)
                .flex()
                .gap_2()
                .px_3()
                .py(px(2.))
                .cursor_pointer()
                .hover(|style| style.bg(theme.rule))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.font_menu = None;
                        let font = font.clone();
                        cx.defer(move |cx| match font {
                            Some(font) => app_settings::set_or_report(key, font, cx),
                            None => app_settings::remove_or_report(key, cx),
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
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
        let content = div()
            .flex()
            .flex_col()
            .items_start()
            .child(button)
            .children(menu);
        self.labelled(setting.label, self.with_note(content, setting.note))
    }

    /// A row for a `true`/`false` setting: a checkbox labelled with the
    /// setting's note.
    fn checkbox(&self, setting: &'static Setting, checked: bool) -> impl IntoElement {
        let theme = self.theme;
        let key = setting.key;
        let checkbox = div()
            .id(key)
            .flex()
            .items_center()
            .gap_2()
            .pt(px(3.))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
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
    fn with_note(&self, content: impl IntoElement, note: Option<&'static str>) -> gpui::Div {
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

    /// A button that runs `action` when clicked.
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id(id)
            .px_3()
            .py(px(3.))
            .rounded_md()
            .cursor_pointer()
            .border_1()
            .border_color(theme.rule)
            .bg(theme.code_background)
            .hover(|style| style.bg(theme.rule))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| action(this, cx)),
            )
            .child(label)
    }

    /// A value (a checkmark, or "—") with a note beneath it.
    fn status(&self, value: &'static str, note: Option<String>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_1()
            .child(div().pt(px(4.)).child(value))
            .children(note.map(|note| self.note(note)))
    }

    /// A button with a note beneath it.
    fn offer(&self, button: impl IntoElement, note: impl Into<SharedString>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_1()
            .child(button)
            .child(self.note(note))
    }

    /// The "Default Markdown app" row: a button to make Kami the default,
    /// with the current default beneath it, or a checkmark if Kami is.
    #[cfg(target_os = "macos")]
    fn markdown_files(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.markdown_app {
            default_app::Status::Kami => self.status("✓ Kami", None),
            default_app::Status::Other(app) => {
                let button = self.button("make-default", "Make Kami the Default", cx, |_, cx| {
                    let result = default_app::make_default();
                    cx.spawn(async move |this, cx| {
                        // macOS asks the user first; refresh either way.
                        result.await.ok();
                        this.update(cx, |this, cx| this.refresh(cx)).ok();
                    })
                    .detach();
                });
                self.offer(
                    button,
                    format!("Currently: {}", app.as_deref().unwrap_or("none")),
                )
            }
            default_app::Status::NotInstalled => {
                self.status("—", Some("Install Kami.app to change".to_owned()))
            }
        };
        self.labelled("Default Markdown app", content)
    }

    /// The "Command line tool" row: install or uninstall `kami`.
    #[cfg(target_os = "macos")]
    fn command_line_tool(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let install = |this: &mut Self, cx: &mut Context<Self>| this.run_cli(cli_tool::install, cx);
        let content = match self.cli_tool {
            cli_tool::Status::Installed => div()
                .flex()
                .items_center()
                .gap_3()
                .child("✓ kami")
                .child(self.button("uninstall-cli", "Uninstall", cx, |this, cx| {
                    this.run_cli(cli_tool::uninstall, cx)
                })),
            cli_tool::Status::NotInstalled => self.offer(
                self.button("install-cli", "Install the kami Command", cx, install),
                format!("Adds {}", cli_tool::LINK),
            ),
            cli_tool::Status::OtherKami => self.offer(
                self.button("install-cli", "Update the kami Command", cx, install),
                "It opens another copy of Kami.app",
            ),
            cli_tool::Status::Taken => self.status(
                "—",
                Some(format!("{} belongs to another program", cli_tool::LINK)),
            ),
            cli_tool::Status::NotInstalledApp => {
                self.status("—", Some("Install Kami.app to change".to_owned()))
            }
        };
        let content = content.children(
            self.cli_error
                .clone()
                .map(|error| self.note(format!("Couldn't change it: {error}"))),
        );
        self.labelled("Command line tool", content)
    }

    /// Installs or uninstalls the command off the main thread, since macOS
    /// may ask for a password, then refreshes.
    #[cfg(target_os = "macos")]
    fn run_cli(&mut self, change: fn() -> Result<(), String>, cx: &mut Context<Self>) {
        self.cli_error = None;
        let task = cx.background_spawn(async move { change() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                // Cancelling the password prompt isn't an error.
                this.cli_error = result.err().filter(|error| !error.contains("(-128)"));
                this.refresh(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Re-checks what can change outside Kami.
    #[cfg(target_os = "macos")]
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.markdown_app = default_app::status();
        self.cli_tool = cli_tool::status();
        cx.notify();
    }

    /// A row: a label, then buttons for the choices with the current one
    /// selected. Choosing one sets the setting in the settings file.
    fn choices(&self, setting: &'static Setting, choices: Vec<Choice>) -> impl IntoElement {
        let theme = self.theme;
        let key = setting.key;
        let buttons = choices.into_iter().map(|choice| {
            let Choice {
                label,
                value,
                selected,
            } = choice;
            div()
                .id(SharedString::from(format!("{key}-{label}")))
                .px_3()
                .py(px(3.))
                .rounded_md()
                .cursor_pointer()
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
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    // Applying a setting updates this window, so wait until
                    // this event is handled.
                    let value = value.clone();
                    cx.defer(move |cx| app_settings::set_or_report(key, value, cx));
                })
                .child(label)
        });
        let buttons = div()
            .flex()
            .gap_1()
            .p(px(2.))
            .rounded_lg()
            .bg(theme.code_background)
            .children(buttons);
        self.labelled(setting.label, self.with_note(buttons, setting.note))
    }

    /// The row for `setting`, showing its value in `settings`.
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
                self.choices(setting, choices).into_any_element()
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
                self.choices(setting, choices).into_any_element()
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
        }
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = app_settings::get(cx);
        let rows: Vec<AnyElement> = SETTINGS
            .iter()
            .map(|setting| self.row(setting, &settings, cx))
            .collect();
        #[cfg(target_os = "macos")]
        let system_rows = [
            self.markdown_files(cx).into_any_element(),
            self.command_line_tool(cx).into_any_element(),
        ];
        #[cfg(not(target_os = "macos"))]
        let system_rows: [gpui::AnyElement; 0] = [];
        let done = div()
            .id("done")
            .px_4()
            .py(px(4.))
            .rounded_md()
            .cursor_pointer()
            .bg(self.theme.caret)
            .text_color(gpui::white())
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(CloseWindow), cx)
            })
            .child("Done");
        div()
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
            .children(rows)
            .children(system_rows)
            .child(div().flex_1())
            .child(div().flex().justify_end().child(done))
    }
}
