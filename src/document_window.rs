//! A document window: the view Kertas wraps a document view in.
//!
//! The document view shows one document. The window around it has the tab
//! strip, the window's title and edited marker, the bar for problems in
//! the settings file, printing, and the actions for tabs, zoom and closing
//! the window. It hears what the document view doesn't handle itself
//! through [`DocumentEvent`]s.

use std::path::Path;

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext, Context, Entity, Focusable, KeyBinding, Render,
    Role, Subscription, SystemWindowTabController, Window, WindowHandle, actions, div, prelude::*,
    px, relative, rems,
};
use kertas_core::titles;

use crate::app_settings::AppSettings;
use crate::builtin::Builtin;
use crate::menus::OpenSettings;
use crate::tab_strip::{render_tab_strip, select_tab};
use crate::{windows, zoom};
use kertas_editor::Theme;
use kertas_editor::{DocumentEvent, DocumentView, bar_button};

actions!(
    kertas,
    [
        New,
        Open,
        Print,
        CloseWindow,
        Quit,
        NextTab,
        PreviousTab,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectTab9,
        ZoomIn,
        ZoomOut,
        ResetZoom,
    ]
);

/// Key bindings for document windows (macOS conventions).
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-n", New, None),
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("cmd-p", Print, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-shift-]", NextTab, None),
        KeyBinding::new("cmd-shift-[", PreviousTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("cmd-1", SelectTab1, None),
        KeyBinding::new("cmd-2", SelectTab2, None),
        KeyBinding::new("cmd-3", SelectTab3, None),
        KeyBinding::new("cmd-4", SelectTab4, None),
        KeyBinding::new("cmd-5", SelectTab5, None),
        KeyBinding::new("cmd-6", SelectTab6, None),
        KeyBinding::new("cmd-7", SelectTab7, None),
        KeyBinding::new("cmd-8", SelectTab8, None),
        KeyBinding::new("cmd-9", SelectTab9, None),
        KeyBinding::new("cmd-=", ZoomIn, None),
        KeyBinding::new("cmd-+", ZoomIn, None),
        KeyBinding::new("cmd--", ZoomOut, None),
        KeyBinding::new("cmd-0", ResetZoom, None),
    ]
}

pub struct DocumentWindow {
    view: Entity<DocumentView>,
    /// Set when showing one of Kertas's built-in documents.
    builtin: Option<Builtin>,
    /// The folder shown after the file's name in the title, if another
    /// open file has the same name.
    title_hint: Option<String>,
    /// Whether the user closed the bar reporting problems in the settings
    /// file. It shows again when the file changes.
    settings_problems_dismissed: bool,
    _subscriptions: [Subscription; 4],
}

impl DocumentWindow {
    pub fn new(view: Entity<DocumentView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Closing the window (e.g. its close button) asks about unsaved
        // changes first.
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |this, cx| this.should_close(window, cx))
                .unwrap_or(true)
        });
        let subscriptions = [
            cx.subscribe_in(&view, window, Self::document_event),
            cx.observe_global_in::<AppSettings>(window, |this, _, cx| {
                this.settings_problems_dismissed = false;
                cx.notify();
            }),
            // Redraw the tab strip when tabs are added, removed or renamed.
            cx.observe_global_in::<SystemWindowTabController>(window, |_, _, cx| cx.notify()),
            cx.observe_window_activation(window, |_, window, cx| {
                if window.is_window_active() {
                    // The default Markdown app can change in Finder.
                    crate::menus::refresh(cx);
                }
            }),
        ];
        Self {
            view,
            builtin: None,
            title_hint: None,
            settings_problems_dismissed: false,
            _subscriptions: subscriptions,
        }
    }

    /// The document view the window shows.
    pub fn view(&self) -> &Entity<DocumentView> {
        &self.view
    }

    /// Shows one of Kertas's built-in documents, view-only.
    pub fn open_builtin(&mut self, builtin: Builtin, window: &mut Window, cx: &mut Context<Self>) {
        self.builtin = Some(builtin);
        self.view.update(cx, |view, cx| {
            view.show_text(&builtin.text(), builtin.title(), window, cx)
        });
    }

    /// The built-in document shown, if any.
    pub fn builtin(&self) -> Option<Builtin> {
        self.builtin
    }

    /// Sets the folder shown after the file's name, to tell it from other
    /// open files with the same name.
    pub fn set_title_hint(
        &mut self,
        hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.title_hint != hint {
            self.title_hint = hint;
            self.apply_title(window, cx);
        }
    }

    /// Whether the bar reporting problems in the settings file is showing,
    /// for the smoke test.
    #[cfg(debug_assertions)]
    pub fn settings_problems_shown(&self) -> bool {
        !self.settings_problems_dismissed
    }

    fn document_event(
        &mut self,
        _: &Entity<DocumentView>,
        event: &DocumentEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            // Every other window's title too, since they may share a name
            // (#95).
            DocumentEvent::TitleChanged => {
                self.apply_title(window, cx);
                cx.defer(windows::refresh_titles);
            }
            // Show unsaved changes in the window's close button and tab.
            DocumentEvent::DirtyChanged(dirty) => {
                window.set_window_edited(*dirty);
                cx.notify();
            }
            DocumentEvent::OpenFile { path, anchor } => {
                let (path, anchor) = (path.clone(), anchor.clone());
                cx.defer(move |cx| windows::open_path_at(path, anchor, cx));
            }
        }
    }

    fn apply_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.view.read(cx);
        let Some(name) = view.display_name() else {
            return;
        };
        let title = match self.builtin {
            Some(_) => name,
            None => titles::title(&name, self.title_hint.as_deref()),
        };
        window.set_window_title(&title);
        window.set_document_path(view.document_path());
        // GPUI doesn't update tab titles from the window title.
        SystemWindowTabController::update_tab_title(
            cx,
            window.window_handle().window_id(),
            title.into(),
        );
    }

    /// Whether the window may close now. With unsaved changes, asks first
    /// and closes the window itself once that's resolved.
    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let view = self.view.clone();
        if view.update(cx, |view, cx| view.ready_to_close(window, cx)) {
            return true;
        }
        let confirmed = view.update(cx, |view, cx| view.confirm_close(window, cx));
        cx.spawn_in(window, async move |_, cx| {
            if confirmed.await {
                cx.update(|window, _| window.remove_window()).ok();
            }
        })
        .detach();
        false
    }

    fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        if self.should_close(window, cx) {
            window.remove_window();
        }
    }

    /// File > Print…: the standard print panel, as a sheet on this window.
    fn print(&mut self, _: &Print, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            let view = self.view.read(cx);
            let Some(model) = view.model() else {
                return;
            };
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                return;
            };
            let Some(ns_window) = crate::native_tabs::ns_window(window) else {
                return;
            };
            crate::print::print(
                &model,
                view.document_path(),
                view.web_images_loaded(),
                &view.config().text,
                &view.display_name().unwrap_or_default(),
                &ns_window,
                mtm,
            );
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (window, cx);
    }

    /// A bar at the bottom of the window reporting problems in the settings
    /// file (G4) and theme files, until the user closes it.
    fn render_settings_problems(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
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
                        this.view.focus_handle(cx).focus(window, cx);
                        cx.notify();
                    })),
                )
                .into_any_element(),
        )
    }
}

impl Render for DocumentWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view.read(cx);
        let theme = view.theme();
        let dirty = view.is_dirty();
        let size = view.config().font_size;
        let zoomed = view.config().is_zoomed();
        // The bars are sized in rems too, so they zoom with the text.
        window.set_rem_size(px(size as f32));
        let tab_strip = render_tab_strip(window, cx, theme, dirty);
        let problems_bar = self.render_settings_problems(theme, cx);
        div()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text)
            .text_size(rems(1.))
            .line_height(relative(1.6))
            .flex()
            .flex_col()
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
            .when_some(zoom::larger(size), |this, size| {
                this.on_action(move |_: &ZoomIn, _, cx| zoom::set(size, cx))
            })
            .when_some(zoom::smaller(size), |this, size| {
                this.on_action(move |_: &ZoomOut, _, cx| zoom::set(size, cx))
            })
            .when(zoomed, |this| {
                this.on_action(|_: &ResetZoom, _, cx| zoom::reset(cx))
            })
            .children(tab_strip)
            .child(div().flex_1().min_h(px(0.)).child(self.view.clone()))
            .children(problems_bar)
    }
}

/// A document window's handle, which reaches the document view inside it
/// as a window handle reaches its root view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentWindowHandle(pub WindowHandle<DocumentWindow>);

impl DocumentWindowHandle {
    /// `window`'s handle, if it's a document window.
    pub fn downcast(window: AnyWindowHandle) -> Option<Self> {
        window.downcast::<DocumentWindow>().map(Self)
    }

    /// Updates the window's document view.
    pub fn update<C: AppContext, R>(
        &self,
        cx: &mut C,
        update: impl FnOnce(&mut DocumentView, &mut Window, &mut Context<DocumentView>) -> R,
    ) -> gpui::Result<R> {
        self.0.update(cx, |this, window, cx| {
            this.view.update(cx, |view, cx| update(view, window, cx))
        })
    }

    /// The window's document view.
    pub fn read<'a>(&self, cx: &'a App) -> gpui::Result<&'a DocumentView> {
        Ok(self.0.read(cx)?.view.read(cx))
    }

    /// Reads the window's document view, with a callback.
    pub fn read_with<C: AppContext, R>(
        &self,
        cx: &C,
        read_with: impl FnOnce(&DocumentView, &App) -> R,
    ) -> gpui::Result<R> {
        self.0
            .read_with(cx, |this, cx| read_with(this.view.read(cx), cx))
    }

    /// The document's file, if it has one.
    pub fn document_path<'a>(&self, cx: &'a App) -> Option<&'a Path> {
        self.read(cx).ok()?.document_path()
    }
}

impl std::ops::Deref for DocumentWindowHandle {
    type Target = AnyWindowHandle;

    fn deref(&self) -> &AnyWindowHandle {
        &self.0
    }
}

impl From<DocumentWindowHandle> for AnyWindowHandle {
    fn from(handle: DocumentWindowHandle) -> Self {
        handle.0.into()
    }
}
