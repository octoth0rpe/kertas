//! The settings file on disk (REQUIREMENTS §8.1), loaded into a GPUI
//! global and kept up to date.
//!
//! The file is `~/Library/Application Support/kami/config.toml` (G1), or
//! `config.toml` in `$KAMI_CONFIG_DIR` if that's set. macOS reports
//! changes to it, so edits made in any editor apply live (G3). Views
//! observe [`AppSettings`] to update.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use futures::StreamExt;

use gpui::{App, BorrowAppContext, Global, WindowAppearance};
use kami::settings::{self, Layout, Problem, Settings, ThemeSetting};

use crate::file_watch;

/// The current settings, and any problems reading them.
pub struct AppSettings {
    pub settings: Settings,
    pub problems: Vec<Problem>,
    path: PathBuf,
    /// The file's modification time and size when last read, to notice
    /// changes.
    stamp: Option<(SystemTime, u64)>,
}

impl Global for AppSettings {}

/// The `layout` setting when Kami started. Changing it takes effect when
/// Kami restarts, so documents already open never have to be rearranged.
struct LaunchLayout(Layout);

impl Global for LaunchLayout {}

/// How documents are arranged in this session: the `layout` setting when
/// Kami started.
pub fn launch_layout(cx: &App) -> Layout {
    cx.try_global::<LaunchLayout>()
        .map(|launch| launch.0)
        .unwrap_or_default()
}

/// The current settings.
pub fn get(cx: &App) -> Settings {
    cx.try_global::<AppSettings>()
        .map(|state| state.settings)
        .unwrap_or_default()
}

/// Loads the settings and starts watching the file for changes.
pub fn init(cx: &mut App) {
    let path = settings_path();
    let state = load(path.clone());
    apply_appearance(state.settings, cx);
    cx.set_global(LaunchLayout(state.settings.layout));
    cx.set_global(state);

    // Watching needs the folder to exist, though the file is only created
    // when a setting is first changed.
    if let Some(directory) = path.parent()
        && fs::create_dir_all(directory).is_ok()
    {
        let mut changes = file_watch::watch(path);
        cx.spawn(async move |cx| {
            while changes.next().await.is_some() {
                cx.update(|cx| {
                    let state = cx.global::<AppSettings>();
                    if stamp(&state.path) != state.stamp {
                        reload(cx);
                    }
                });
            }
        })
        .detach();
    }
}

/// Changes a setting, like [`set`], reporting any problem in the bar
/// that shows settings file problems.
pub fn set_or_report(key: &str, value: impl Into<toml_edit::Value>, cx: &mut App) {
    if let Err(problem) = set(key, value, cx) {
        cx.update_global::<AppSettings, _>(|state, _| state.problems.insert(0, problem));
    }
}

/// Changes setting `key` to `value` in the settings file, creating it if
/// needed, and applies it.
pub fn set(key: &str, value: impl Into<toml_edit::Value>, cx: &mut App) -> Result<(), Problem> {
    let path = cx.global::<AppSettings>().path.clone();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(file_problem(&path, &err)),
    };
    let text = settings::set(&text, key, value)?;
    write_atomically(&path, &text).map_err(|err| file_problem(&path, &err))?;
    reload(cx);
    Ok(())
}

/// Re-reads the settings file and applies it.
fn reload(cx: &mut App) {
    let path = cx.global::<AppSettings>().path.clone();
    let state = load(path);
    apply_appearance(state.settings, cx);
    cx.set_global(state);
}

/// Makes the window chrome, dialogs and theme follow the theme setting.
fn apply_appearance(settings: Settings, cx: &mut App) {
    cx.set_window_appearance(match settings.theme {
        ThemeSetting::Light => Some(WindowAppearance::Light),
        ThemeSetting::Dark => Some(WindowAppearance::Dark),
        ThemeSetting::System => None,
    });
}

fn settings_path() -> PathBuf {
    let directory = match std::env::var_os("KAMI_CONFIG_DIR") {
        Some(directory) => PathBuf::from(directory),
        None => std::env::home_dir()
            .unwrap_or_default()
            .join("Library/Application Support/kami"),
    };
    directory.join("config.toml")
}

fn load(path: PathBuf) -> AppSettings {
    let stamp = stamp(&path);
    let (settings, problems) = match fs::read_to_string(&path) {
        Ok(text) => settings::parse(&text),
        // No file means all defaults (G2).
        Err(err) if err.kind() == io::ErrorKind::NotFound => (Settings::default(), vec![]),
        Err(err) => (Settings::default(), vec![file_problem(&path, &err)]),
    };
    AppSettings {
        settings,
        problems,
        path,
        stamp,
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
    let temp = path.with_extension("toml.kami-save");
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
