//! Kami's built-in documents: Kami Help and Keyboard Shortcuts.
//!
//! They open as tabs, but they're Kami's own content, so they're
//! view-only: no caret, no editing, nothing to save.

use gpui::{App, Keystroke, WindowHandle};

use crate::document_view::DocumentView;
use crate::windows;

/// A built-in document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Help,
    KeyboardShortcuts,
}

impl Builtin {
    pub fn title(self) -> &'static str {
        match self {
            Self::Help => "Kami Help",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Help => include_str!("builtin/help.md"),
            Self::KeyboardShortcuts => include_str!("builtin/shortcuts.md"),
        }
    }
}

/// Opens `builtin` in a new tab, or switches to it if it's open already.
pub fn open(builtin: Builtin, cx: &mut App) {
    let existing = cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<DocumentView>())
        .find(|window: &WindowHandle<DocumentView>| {
            window
                .read(cx)
                .is_ok_and(|view| view.builtin() == Some(builtin))
        });
    match existing {
        Some(window) => {
            window
                .update(cx, |_, window, _| window.activate_window())
                .ok();
        }
        None => {
            windows::open_window(cx, |view, window, cx| {
                view.open_builtin(builtin, window, cx)
            });
        }
    }
}

/// How a keystroke is written in the Keyboard Shortcuts document, e.g.
/// `⇧⌘S`. Modifiers come in the macOS order: ⌃ ⌥ ⇧ ⌘.
#[cfg_attr(not(test), expect(dead_code, reason = "used by tests"))]
pub fn shortcut_label(keystroke: &Keystroke) -> String {
    let modifiers = &keystroke.modifiers;
    let mut label = String::new();
    for (held, symbol) in [
        (modifiers.control, "⌃"),
        (modifiers.alt, "⌥"),
        (modifiers.shift, "⇧"),
        (modifiers.platform, "⌘"),
    ] {
        if held {
            label.push_str(symbol);
        }
    }
    let key = match keystroke.key.as_str() {
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "enter" => "↩".to_owned(),
        "tab" => "⇥".to_owned(),
        "backspace" => "⌫".to_owned(),
        "delete" => "⌦".to_owned(),
        "escape" => "Esc".to_owned(),
        "home" => "Home".to_owned(),
        "end" => "End".to_owned(),
        key => key.to_uppercase(),
    };
    label.push_str(&key);
    label
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_view::key_bindings;

    #[test]
    fn every_shortcut_is_documented() {
        let document = Builtin::KeyboardShortcuts.text();
        let mut missing = Vec::new();
        for binding in key_bindings() {
            for keystroke in binding.keystrokes() {
                let label = shortcut_label(keystroke.inner());
                // Tabs 2 to 8 are written as a range, "⌘1 … ⌘9".
                let in_tab_range = matches!(
                    label.as_str(),
                    "⌘2" | "⌘3" | "⌘4" | "⌘5" | "⌘6" | "⌘7" | "⌘8"
                );
                if !in_tab_range && !document.contains(&label) {
                    missing.push(label);
                }
            }
        }
        assert!(missing.is_empty(), "not in shortcuts.md: {missing:?}");
    }

    #[test]
    fn labels() {
        let label = |text: &str| shortcut_label(&Keystroke::parse(text).unwrap());
        assert_eq!(label("cmd-shift-s"), "⇧⌘S");
        assert_eq!(label("ctrl-shift-tab"), "⌃⇧⇥");
        assert_eq!(label("alt-shift-left"), "⌥⇧←");
        assert_eq!(label("cmd-]"), "⌘]");
    }
}
