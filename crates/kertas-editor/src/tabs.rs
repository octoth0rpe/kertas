//! Tabs holding document views: kertas-ui's [`Tabs`], with
//! [`DocumentView`] as a [`TabItem`]. Everything else here is kertas-ui's
//! [`kertas_ui::tabs`], so an app can use either path.

use gpui::{App, Context, Task, Window};

pub use kertas_ui::tabs::*;

use crate::{DocumentEvent, DocumentView, Theme};

/// Tabs holding document views.
pub type DocumentTabs = Tabs<DocumentView>;

impl TabItem for DocumentView {
    type Event = DocumentEvent;

    const STRIP_LABEL: &'static str = "Documents";

    fn title(&self, _: &App) -> Option<String> {
        self.display_name()
    }

    fn theme(&self, _: &App) -> Theme {
        DocumentView::theme(self)
    }

    fn is_dirty(&self, _: &App) -> bool {
        DocumentView::is_dirty(self)
    }

    fn hidden(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Leaving a tab autosaves it, as leaving a window does.
        DocumentView::hidden(self, window, cx);
    }

    fn ready_to_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        DocumentView::ready_to_close(self, window, cx)
    }

    fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        DocumentView::confirm_close(self, window, cx)
    }
}
