//! A GPUI view that shows and edits a Markdown document, as the Kertas
//! app does, for an app to put in its own windows.
//!
//! Every block is rendered except the one holding the caret, which is shown
//! as styled Markdown source and edited in place.
//!
//! An app creates a [`DocumentView`] with an [`EditorConfig`], and
//! gives it a new one when its settings change. It opens a file with
//! [`DocumentView::open_path`], text with [`DocumentView::show_text`], or
//! starts an untitled document with [`DocumentView::start_untitled`]. The
//! view saves itself on its [`actions::Save`] and [`actions::SaveAs`]
//! actions, and reloads its file when another app changes it. It tells
//! the app what it doesn't handle itself with [`DocumentEvent`]s, among
//! them [`DocumentEvent::Edited`] once typing pauses, when the app can
//! read the view's [`text`](DocumentView::text),
//! [`headings`](DocumentView::headings) and
//! [`word count`](DocumentView::word_count).
//!
//! The app binds the view's keys with [`key_bindings`] and loads its
//! icons with [`Assets`]. `examples/embed.rs` shows a window that holds
//! one document view and nothing else.
//!
//! This crate re-exports [`gpui`] and [`gpui_platform`], so an app uses
//! the same GPUI as the view, and [`kertas_core`], whose settings and
//! themes an [`EditorConfig`] is made from.
//!
//! An app can also share Kertas's settings, from its own settings file
//! ([`app_settings`]), and with them zooming ([`zoom`]), the View menu
//! ([`view_menu`]), the settings window ([`settings_window`]), the bar that
//! reports problems ([`problems_bar`]), and help texts ([`help`]).
//!
//! [`tabs::DocumentTabs`] holds several document views in one window, as
//! tabs.
//!
//! A document view has a find bar (⌘F, ⌥⌘F to replace too). A host can
//! open it at a match with [`DocumentView::show_find_with`], choose which
//! of its options it offers with [`EditorConfig::find_options`], and
//! change a document as one undoable edit with
//! [`DocumentView::replace_ranges`], say to replace across several
//! documents. [`text_field::TextField`] is the bar's text field, for a
//! host's own windows too.
//!
//! On macOS, a host can print any document as Kertas does, with the
//! standard print panel ([`print::print()`]), or write it as a PDF without
//! one ([`print::save_pdf`]). Web images print once they've loaded, which
//! needs [`web_images::init`].

pub mod app_settings;
mod assets;
mod document_view;
mod editor_config;
pub mod file_watch;
pub mod help;
mod marks;
#[cfg(target_os = "macos")]
pub mod ns_window;
#[cfg(target_os = "macos")]
pub mod print;
mod problems_bar;
mod rendered_block;
mod revealed_block;
pub mod settings_window;
pub mod tabs;
pub mod text_field;
mod text_style;
mod theme;
pub mod view_menu;
#[cfg(target_os = "macos")]
pub mod web_images;
pub mod zoom;

pub use gpui;
pub use gpui_platform;
pub use kertas_core;

pub use assets::Assets;
pub use document_view::{
    CARET_BLINK_INTERVAL, DocumentEvent, DocumentView, EDIT_PAUSE, HOVER_DELAY, HeadingTarget,
    HoverPopover, HoverTarget, bar_button, key_bindings,
};
pub use editor_config::EditorConfig;
pub use marks::{Mark, MarkStyle};
pub use problems_bar::problems_bar;
pub use rendered_block::{LinkCheck, natural_size};
pub use text_style::TextSettings;
pub use theme::Theme;

/// The actions a document view handles: saving, moving the caret,
/// selecting, editing and finding.
pub mod actions {
    gpui::actions!(
        kertas,
        [
            Save,
            SaveAs,
            PageUp,
            PageDown,
            Unreveal,
            MoveLeft,
            MoveRight,
            MoveUp,
            MoveDown,
            MoveWordLeft,
            MoveWordRight,
            MoveLineStart,
            MoveLineEnd,
            MoveDocumentStart,
            MoveDocumentEnd,
            SelectLeft,
            SelectRight,
            SelectUp,
            SelectDown,
            SelectWordLeft,
            SelectWordRight,
            SelectLineStart,
            SelectLineEnd,
            SelectDocumentStart,
            SelectDocumentEnd,
            SelectAll,
            Backspace,
            Delete,
            DeleteWordLeft,
            DeleteWordRight,
            DeleteToLineStart,
            Newline,
            LineBreak,
            IndentTab,
            OutdentTab,
            IndentLines,
            OutdentLines,
            InsertTab,
            MoveBlockUp,
            MoveBlockDown,
            SelectBlockUp,
            SelectBlockDown,
            Cut,
            Copy,
            Paste,
            Undo,
            Redo,
            Find,
            FindNext,
            FindPrevious,
            UseSelectionForFind,
            HideFind,
            FindAndReplace,
            Replace,
            ReplaceAll,
        ]
    );
}
