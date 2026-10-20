//! What the view tests share: a window with a document in it, the events
//! it sends, the fixtures, and the real macOS text system.

// Each test file uses only some of these.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    PlatformTextSystem, Point, TestAppContext, VisualTestContext,
};
use kertas_editor::actions::Unreveal;
use kertas_editor::{DocumentEvent, DocumentView, EditorConfig};

/// The events a view sent.
pub type Events = Rc<RefCell<Vec<DocumentEvent>>>;

/// A window showing a new untitled document, ready for typing, and the
/// events the view sends.
pub fn untitled(cx: &mut TestAppContext) -> (Entity<DocumentView>, &mut VisualTestContext, Events) {
    cx.update(|cx| cx.bind_keys(kertas_editor::key_bindings()));
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = DocumentView::new(EditorConfig::default(), window, cx);
        view.start_untitled(window, cx);
        view
    });
    let events = Events::default();
    cx.update(|_, cx| {
        let events = events.clone();
        cx.subscribe(&view, move |_, event: &DocumentEvent, _| {
            events.borrow_mut().push(event.clone())
        })
        .detach();
    });
    cx.run_until_parked();
    (view, cx, events)
}

/// A window showing `text`, editable and with no caret, as a document is
/// when it's opened.
pub fn showing<'a>(
    text: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<DocumentView>, &'a mut VisualTestContext, Events) {
    let (view, cx, events) = untitled(cx);
    // Put in at once: `simulate_input` types a character at a time.
    view.update(cx, |view, cx| {
        view.replace_ranges(vec![(0..0, text.to_owned())], cx)
    });
    cx.dispatch_action(Unreveal);
    cx.run_until_parked();
    // Only what the test does from here.
    events.borrow_mut().clear();
    (view, cx, events)
}

/// The text of `name` in tests/fixtures.
pub fn fixture(name: &str) -> String {
    let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// A test app with the real macOS text system, so lines break where they
/// do in the app. The text system is built once and shared, as loading
/// its fonts takes a few seconds.
#[cfg(target_os = "macos")]
pub fn with_real_text() -> TestAppContext {
    static TEXT_SYSTEM: OnceLock<Arc<dyn PlatformTextSystem>> = OnceLock::new();
    let text_system = TEXT_SYSTEM
        .get_or_init(|| Arc::new(gpui_apple::AppleTextSystem::new()))
        .clone();
    TestAppContext::build_with_text_system(gpui::TestDispatcher::new(0), None, text_system)
}

/// Lets time pass for whatever waits for typing to pause, and for the
/// work that starts then (checking spelling, sending Edited) to finish.
pub fn settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(kertas_editor::EDIT_PAUSE * 2);
    cx.run_until_parked();
}

/// Where the text of the view's `index`th block starts.
pub fn origin(
    view: &Entity<DocumentView>,
    cx: &mut VisualTestContext,
    index: usize,
) -> Point<Pixels> {
    cx.update(|window, cx| view.read(cx).text_origin(index, window))
        .unwrap_or_else(|| panic!("block {index} isn't laid out"))
}

/// Moves the pointer to `position`.
pub fn move_to(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_event(MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Modifiers::default(),
    });
    cx.run_until_parked();
}

/// Clicks `button` at `position`, holding `modifiers`.
pub fn click(
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
    button: MouseButton,
    modifiers: Modifiers,
) {
    cx.simulate_event(MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers,
    });
    cx.simulate_event(MouseDownEvent {
        button,
        position,
        modifiers,
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button,
        position,
        modifiers,
        click_count: 1,
    });
    cx.run_until_parked();
}
