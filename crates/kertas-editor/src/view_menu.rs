//! The View menu: appearance, theme, text width, the scrollbar, fading
//! the other blocks, typewriter scrolling, and zoom.
//!
//! Its choices are settings, so a host app rebuilds its menus whenever
//! [`AppSettings`](crate::app_settings::AppSettings) changes, to keep their
//! checkmarks current. [`items`] gives the menu's items for the current
//! settings, and [`init`] handles their actions.

use gpui::{Action, App, MenuItem, NoAction, SharedString, actions};
use kertas_core::settings::{AppearanceSetting, DEFAULT_THEME, TextWidth};

use crate::app_settings;
use crate::zoom::{ResetZoom, ZoomIn, ZoomOut};

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
        ToggleFadeOtherBlocks,
        ToggleTypewriterScrolling,
        ToggleCheckSpelling,
    ]
);

/// Uses the theme called `name`.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = kertas, no_json)]
pub struct UseTheme {
    pub name: SharedString,
}

/// Handles the View menu's actions, which change settings. The zoom
/// actions are handled by each window, with
/// [`zoom::handle_actions`](crate::zoom::handle_actions).
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
    cx.on_action(|_: &ToggleFadeOtherBlocks, cx| {
        cx.defer(|cx| {
            let fade = !app_settings::get(cx).fade_other_blocks;
            app_settings::set_or_report("fade_other_blocks", fade, cx);
        })
    });
    cx.on_action(|_: &ToggleTypewriterScrolling, cx| {
        cx.defer(|cx| {
            let on = !app_settings::get(cx).typewriter_scrolling;
            app_settings::set_or_report("typewriter_scrolling", on, cx);
        })
    });
    cx.on_action(|_: &ToggleCheckSpelling, cx| {
        cx.defer(|cx| {
            let on = !app_settings::get(cx).check_spelling;
            app_settings::set_or_report("check_spelling", on, cx);
        })
    });
}

/// The View menu's items, checked for the current settings.
pub fn items(cx: &App) -> Vec<MenuItem> {
    let settings = app_settings::get(cx);
    let appearance = |value: AppearanceSetting| settings.appearance == value;
    let width = |value: TextWidth| settings.text_width == value;
    [
        section("Appearance"),
        MenuItem::action("Light", UseLightAppearance).checked(appearance(AppearanceSetting::Light)),
        MenuItem::action("Dark", UseDarkAppearance).checked(appearance(AppearanceSetting::Dark)),
        MenuItem::action("Match System", UseSystemAppearance)
            .checked(appearance(AppearanceSetting::System)),
        MenuItem::separator(),
        section("Theme"),
    ]
    .into_iter()
    .chain(app_settings::themes(cx).iter().map(|theme| {
        let name = SharedString::from(theme.name.clone());
        MenuItem::action(name.clone(), UseTheme { name }).checked(theme.name == settings.theme)
    }))
    .chain([
        MenuItem::separator(),
        section("Text Width"),
        MenuItem::action("Standard", UseStandardWidth).checked(width(TextWidth::Standard)),
        MenuItem::action("Wide", UseWideWidth).checked(width(TextWidth::Wide)),
        MenuItem::action("Full Width", UseFullWidth).checked(width(TextWidth::Full)),
        MenuItem::separator(),
        MenuItem::action("Show Scrollbar", ToggleScrollbar).checked(settings.scrollbar),
        MenuItem::action("Fade Other Blocks", ToggleFadeOtherBlocks)
            .checked(settings.fade_other_blocks),
        MenuItem::action("Typewriter Scrolling", ToggleTypewriterScrolling)
            .checked(settings.typewriter_scrolling),
        MenuItem::action("Check Spelling", ToggleCheckSpelling).checked(settings.check_spelling),
        MenuItem::separator(),
        MenuItem::action("Zoom In", ZoomIn),
        MenuItem::action("Zoom Out", ZoomOut),
        MenuItem::action("Reset Zoom", ResetZoom),
        // macOS adds Enter Full Screen, with an icon, at the end. In a
        // section of its own, its icon doesn't indent the items above.
        MenuItem::separator(),
    ])
    .collect()
}

/// A section title: an item that does nothing, so macOS shows it dimmed.
pub fn section(title: &'static str) -> MenuItem {
    MenuItem::action(title, NoAction).disabled(true)
}
