//! The settings file on disk (REQUIREMENTS §8.1), loaded into a GPUI
//! global and kept up to date.
//!
//! The file is `~/Library/Application Support/kami/config.toml` (G1), or
//! `config.toml` in `$KAMI_CONFIG_DIR` if that's set. It's checked for
//! changes once a second, so edits made in any editor apply live (G3).
//! Views observe [`AppSettings`] to update.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gpui::{App, Global, WindowAppearance};
use kami::settings::{self, Problem, Settings, ThemeSetting};

/// How often to check the settings file for changes.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

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

/// The current settings.
pub fn get(cx: &App) -> Settings {
    cx.try_global::<AppSettings>()
        .map(|state| state.settings)
        .unwrap_or_default()
}

/// Loads the settings and starts watching the file for changes.
pub fn init(cx: &mut App) {
    let path = settings_path();
    let state = load(path);
    apply_appearance(state.settings, cx);
    cx.set_global(state);

    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
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

/// Changes setting `key` to `value` in the settings file, creating it if
/// needed, and applies it.
#[cfg_attr(
    not(debug_assertions),
    expect(dead_code, reason = "used by the View menu (#16)")
)]
pub fn set(key: &str, value: &str, cx: &mut App) -> Result<(), Problem> {
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
