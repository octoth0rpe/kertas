//! Images pasted or dropped onto the document, passed to a host that asks
//! for them (#155) with [`DocumentView::set_accepts_images`]. Kertas saves
//! nothing itself: the host keeps the image where it keeps its images and
//! puts the Markdown that shows it in, with
//! [`DocumentView::replace_ranges`].

use std::path::{Path, PathBuf};

use gpui::{
    App, ClipboardEntry, ClipboardItem, Context, ExternalPaths, ImageFormat, Pixels, Point, Window,
};

use super::{DocumentEvent, DocumentView};

/// Images to add to the document, pasted or dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddedImages {
    /// Image files, such as ones dropped from the Finder or copied there,
    /// in order.
    Files(Vec<PathBuf>),
    /// An image's data, pasted from an app that copied the picture itself.
    Data { bytes: Vec<u8>, format: ImageFormat },
}

/// The extensions of the image files a host is told about.
const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "svg", "webp"];

/// Whether `path` is an image file, by its extension.
pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// The image files among `paths`, in order.
fn image_files(paths: &ExternalPaths) -> Vec<PathBuf> {
    paths
        .paths()
        .iter()
        .filter(|path| is_image_file(path))
        .cloned()
        .collect()
}

/// The images on the clipboard in `item`, if any: image files copied in
/// the Finder, which also puts their names there as text, or else a
/// picture's data, when there's no text.
pub(super) fn clipboard_images(item: &ClipboardItem) -> Option<AddedImages> {
    let files: Vec<PathBuf> = item
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            ClipboardEntry::ExternalPaths(paths) => Some(image_files(paths)),
            _ => None,
        })
        .flatten()
        .collect();
    if !files.is_empty() {
        return Some(AddedImages::Files(files));
    }
    if item.text().is_some() {
        return None;
    }
    item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(AddedImages::Data {
            bytes: image.bytes.clone(),
            format: image.format,
        }),
        _ => None,
    })
}

impl DocumentView {
    /// Asks for images pasted or dropped onto the document, sent as
    /// [`DocumentEvent::ImagesAdded`] in place of pasting their names or
    /// ignoring the drop. Off by default.
    pub fn set_accepts_images(&mut self, on: bool) {
        self.accepts_images = on;
    }

    /// Whether a paste or drop of images goes to the host now: it asked
    /// for them, and the document can be edited.
    fn takes_images(&self) -> bool {
        self.accepts_images
            && self
                .open
                .as_ref()
                .is_some_and(|open| !open.editor.read_only())
    }

    /// Pastes the images in `item` by sending them to the host, if it
    /// takes them, after deleting the selection as a paste would. Returns
    /// whether it did.
    pub(super) fn paste_images(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        if !self.takes_images() || self.caret_offset().is_none() {
            return false;
        }
        let Some(images) = clipboard_images(item) else {
            return false;
        };
        let selected = self
            .open
            .as_ref()
            .is_some_and(|open| open.editor.selected_text().is_some());
        if selected {
            self.edit(cx, |editor| editor.paste(""));
        }
        let Some(at) = self.caret_offset() else {
            return false;
        };
        cx.emit(DocumentEvent::ImagesAdded { images, at });
        true
    }

    /// Whether `dragged` is image files the host would take.
    pub(super) fn can_drop(&self, dragged: &dyn std::any::Any) -> bool {
        self.takes_images()
            && dragged
                .downcast_ref::<ExternalPaths>()
                .is_some_and(|paths| !image_files(paths).is_empty())
    }

    /// Sends the image files among `paths`, dropped at `position` in the
    /// window, to the host.
    pub(super) fn drop_images(
        &mut self,
        paths: &ExternalPaths,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let files = image_files(paths);
        if files.is_empty() || !self.takes_images() {
            return;
        }
        let Some(at) = self.drop_offset(position) else {
            return;
        };
        cx.emit(DocumentEvent::ImagesAdded {
            images: AddedImages::Files(files),
            at,
        });
    }

    /// Where something dropped at `position` goes in the text: there, in a
    /// block being edited; the end of any other block under it; or the
    /// end of the document, below them all.
    fn drop_offset(&self, position: Point<Pixels>) -> Option<usize> {
        let open = self.open.as_ref()?;
        if let Some((layout, start)) = self.revealed_layout()
            && let Ok(index) = layout.index_for_position(position)
        {
            return Some(start + index);
        }
        let items = &open.editor.model().items;
        let first = open.list_state.logical_scroll_top().item_ix;
        for (index, item) in items.iter().enumerate().skip(first) {
            let Some(bounds) = open.list_state.bounds_for_item(index) else {
                break;
            };
            if position.y < bounds.bottom() {
                return Some(item.block.end);
            }
        }
        Some(open.editor.text().len())
    }
}

/// Registers drops of image files on `element`, the document, for `view`.
pub(super) fn on_drop(
    element: gpui::Stateful<gpui::Div>,
    cx: &mut Context<DocumentView>,
) -> gpui::Stateful<gpui::Div> {
    use gpui::InteractiveElement;

    let (drop_view, check_view) = (cx.weak_entity(), cx.weak_entity());
    element
        .can_drop(move |dragged, _: &mut Window, cx: &mut App| {
            check_view
                .read_with(cx, |view, _| view.can_drop(dragged))
                .unwrap_or(false)
        })
        .on_drop(move |paths: &ExternalPaths, window, cx| {
            let position = window.mouse_position();
            drop_view
                .update(cx, |view, cx| view.drop_images(paths, position, cx))
                .ok();
        })
}
