//! The menu bar.
//!
//! The View menu's choices are settings, so the menus are rebuilt whenever
//! the settings change, to keep their checkmarks current.

use gpui::{Action, App, Menu, MenuItem, NoAction, SharedString, actions};
use kertas_core::settings::{AppearanceSetting, DEFAULT_THEME, Settings, TextWidth};
use kertas_core::themes::Theme;

use crate::app_settings::{self, AppSettings};
use crate::builtin::{self, Builtin};
use crate::document_view::{
    CloseWindow, New, Open, Print, Quit, ResetZoom, Save, SaveAs, ZoomIn, ZoomOut,
};
use crate::settings_window;

actions!(
    kertas,
    [
        UseLightAppearance,
        UseDarkAppearance,
        UseSystemAppearance,
        UseStandardWidth,
        UseWideWidth,
        UseFullWidth,
        ToggleScrollbar,
        OpenHelp,
        OpenMarkdownSyntax,
        OpenKeyboardShortcuts,
        OpenThemes,
        OpenReleaseNotes,
        OpenLicenses,
        OpenAbout,
        OpenSettings,
    ]
);

/// Uses the theme called `name`.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = kertas, no_json)]
pub struct UseTheme {
    pub name: SharedString,
}

/// Sets up the menu bar and its app-wide actions.
pub fn init(cx: &mut App) {
    // Menu actions arrive while a window is locked for the dispatch, and
    // changing a setting updates every window, so defer it.
    fn setting(key: &'static str, value: &'static str) -> impl Fn(&mut App) {
        move |cx| cx.defer(move |cx| app_settings::set_or_report(key, value, cx))
    }
    let appearance = |value: AppearanceSetting| setting("appearance", value.as_str());
    let width = |value: TextWidth| setting("text_width", value.as_str());
    let light = appearance(AppearanceSetting::Light);
    cx.on_action(move |_: &UseLightAppearance, cx| light(cx));
    let dark = appearance(AppearanceSetting::Dark);
    cx.on_action(move |_: &UseDarkAppearance, cx| dark(cx));
    let system = appearance(AppearanceSetting::System);
    cx.on_action(move |_: &UseSystemAppearance, cx| system(cx));
    cx.on_action(|action: &UseTheme, cx| {
        let name = action.name.to_string();
        cx.defer(move |cx| {
            // The default is set by not setting it, as in the settings
            // window.
            if name == DEFAULT_THEME {
                app_settings::remove_or_report("theme", cx);
            } else {
                app_settings::set_or_report("theme", name, cx);
            }
        })
    });
    let standard = width(TextWidth::Standard);
    cx.on_action(move |_: &UseStandardWidth, cx| standard(cx));
    let wide = width(TextWidth::Wide);
    cx.on_action(move |_: &UseWideWidth, cx| wide(cx));
    let full = width(TextWidth::Full);
    cx.on_action(move |_: &UseFullWidth, cx| full(cx));
    cx.on_action(|_: &ToggleScrollbar, cx| {
        cx.defer(|cx| {
            let show = !app_settings::get(cx).scrollbar;
            app_settings::set_or_report("scrollbar", show, cx);
        })
    });

    cx.on_action(|_: &OpenSettings, cx| cx.defer(settings_window::open));
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
    cx.set_menus(menus(app_settings::get(cx), app_settings::themes(cx)));
}

fn menus(settings: Settings, themes: &[Theme]) -> Vec<Menu> {
    let appearance = |value: AppearanceSetting| settings.appearance == value;
    let width = |value: TextWidth| settings.text_width == value;
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
        Menu::new("File").items([
            MenuItem::action("New", New),
            MenuItem::action("Open…", Open),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Save As…", SaveAs),
            MenuItem::separator(),
            MenuItem::action("Print…", Print),
            MenuItem::separator(),
            MenuItem::action("Close", CloseWindow),
        ]),
        Menu::new("View").items(
            [
                section("Appearance"),
                MenuItem::action("Light", UseLightAppearance)
                    .checked(appearance(AppearanceSetting::Light)),
                MenuItem::action("Dark", UseDarkAppearance)
                    .checked(appearance(AppearanceSetting::Dark)),
                MenuItem::action("Match System", UseSystemAppearance)
                    .checked(appearance(AppearanceSetting::System)),
                MenuItem::separator(),
                section("Theme"),
            ]
            .into_iter()
            .chain(themes.iter().map(|theme| {
                let name = SharedString::from(theme.name.clone());
                MenuItem::action(name.clone(), UseTheme { name })
                    .checked(theme.name == settings.theme)
            }))
            .chain([
                MenuItem::separator(),
                section("Text Width"),
                MenuItem::action("Standard", UseStandardWidth).checked(width(TextWidth::Standard)),
                MenuItem::action("Wide", UseWideWidth).checked(width(TextWidth::Wide)),
                MenuItem::action("Full Width", UseFullWidth).checked(width(TextWidth::Full)),
                MenuItem::separator(),
                MenuItem::action("Show Scrollbar", ToggleScrollbar).checked(settings.scrollbar),
                MenuItem::separator(),
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
                MenuItem::action("Reset Zoom", ResetZoom),
                // macOS adds Enter Full Screen, with an icon, at the end. In a
                // section of its own, its icon doesn't indent the items above.
                MenuItem::separator(),
            ]),
        ),
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

/// A section title: an item that does nothing, so macOS shows it dimmed.
fn section(title: &'static str) -> MenuItem {
    MenuItem::action(title, NoAction).disabled(true)
}
