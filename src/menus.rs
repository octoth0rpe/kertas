//! The menu bar.
//!
//! The View menu's choices are settings, so the menus are rebuilt whenever
//! the settings change, to keep their checkmarks current.

use gpui::{App, Menu, MenuItem, actions};

use crate::app_settings::AppSettings;
use crate::builtin::{self, Builtin};
use crate::document_window::{CloseWindow, New, Open, Print, Quit};
use crate::windows;
use kertas_core::settings::Layout;
use kertas_editor::actions::{Find, FindNext, FindPrevious, Save, SaveAs, UseSelectionForFind};
use kertas_editor::settings_window::OpenSettings;
use kertas_editor::tabs::CloseTab;
use kertas_editor::view_menu;

actions!(
    kertas,
    [
        OpenHelp,
        OpenMarkdownSyntax,
        OpenKeyboardShortcuts,
        OpenThemes,
        OpenReleaseNotes,
        OpenLicenses,
        OpenAbout,
    ]
);

/// Sets up the menu bar and its app-wide actions.
pub fn init(cx: &mut App) {
    view_menu::init(cx);
    cx.on_action(|_: &OpenAbout, cx| cx.defer(|cx| builtin::open(Builtin::About, cx)));
    cx.on_action(|_: &OpenHelp, cx| cx.defer(|cx| builtin::open(Builtin::Help, cx)));
    cx.on_action(|_: &OpenLicenses, cx| cx.defer(|cx| builtin::open(Builtin::Licenses, cx)));
    cx.on_action(|_: &OpenMarkdownSyntax, cx| {
        cx.defer(|cx| builtin::open(Builtin::MarkdownSyntax, cx))
    });
    cx.on_action(|_: &OpenKeyboardShortcuts, cx| {
        cx.defer(|cx| builtin::open(Builtin::KeyboardShortcuts, cx))
    });
    cx.on_action(|_: &OpenThemes, cx| cx.defer(|cx| builtin::open(Builtin::Themes, cx)));
    cx.on_action(|_: &OpenReleaseNotes, cx| {
        cx.defer(|cx| builtin::open(Builtin::ReleaseNotes, cx))
    });

    refresh(cx);
    cx.observe_global::<AppSettings>(refresh).detach();
}

/// Rebuilds the menus, to show the current settings and system state.
pub fn refresh(cx: &mut App) {
    let menus = menus(cx);
    cx.set_menus(menus);
}

fn menus(cx: &App) -> Vec<Menu> {
    let mut kertas = vec![
        MenuItem::action("About Kertas", OpenAbout),
        MenuItem::separator(),
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::separator(),
    ];
    #[cfg(target_os = "macos")]
    {
        kertas.extend(crate::system_actions::menu_items());
        kertas.push(MenuItem::separator());
    }
    kertas.push(MenuItem::action("Quit Kertas", Quit));
    vec![
        Menu::new("Kertas").items(kertas),
        Menu::new("File").items(
            [
                MenuItem::action("New", New),
                MenuItem::action("Open…", Open),
                MenuItem::separator(),
                MenuItem::action("Save", Save),
                MenuItem::action("Save As…", SaveAs),
                MenuItem::separator(),
                MenuItem::action("Print…", Print),
                MenuItem::separator(),
            ]
            .into_iter()
            .chain(close_items(cx)),
        ),
        Menu::new("Edit").items([MenuItem::submenu(Menu::new("Find").items([
            MenuItem::action("Find…", Find),
            MenuItem::action("Find Next", FindNext),
            MenuItem::action("Find Previous", FindPrevious),
            MenuItem::action("Use Selection for Find", UseSelectionForFind),
        ]))]),
        Menu::new("View").items(view_menu::items(cx)),
        Menu::new("Help").items([
            MenuItem::action("Kertas Help", OpenHelp),
            MenuItem::action("Markdown Syntax", OpenMarkdownSyntax),
            MenuItem::action("Keyboard Shortcuts", OpenKeyboardShortcuts),
            MenuItem::action("Themes", OpenThemes),
            MenuItem::separator(),
            MenuItem::action("Release Notes", OpenReleaseNotes),
            MenuItem::action("Licenses", OpenLicenses),
        ]),
    ]
}

/// Close Tab and Close Window, or just Close when each document has its
/// own window.
fn close_items(cx: &App) -> Vec<MenuItem> {
    match windows::launch_layout(cx) {
        Layout::Tabs => vec![
            MenuItem::action("Close Tab", CloseTab),
            MenuItem::action("Close Window", CloseWindow),
        ],
        Layout::Windows => vec![MenuItem::action("Close", CloseTab)],
    }
}
