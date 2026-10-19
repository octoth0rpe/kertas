//! What a document view is told by the app that hosts it: how text looks,
//! the theme, the text size, and the editing features that are on.
//!
//! The view never reads the app's settings itself. The app gives it an
//! [`EditorConfig`] when it's created, and a new one whenever the settings
//! or the zoom change.

use kertas_core::find::OfferedOptions;
use kertas_core::settings::{AppearanceSetting, Settings};
use kertas_core::themes;

use crate::text_style::TextSettings;

#[derive(Debug, Clone, PartialEq)]
pub struct EditorConfig {
    /// The text width, tab width, list indent and fonts.
    pub text: TextSettings,
    /// The color theme, in both its light and dark colors.
    pub theme: themes::Theme,
    /// Whether the theme's light or dark colors are shown, or the window's
    /// appearance decides.
    pub appearance: AppearanceSetting,
    /// The current text size, in pixels.
    pub font_size: u32,
    /// The size zooming started from, which images are drawn at their
    /// natural size at.
    pub default_font_size: u32,
    /// Save once typing has paused this many seconds, or `None` not to
    /// save automatically.
    pub autosave: Option<u32>,
    /// Whether the document shows a scrollbar.
    pub scrollbar: bool,
    /// Whether the block being edited shows its keyboard hints.
    pub editing_hint: bool,
    /// Whether the blocks that aren't being edited are drawn faded, while
    /// one is (#121).
    pub fade_other_blocks: bool,
    /// Whether the caret's line is kept at the middle of the window as
    /// the caret moves (#128).
    pub typewriter_scrolling: bool,
    /// Whether `[[wiki links]]` are links (#119). Kertas leaves them off;
    /// a host that turns them on also takes clicked links
    /// ([`DocumentView::set_links_to_host`](crate::DocumentView::set_links_to_host)),
    /// since only it knows what a wiki link's target is.
    pub wiki_links: bool,
    /// Which options the find bar offers (#125): all of them, unless the
    /// host leaves some out.
    pub find_options: OfferedOptions,
}

impl Default for EditorConfig {
    /// The default settings, in the Kertas theme.
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            text: TextSettings::from(&settings),
            theme: themes::kertas().clone(),
            appearance: settings.appearance,
            font_size: settings.font_size,
            default_font_size: settings.font_size,
            autosave: settings.autosave,
            scrollbar: settings.scrollbar,
            editing_hint: settings.editing_hint,
            fade_other_blocks: settings.fade_other_blocks,
            typewriter_scrolling: settings.typewriter_scrolling,
            wiki_links: false,
            find_options: OfferedOptions::default(),
        }
    }
}

impl EditorConfig {
    /// How much images are scaled: 1 at the default size, growing and
    /// shrinking with zoom.
    pub fn image_scale(&self) -> f32 {
        self.font_size as f32 / self.default_font_size as f32
    }

    /// The syntax documents are read with.
    pub fn syntax(&self) -> kertas_core::blocks::Syntax {
        kertas_core::blocks::Syntax {
            wiki_links: self.wiki_links,
        }
    }

    /// Whether the text size differs from the default one.
    pub fn is_zoomed(&self) -> bool {
        self.font_size != self.default_font_size
    }
}
