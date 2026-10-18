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

/// `path` with the home folder written as `~`.
fn home_relative(path: &Path) -> String {
    match std::env::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_owned)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}
