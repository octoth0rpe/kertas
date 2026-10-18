//! Help texts a host app can show as they are or build on: Markdown
//! Syntax, Themes, and the editor's keyboard shortcuts. They're Markdown,
//! naming the app as the host app gives it.

use std::path::Path;

/// The Markdown Syntax document, for the app called `app`.
pub fn markdown_syntax(app: &str) -> String {
    include_str!("markdown.md").replace("{app}", app)
}

/// The Themes document, for the app called `app`, whose themes are in
/// `themes_folder`. Its example is the Kertas theme, renamed.
pub fn themes(app: &str, themes_folder: &Path) -> String {
    let example = kertas_core::themes::KERTAS_FILE.replacen(
        "# Kertas's own theme, after GitHub's light and dark colors.\nname = \"Kertas\"",
        "# My own theme, starting from Kertas's colors.\nname = \"My Theme\"",
        1,
    );
    include_str!("themes.md")
        .replace("{app}", app)
        .replace("{themes_folder}", &home_relative(themes_folder))
        .replace("{example}", &example)
        .trim_end()
        .to_owned()
}

/// The keyboard shortcuts for zooming, moving, selecting and editing in a
/// document, and for the settings window, as sections to follow the app's
/// own in its Keyboard Shortcuts document.
pub const EDITOR_SHORTCUTS: &str = include_str!("shortcuts.md");

/// How a keystroke is written in a Keyboard Shortcuts document, e.g.
/// `⇧⌘S`. Modifiers come in the macOS order: ⌃ ⌥ ⇧ ⌘. An app can use it
/// to check that its document lists every key binding.
pub fn shortcut_label(keystroke: &gpui::Keystroke) -> String {
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
        "pageup" => "Page Up".to_owned(),
        "pagedown" => "Page Down".to_owned(),
        "end" => "End".to_owned(),
        key => key.to_uppercase(),
    };
    label.push_str(&key);
    label
}

/// `path` with the home folder written as `~`.
fn home_relative(path: &Path) -> String {
    match std::env::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_owned)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}
