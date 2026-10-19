//! Core of the Kertas Markdown editor: the document model, Markdown
//! parsing and settings, without any UI. Doesn't depend on GPUI.

pub mod anchors;
pub mod autolinks;
pub mod blocks;
pub mod buffer;
pub mod definitions;
pub mod dictionaries;
pub mod document;
pub mod editing;
pub mod editor;
pub mod find;
pub mod history;
pub mod html_tags;
pub mod inline;
pub mod links;
pub mod model;
pub mod movement;
pub mod outline;
pub mod print_layout;
pub mod rendered;
pub mod reveal;
pub mod scroll_estimate;
pub mod settings;
pub mod spelling;
pub mod tabs;
pub mod themes;
pub mod titles;
pub mod words;
