//! The settings window (G5), opened with Kami > Settings… (⌘,).
//!
//! It's a sheet on the front document window, so it can't get lost behind
//! it. It shows the same settings as the settings file and the View menu,
//! and changing one here writes it to the file, so all three always agree.

use gpui::{
    App, Bounds, Context, FocusHandle, Global, KeyBinding, MouseButton, SharedString, Subscription,
    Window, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, prelude::*, px, size,
};
use kami::settings::{Layout, Settings, TextWidth, ThemeSetting};

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
    let bounds = Bounds::centered(None, size(px(520.), px(290.)), cx);
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

pub struct SettingsWindow {
    focus_handle: FocusHandle,
    theme: Theme,
    _settings_observer: Subscription,
    _appearance_observer: Subscription,
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
            _appearance_observer: cx.observe_window_appearance(window, |this, window, cx| {
                this.theme = Theme::current(window, cx);
                cx.notify();
            }),
        }
    }

    /// A row: a label, then buttons for the choices with the current one
    /// selected. Choosing one sets `key` in the settings file.
    fn choices<T: Copy + PartialEq + 'static>(
        &self,
        label: &'static str,
        key: &'static str,
        current: T,
        options: &[(&'static str, T, &'static str)],
        note: Option<&'static str>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let buttons = options.iter().map(|&(name, value, setting)| {
            let selected = value == current;
            div()
                .id(SharedString::from(format!("{key}-{setting}")))
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
                    cx.defer(move |cx| app_settings::set_or_report(key, setting, cx));
                })
                .child(name)
        });
        div()
            .flex()
            .gap_4()
            .child(
                div()
                    .w(px(130.))
                    .flex_none()
                    .text_right()
                    .pt(px(4.))
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    // Keep the buttons as wide as their labels, not the note.
                    .items_start()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .p(px(2.))
                            .rounded_lg()
                            .bg(theme.code_background)
                            .children(buttons),
                    )
                    .children(note.map(|note| {
                        div()
                            .text_size(px(11.))
                            .text_color(theme.muted_text)
                            .child(note)
                    })),
            )
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Settings {
            theme,
            layout,
            text_width,
        } = app_settings::get(cx);
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
            .key_context(KEY_CONTEXT)
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .size_full()
            .flex()
            .flex_col()
            .gap_5()
            .p_6()
            .bg(self.theme.background)
            .text_color(self.theme.text)
            .text_size(px(13.))
            .child(self.choices(
                "Appearance",
                "theme",
                theme,
                &[
                    ("Light", ThemeSetting::Light, "light"),
                    ("Dark", ThemeSetting::Dark, "dark"),
                    ("Match System", ThemeSetting::System, "system"),
                ],
                None,
            ))
            .child(self.choices(
                "Text width",
                "text_width",
                text_width,
                &[
                    ("Standard", TextWidth::Standard, "standard"),
                    ("Wide", TextWidth::Wide, "wide"),
                    ("Full Width", TextWidth::Full, "full"),
                ],
                None,
            ))
            .child(self.choices(
                "Open documents in",
                "layout",
                layout,
                &[
                    ("Tabs", Layout::Tabs, "tabs"),
                    ("Windows", Layout::Windows, "windows"),
                ],
                Some("Applies to documents opened from now on."),
            ))
            .child(div().flex_1())
            .child(div().flex().justify_end().child(done))
    }
}
