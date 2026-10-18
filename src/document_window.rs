//! A document window: the documents' tabs and the bars around them.
//!
//! The tabs ([`DocumentTabs`]) hold the document views and show one at a
//! time. The window around them has the window's title and edited marker
//! (the active tab's), the bar for problems in the settings file,
//! printing, and the actions for zoom and closing the window. It hears
//! what the tabs and documents don't handle themselves through
//! [`TabsEvent`]s.

use std::collections::HashMap;
use std::path::Path;

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext, Context, Entity, EntityId, Focusable, KeyBinding,
    Render, Subscription, Window, WindowHandle, actions, div, prelude::*, px, relative, rems,
};

use crate::app_settings::{self, AppSettings};
use crate::builtin::Builtin;
use crate::{windows, zoom};
use kertas_editor::Theme;
use kertas_editor::tabs::{DocumentTabs, TabsEvent};
use kertas_editor::{DocumentEvent, DocumentView, problems_bar};

actions!(kertas, [New, Open, Print, CloseWindow, Quit]);

/// Key bindings for document windows (macOS conventions). The tabs' keys,
/// including ⌘W, are [`kertas_editor::tabs::key_bindings`].
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-n", New, None),
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("cmd-p", Print, None),
        KeyBinding::new("cmd-shift-w", CloseWindow, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]
}

pub struct DocumentWindow {
    tabs: Entity<DocumentTabs>,
    /// The tabs showing Kertas's built-in documents, by document view.
    builtins: HashMap<EntityId, Builtin>,
    /// Whether the user closed the bar reporting problems in the settings
    /// file. It shows again when the file changes.
    settings_problems_dismissed: bool,
    _subscriptions: [Subscription; 3],
}

impl DocumentWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Closing the window (e.g. its close button) asks about unsaved
        // changes first.
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |this, cx| this.should_close(window, cx))
                .unwrap_or(true)
        });
        let tabs = cx.new(|_| DocumentTabs::new());
        let subscriptions = [
            cx.subscribe_in(&tabs, window, Self::tabs_event),
            cx.observe_global_in::<AppSettings>(window, |this, _, cx| {
                this.settings_problems_dismissed = false;
                cx.notify();
            }),
            cx.observe_window_activation(window, |_, window, cx| {
                if window.is_window_active() {
                    // The default Markdown app can change in Finder.
                    crate::menus::refresh(cx);
                }
            }),
        ];
        Self {
            tabs,
            builtins: HashMap::new(),
            settings_problems_dismissed: false,
            _subscriptions: subscriptions,
        }
    }

    /// The window's tabs.
    pub fn tabs(&self) -> &Entity<DocumentTabs> {
        &self.tabs
    }

    /// Adds a tab for a new document view, makes it the active tab, and
    /// returns its view.
    pub fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<DocumentView> {
        let config = app_settings::editor_config(cx);
        let view = cx.new(|cx| DocumentView::new(config, window, cx));
        self.tabs
            .update(cx, |tabs, cx| tabs.add(view.clone(), window, cx));
        view
    }

    /// Shows one of Kertas's built-in documents, view-only, in `view`.
    pub fn open_builtin(
        &mut self,
        view: &Entity<DocumentView>,
        builtin: Builtin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.builtins.insert(view.entity_id(), builtin);
        view.update(cx, |view, cx| {
            view.show_text(&builtin.text(), builtin.title(), window, cx)
        });
    }

    /// The tab showing `builtin`, if any, by its position.
    pub fn builtin_tab(&self, builtin: Builtin, cx: &App) -> Option<usize> {
        let tabs = self.tabs.read(cx);
        tabs.views()
            .position(|view| self.builtins.get(&view.entity_id()) == Some(&builtin))
    }

    /// The built-in document in the active tab, if any.
    #[cfg(debug_assertions)]
    pub fn builtin(&self, cx: &App) -> Option<Builtin> {
        let view = self.tabs.read(cx).active_view()?;
        self.builtins.get(&view.entity_id()).copied()
    }

    /// Whether the bar reporting problems in the settings file is showing,
    /// for the smoke test.
    #[cfg(debug_assertions)]
    pub fn settings_problems_shown(&self) -> bool {
        !self.settings_problems_dismissed
    }

    fn tabs_event(
        &mut self,
        _: &Entity<DocumentTabs>,
        event: &TabsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TabsEvent::ActiveChanged | TabsEvent::Reordered => self.apply_title(window, cx),
            TabsEvent::Closed(view) => {
                self.builtins.remove(&view.entity_id());
                if self.tabs.read(cx).is_empty() {
                    window.remove_window();
                    return;
                }
                self.apply_title(window, cx);
                // A file that shared its name with the closed one may not
                // need its folder any more.
                cx.defer(windows::refresh_titles);
            }
            // Every other tab's title too, since they may share a name
            // (#95).
            TabsEvent::Document(_, DocumentEvent::TitleChanged) => {
                self.apply_title(window, cx);
                cx.defer(windows::refresh_titles);
            }
            TabsEvent::Document(_, DocumentEvent::DirtyChanged(_)) => {
                self.apply_title(window, cx);
                cx.notify();
            }
            // Kertas shows nothing that follows the text.
            TabsEvent::Document(_, DocumentEvent::Edited) => {}
            TabsEvent::Document(_, DocumentEvent::OpenFile { path, anchor }) => {
                let (path, anchor) = (path.clone(), anchor.clone());
                cx.defer(move |cx| windows::open_path_at(path, anchor, cx));
            }
        }
    }

    /// Gives the window the active tab's title, file and unsaved changes.
    pub fn apply_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = self.tabs.read(cx);
        let Some(view) = tabs.active_view() else {
            return;
        };
        let view = view.read(cx);
        if let Some(title) = tabs.title(tabs.active_index(), cx) {
            window.set_window_title(&title);
        }
        window.set_document_path(view.document_path());
        // Show unsaved changes in the window's close button.
        window.set_window_edited(view.is_dirty());
    }

    /// Whether the window may close now. With unsaved changes, asks about
    /// each tab in turn and closes the window itself once they're all
    /// resolved.
    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let tabs = self.tabs.clone();
        if tabs.update(cx, |tabs, cx| tabs.ready_to_close(window, cx)) {
            return true;
        }
        let confirmed = tabs.update(cx, |tabs, cx| tabs.confirm_close(window, cx));
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
            let Some(view) = self.tabs.read(cx).active_view() else {
                return;
            };
            let view = view.read(cx);
            let Some(model) = view.model() else {
                return;
            };
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                return;
            };
            let Some(ns_window) = crate::ns_window::ns_window(window) else {
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
        let message = app_settings::problems_message(cx)?;
        Some(
            problems_bar(
                theme,
                "settings-problems",
                message,
                "Dismiss settings problems",
                cx.listener(|this, _, window, cx| {
                    this.settings_problems_dismissed = true;
                    // Focus would otherwise be left on nothing.
                    this.tabs.focus_handle(cx).focus(window, cx);
                    cx.notify();
                }),
            )
            .into_any_element(),
        )
    }
}

impl Render for DocumentWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (theme, size) = match self.tabs.read(cx).active_view() {
            Some(view) => {
                let view = view.read(cx);
                (view.theme(), view.config().font_size)
            }
            None => {
                let config = app_settings::editor_config(cx);
                (
                    Theme::for_window(&config.theme, config.appearance, window),
                    config.font_size,
                )
            }
        };
        // The bars are sized in rems too, so they zoom with the text.
        window.set_rem_size(px(size as f32));
        let problems_bar = self.render_settings_problems(theme, cx);
        // Zoom In, Zoom Out and Reset Zoom, while each is possible.
        zoom::handle_actions(div(), cx)
            .size_full()
            .bg(theme.background)
            .text_color(theme.text)
            .text_size(rems(1.))
            .line_height(relative(1.6))
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::print))
            .on_action(cx.listener(Self::close_window))
            .child(div().flex_1().min_h(px(0.)).child(self.tabs.clone()))
            .children(problems_bar)
    }
}

/// A document window's handle, which reaches the active tab's document
/// view as a window handle reaches its root view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentWindowHandle(pub WindowHandle<DocumentWindow>);

impl DocumentWindowHandle {
    /// `window`'s handle, if it's a document window.
    pub fn downcast(window: AnyWindowHandle) -> Option<Self> {
        window.downcast::<DocumentWindow>().map(Self)
    }

    /// Updates the active tab's document view.
    pub fn update<C: AppContext, R>(
        &self,
        cx: &mut C,
        update: impl FnOnce(&mut DocumentView, &mut Window, &mut Context<DocumentView>) -> R,
    ) -> gpui::Result<R> {
        self.0.update(cx, |this, window, cx| {
            let view = this.active_view(cx)?;
            Ok(view.update(cx, |view, cx| update(view, window, cx)))
        })?
    }

    /// The active tab's document view.
    pub fn read<'a>(&self, cx: &'a App) -> gpui::Result<&'a DocumentView> {
        let view = self.0.read(cx)?.tabs.read(cx).active_view();
        Ok(view.ok_or_else(no_tabs)?.read(cx))
    }

    /// Reads the active tab's document view, with a callback.
    pub fn read_with<C: AppContext, R>(
        &self,
        cx: &C,
        read_with: impl FnOnce(&DocumentView, &App) -> R,
    ) -> gpui::Result<R> {
        self.0.read_with(cx, |this, cx| {
            let view = this.active_view(cx)?;
            Ok(read_with(view.read(cx), cx))
        })?
    }

    /// Every tab's document view, in tab order.
    pub fn views(&self, cx: &App) -> Vec<Entity<DocumentView>> {
        self.0.read(cx).map_or_else(
            |_| Vec::new(),
            |this| this.tabs.read(cx).views().cloned().collect(),
        )
    }

    /// The document's file, if it has one.
    pub fn document_path<'a>(&self, cx: &'a App) -> Option<&'a Path> {
        self.read(cx).ok()?.document_path()
    }
}

impl DocumentWindow {
    fn active_view(&self, cx: &App) -> gpui::Result<Entity<DocumentView>> {
        self.tabs
            .read(cx)
            .active_view()
            .cloned()
            .ok_or_else(no_tabs)
    }
}

fn no_tabs() -> anyhow::Error {
    anyhow::anyhow!("the window has no tabs")
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
