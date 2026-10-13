//! The Kertas menu's one-time system actions: making Kertas the default app
//! for Markdown files (#23) and installing the `kertas` command (#27).
//! Each reports what happened, or why it couldn't, in an alert.

use gpui::{App, AppContext, MenuItem, PromptLevel, actions};

use crate::cli_tool;
use crate::default_app;
use crate::document_view::DocumentView;
use crate::menus;

actions!(
    kertas,
    [
        MakeDefaultMarkdownApp,
        InstallCommandLineTool,
        UninstallCommandLineTool
    ]
);

/// Handles the actions.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &MakeDefaultMarkdownApp, cx| cx.defer(make_default));
    cx.on_action(|_: &InstallCommandLineTool, cx| {
        cx.defer(|cx| change_command(cli_tool::install, true, cx))
    });
    cx.on_action(|_: &UninstallCommandLineTool, cx| {
        cx.defer(|cx| change_command(cli_tool::uninstall, false, cx))
    });
}

/// The Kertas menu's items for the actions, showing the current state.
pub fn menu_items() -> Vec<MenuItem> {
    let default = MenuItem::action(
        "Make Kertas the Default Markdown App",
        MakeDefaultMarkdownApp,
    )
    .checked(default_app::status() == default_app::Status::Kertas);
    let command = match cli_tool::status() {
        cli_tool::Status::Installed => {
            MenuItem::action("Uninstall Command Line Tool", UninstallCommandLineTool)
        }
        _ => MenuItem::action("Install Command Line Tool…", InstallCommandLineTool),
    };
    vec![default, command]
}

fn make_default(cx: &mut App) {
    match default_app::status() {
        default_app::Status::Kertas => {}
        default_app::Status::NotInstalled => alert(
            "Kertas can't be the default Markdown app",
            "Kertas isn't running as Kertas.app. Install Kertas.app, then try again.",
            cx,
        ),
        default_app::Status::Other(_) => {
            let result = default_app::make_default();
            cx.spawn(async move |cx| {
                // macOS asks the user first, and shows nothing more.
                let result = result.await.unwrap_or(Ok(()));
                cx.update(|cx| {
                    if let Err(error) = result {
                        alert(
                            "Kertas couldn't become the default Markdown app",
                            &capitalize(&error),
                            cx,
                        );
                    }
                    menus::refresh(cx);
                });
            })
            .detach();
        }
    }
}

/// Installs or uninstalls the command off the main thread, since macOS
/// may ask for a password, then says how it went.
fn change_command(change: fn() -> Result<(), String>, installing: bool, cx: &mut App) {
    let problem = match cli_tool::status() {
        cli_tool::Status::NotInstalledApp => Some(
            "Kertas isn't running as Kertas.app, so there's no command to install. \
             Install Kertas.app, then try again."
                .to_owned(),
        ),
        cli_tool::Status::Taken => Some(format!(
            "{} belongs to another program. Remove it, then try again.",
            cli_tool::LINK
        )),
        _ => None,
    };
    if let Some(problem) = problem {
        alert("Couldn't install the kertas command", &problem, cx);
        return;
    }
    let task = cx.background_spawn(async move { change() });
    cx.spawn(async move |cx| {
        let result = task.await;
        cx.update(|cx| {
            menus::refresh(cx);
            match (result, installing) {
                (Ok(()), true) => alert(
                    "The kertas command is installed",
                    &format!(
                        "It's at {}. In Terminal, `kertas notes.md` opens a file in Kertas.",
                        cli_tool::LINK
                    ),
                    cx,
                ),
                (Ok(()), false) => alert(
                    "The kertas command is uninstalled",
                    &format!("{} has been removed.", cli_tool::LINK),
                    cx,
                ),
                // Cancelling the password prompt isn't an error.
                (Err(error), _) if error.contains("(-128)") => {}
                (Err(error), true) => alert(
                    "Couldn't install the kertas command",
                    &capitalize(&error),
                    cx,
                ),
                (Err(error), false) => alert(
                    "Couldn't uninstall the kertas command",
                    &capitalize(&error),
                    cx,
                ),
            }
        });
    })
    .detach();
}

/// Shows an alert on the front document window.
fn alert(message: &str, detail: &str, cx: &mut App) {
    let window = cx
        .active_window()
        .into_iter()
        .chain(cx.window_stack().unwrap_or_default())
        .find_map(|window| window.downcast::<DocumentView>());
    let Some(window) = window else {
        return;
    };
    window
        .update(cx, |_, window, cx| {
            let answer = window.prompt(PromptLevel::Info, message, Some(detail), &["OK"], cx);
            cx.spawn(async move |_, _| answer.await.ok()).detach();
        })
        .ok();
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
