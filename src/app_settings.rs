//! The settings file on disk (REQUIREMENTS §8.1), loaded into a GPUI
//! global and kept up to date.
//!
//! The file is `~/Library/Application Support/kertas/config.toml` (G1), or
//! `config.toml` in `$KERTAS_CONFIG_DIR` if that's set. macOS reports
//! changes to it, so edits made in any editor apply live (G3). Views
//! observe [`AppSettings`] to update.
//!
//! The user's color themes are files in `themes/` next to it, watched the
//! same way.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gpui::{App, BorrowAppContext, Global, WindowAppearance};
use kertas_core::settings::{
    self, AppearanceSetting, Available, FontCheck, Layout, Problem, Settings,
};
use kertas_core::themes::{self, FileProblem, Theme};

use crate::file_watch;

/// The current settings and themes, and any problems reading them.
pub struct AppSettings {
    pub settings: Settings,
    pub problems: Vec<Problem>,
    /// The themes to choose from: the built-in ones and the user's.
    pub themes: Vec<Theme>,
    pub theme_problems: Vec<FileProblem>,
    path: PathBuf,
    /// The file's modification time and size when last read, to notice
    /// changes.
    stamp: Option<(SystemTime, u64)>,
}

impl Global for AppSettings {}

/// The `layout` setting when Kertas started. Changing it takes effect when
/// Kertas restarts, so documents already open never have to be rearranged.
struct LaunchLayout(Layout);

impl Global for LaunchLayout {}

/// How documents are arranged in this session: the `layout` setting when
/// Kertas started.
pub fn launch_layout(cx: &App) -> Layout {
    cx.try_global::<LaunchLayout>()
        .map(|launch| launch.0)
        .unwrap_or_default()
}

/// The current settings.
pub fn get(cx: &App) -> Settings {
    cx.try_global::<AppSettings>()
        .map(|state| state.settings.clone())
        .unwrap_or_default()
}

/// The themes to choose from: the built-in ones and the user's.
pub fn themes(cx: &App) -> &[Theme] {
    cx.try_global::<AppSettings>()
        .map_or(themes::builtin(), |state| state.themes.as_slice())
}

/// The installed font families, sorted, for the font settings.
struct InstalledFonts(Vec<String>);

impl Global for InstalledFonts {}

/// The installed font families, sorted, without the system's hidden ones.
pub fn installed_fonts(cx: &App) -> &[String] {
    cx.try_global::<InstalledFonts>()
        .map_or(&[], |fonts| fonts.0.as_slice())
}

/// Loads the settings and starts watching the file for changes.
pub fn init(cx: &mut App) {
    let mut fonts = cx.text_system().all_font_names();
    fonts.retain(|name| !name.starts_with('.'));
    cx.set_global(InstalledFonts(fonts));
    let path = settings_path();
    let state = load(path.clone(), cx);
    apply_appearance(&state.settings, cx);
    cx.set_global(LaunchLayout(state.settings.layout));
    cx.set_global(state);

    // Watching needs the folder to exist, though the file is only created
    // when a setting is first changed.
    if let Some(directory) = path.parent()
        && fs::create_dir_all(directory).is_ok()
    {
        let mut changes = file_watch::watch(path);
        cx.spawn(async move |cx| {
            while changes.changed().await {
                cx.update(|cx| {
                    let state = cx.global::<AppSettings>();
                    if stamp(&state.path) != state.stamp {
                        reload(cx);
                    }
                });
            }
        })
        .detach();
        let mut changes = file_watch::watch_folder(themes_folder());
        cx.spawn(async move |cx| {
            while changes.changed().await {
                cx.update(reload);
            }
        })
        .detach();
    }
}

/// Opens the folder of the user's themes in the Finder, creating it if
/// needed.
pub fn open_themes_folder(cx: &mut App) {
    let folder = themes_folder();
    match fs::create_dir_all(&folder) {
        Ok(()) => cx.open_with_system(&folder),
        Err(err) => cx.update_global::<AppSettings, _>(|state, _| {
            state.problems.insert(0, file_problem(&folder, &err))
        }),
    }
}

/// Changes a setting, like [`set`], reporting any problem in the bar
/// that shows settings file problems.
pub fn set_or_report(key: &str, value: impl Into<toml_edit::Value>, cx: &mut App) {
    if let Err(problem) = set(key, value, cx) {
        cx.update_global::<AppSettings, _>(|state, _| state.problems.insert(0, problem));
    }
}

/// Removes setting `key` from the settings file, so it takes its default,
/// reporting any problem in the bar that shows settings file problems.
pub fn remove_or_report(key: &str, cx: &mut App) {
    if let Err(problem) = edit(|text| settings::remove(text, key), cx) {
        cx.update_global::<AppSettings, _>(|state, _| state.problems.insert(0, problem));
    }
}

/// Changes setting `key` to `value` in the settings file, creating it if
/// needed, and applies it.
pub fn set(key: &str, value: impl Into<toml_edit::Value>, cx: &mut App) -> Result<(), Problem> {
    let value = value.into();
    edit(|text| settings::set(text, key, value), cx)
}

/// Rewrites the settings file with `change`, creating it if needed, and
/// applies it.
fn edit(change: impl FnOnce(&str) -> Result<String, Problem>, cx: &mut App) -> Result<(), Problem> {
    let path = cx.global::<AppSettings>().path.clone();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(file_problem(&path, &err)),
    };
    let changed = change(&text)?;
    if changed != text {
        write_atomically(&path, &changed).map_err(|err| file_problem(&path, &err))?;
    }
    reload(cx);
    Ok(())
}

/// Re-reads the settings file and applies it.
fn reload(cx: &mut App) {
    let path = cx.global::<AppSettings>().path.clone();
    let state = load(path, cx);
    apply_appearance(&state.settings, cx);
    cx.set_global(state);
}

/// Makes the window chrome, dialogs and theme follow the appearance
/// setting.
fn apply_appearance(settings: &Settings, cx: &mut App) {
    cx.set_window_appearance(match settings.appearance {
        AppearanceSetting::Light => Some(WindowAppearance::Light),
        AppearanceSetting::Dark => Some(WindowAppearance::Dark),
        AppearanceSetting::System => None,
    });
}

fn config_folder() -> PathBuf {
    match std::env::var_os("KERTAS_CONFIG_DIR") {
        Some(directory) => PathBuf::from(directory),
        None => std::env::home_dir()
            .unwrap_or_default()
            .join("Library/Application Support/kertas"),
    }
}

fn settings_path() -> PathBuf {
    config_folder().join("config.toml")
}

/// The folder of the user's theme files.
fn themes_folder() -> PathBuf {
    config_folder().join("themes")
}

fn load(path: PathBuf, cx: &App) -> AppSettings {
    let stamp = stamp(&path);
    let fonts = installed_fonts(cx);
    let installed = |name: &str| {
        fonts
            .iter()
            .find(|font| font.eq_ignore_ascii_case(name))
            .cloned()
    };
    let (user, theme_problems) = themes::read_folder(&themes_folder());
    let themes = themes::combine(themes::builtin(), user);
    let (settings, problems) = read(&path, &installed, &themes);
    AppSettings {
        settings,
        problems,
        themes,
        theme_problems,
        path,
        stamp,
    }
}

/// Reads the settings file once, outside of GPUI, for `--convert`.
/// Problems are ignored: the defaults stand in, as they do in the app.
#[cfg(target_os = "macos")]
pub fn read_once(installed: &FontCheck) -> Settings {
    let user = themes::read_folder(&themes_folder()).0;
    let themes = themes::combine(themes::builtin(), user);
    read(&settings_path(), installed, &themes).0
}

fn read(path: &Path, installed: &FontCheck, themes: &[Theme]) -> (Settings, Vec<Problem>) {
    let available = Available {
        fonts: installed,
        themes: &|name| themes::find(themes, name),
    };
    match fs::read_to_string(path) {
        Ok(text) => settings::parse_checking(&text, &available),
        // No file means all defaults (G2).
        Err(err) if err.kind() == io::ErrorKind::NotFound => (Settings::default(), vec![]),
        Err(err) => (Settings::default(), vec![file_problem(path, &err)]),
    }
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

fn file_problem(path: &Path, err: &io::Error) -> Problem {
    Problem {
        line: None,
        message: format!("couldn't read or write {}: {err}", path.display()),
    }
}

/// Writes `text` to a temporary file next to `path`, then renames it over
/// `path`, so a failed write never leaves a half-written file.
fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let temp = path.with_extension("toml.kertas-save");
    let result = (|| {
        let mut file = fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        fs::remove_file(&temp).ok();
    }
    result
}
