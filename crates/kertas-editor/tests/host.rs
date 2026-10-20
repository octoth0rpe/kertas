//! What a host hears from the view and can set on it: Edited and
//! CaretMoved, wiki links, the link check, discarding changes, marks and
//! hover popovers. Headless, with GPUI's test text system and a clock the
//! test moves on.

mod common;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    ClipboardEntry, ClipboardItem, ClipboardString, EntityInputHandler, ExternalPaths,
    FileDropEvent, ImageFormat, IntoElement, Modifiers, MouseButton, ParentElement, TestAppContext,
    point, px,
};
use kertas_editor::actions::Unreveal;
use kertas_editor::kertas_core::rendered::RenderedBlock;
use kertas_editor::{
    AddedImages, DocumentEvent, DocumentView, EDIT_PAUSE, HOVER_DELAY, HoverTarget, Mark, MarkStyle,
};

use common::Events;

fn count(events: &Events, event: &DocumentEvent) -> usize {
    events.borrow().iter().filter(|sent| *sent == event).count()
}

/// A burst of typing sends one Edited once it pauses (#118), after which
/// the headings and word count are up to date. That a reload sends one is
/// checked by the `disk` smoke mode, as it needs a watched file.
#[gpui::test]
fn edited_is_sent_once_typing_pauses(cx: &mut TestAppContext) {
    let (view, cx, events) = common::showing(&common::fixture("showcase.md"), cx);
    let words_before = view.read_with(cx, |view, _| view.word_count());
    cx.update(|window, cx| view.update(cx, |view, cx| view.click(0, false, window, cx)));
    for text in ["# Fresh ", "heading", "\n\n"] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.replace_text_in_range(None, text, window, cx)
            })
        });
        cx.executor().advance_clock(EDIT_PAUSE / 4);
        cx.run_until_parked();
    }
    assert_eq!(
        count(&events, &DocumentEvent::Edited),
        0,
        "not sent while typing"
    );
    cx.executor().advance_clock(EDIT_PAUSE * 4);
    cx.run_until_parked();
    assert_eq!(
        count(&events, &DocumentEvent::Edited),
        1,
        "sent once after the pause"
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.headings()[0].text, "Fresh heading");
        assert_eq!(view.word_count(), words_before + 2);
    });
}

/// The caret moving without an edit sends CaretMoved right away, and the
/// view says where the caret is.
#[gpui::test]
fn caret_moved_is_sent_right_away(cx: &mut TestAppContext) {
    let (view, cx, events) = common::showing(&common::fixture("showcase.md"), cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.click(3, false, window, cx)));
    assert_eq!(count(&events, &DocumentEvent::CaretMoved), 1);
    assert_eq!(view.read_with(cx, |view, _| view.caret_offset()), Some(3));
    cx.dispatch_action(Unreveal);
    cx.run_until_parked();
    assert_eq!(
        count(&events, &DocumentEvent::CaretMoved),
        2,
        "so does Escape"
    );
    assert_eq!(view.read_with(cx, |view, _| view.caret_offset()), None);
}

/// The links in the first block, and whether each is a wiki link.
fn first_links(view: &DocumentView) -> Option<Vec<(String, bool)>> {
    match &*view.model()?.items.first()?.rendered {
        RenderedBlock::Paragraph(text) => Some(
            text.links
                .iter()
                .map(|link| (link.destination.clone(), link.wiki))
                .collect(),
        ),
        _ => None,
    }
}

/// A host turning on wiki links (#119) has the open document read again,
/// and its check of which links lead anywhere is asked as it's drawn.
#[gpui::test]
fn wiki_links_and_the_link_check(cx: &mut TestAppContext) {
    let (view, cx, _) = common::showing("Ask [[Ada|her]].\n\nMore.\n", cx);
    assert_eq!(
        view.read_with(cx, |view, _| first_links(view)),
        Some(Vec::new())
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut config = view.config().clone();
            config.wiki_links = true;
            view.set_config(config, window, cx);
        })
    });
    assert_eq!(
        view.read_with(cx, |view, _| first_links(view)),
        Some(vec![("Ada".to_owned(), true)])
    );

    let checked = Rc::new(RefCell::new(Vec::new()));
    view.update(cx, |view, cx| {
        let checked = checked.clone();
        view.set_link_check(
            Some(Rc::new(
                move |link: &kertas_editor::kertas_core::inline::Link| {
                    checked.borrow_mut().push(link.destination.clone());
                    false
                },
            )),
            cx,
        );
    });
    cx.run_until_parked();
    assert!(
        checked
            .borrow()
            .iter()
            .any(|destination| destination == "Ada"),
        "{checked:?}"
    );
}

/// A host forgetting unsaved changes, as after deleting the file, leaves
/// nothing to ask about on closing.
#[gpui::test]
fn discard_changes_leaves_nothing_to_ask(cx: &mut TestAppContext) {
    let (view, cx, _) = common::showing("Text.\n", cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.click(0, false, window, cx);
            view.replace_text_in_range(None, "x", window, cx);
        })
    });
    assert!(view.read_with(cx, |view, _| view.is_dirty()));
    view.update(cx, |view, cx| view.discard_changes(cx));
    let ready = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            !view.is_dirty() && view.ready_to_close(window, cx)
        })
    });
    assert!(ready);
}

/// A right-click and a Control-click on a host's mark (#120) tell the
/// host, leaving the block rendered. How marks move with edits is
/// unit-tested in `marks.rs`.
#[gpui::test]
fn clicking_a_mark_tells_the_host(cx: &mut TestAppContext) {
    let (view, cx, events) = common::showing(&common::fixture("marks.md"), cx);
    view.update(cx, |view, cx| {
        view.set_marks(
            vec![Mark {
                range: 0..2,
                style: MarkStyle::Hidden,
                id: 7,
            }],
            cx,
        )
    });
    cx.run_until_parked();
    let position = common::origin(&view, cx, 0) + point(px(4.), px(12.));
    common::click(cx, position, MouseButton::Right, Modifiers::default());
    common::click(cx, position, MouseButton::Left, Modifiers::control());
    let menus: Vec<_> = events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            DocumentEvent::MarkMenu { id, range, .. } => Some((*id, range.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(menus, [(7, 0..2), (7, 0..2)]);
    assert_eq!(view.read_with(cx, |view, _| view.caret_offset()), None);
}

/// Hover popovers (#126): resting the pointer on a mark or a link asks the
/// host for a popover after a delay, and it closes when the pointer
/// leaves, or on typing.
#[gpui::test]
fn hovering_asks_the_host_for_a_popover(cx: &mut TestAppContext) {
    let (view, cx, _) = common::showing(&common::fixture("marks.md"), cx);
    let asked = Rc::new(RefCell::new(Vec::new()));
    view.update(cx, |view, cx| {
        let asked = asked.clone();
        view.set_hover_popover(
            Some(Rc::new(move |target: &HoverTarget, _, _| {
                asked.borrow_mut().push(target.clone());
                Some(gpui::div().child("Popover").into_any_element())
            })),
            cx,
        );
        // "Ann", at the start of the first paragraph.
        view.set_marks(
            vec![Mark {
                range: 0..3,
                style: MarkStyle::Hidden,
                id: 4,
            }],
            cx,
        );
    });
    cx.run_until_parked();
    let shown = |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, _| view.hover_shown());
    let (first, second) = (common::origin(&view, cx, 0), common::origin(&view, cx, 1));
    let line = px(12.);

    common::move_to(cx, first + point(px(6.), line));
    assert_eq!(shown(cx), None, "a popover waits");
    cx.executor().advance_clock(HOVER_DELAY * 2);
    cx.run_until_parked();
    assert!(
        matches!(shown(cx), Some(HoverTarget::Mark(mark)) if mark.id == 4),
        "{:?}",
        shown(cx)
    );
    assert!(
        asked
            .borrow()
            .iter()
            .any(|target| matches!(target, HoverTarget::Mark(_)))
    );

    // Off the text, onto the window's side.
    common::move_to(cx, point(px(2.), first.y + line));
    cx.executor().advance_clock(HOVER_DELAY * 2);
    cx.run_until_parked();
    assert_eq!(shown(cx), None, "leaving closes it");

    // "A [link](…)": the link's text starts after "A ".
    common::move_to(cx, second + point(px(24.), line));
    cx.executor().advance_clock(HOVER_DELAY * 2);
    cx.run_until_parked();
    assert!(
        matches!(shown(cx), Some(HoverTarget::Link(link)) if link.destination == "https://example.com"),
        "{:?}",
        shown(cx)
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.click(0, false, window, cx);
            view.replace_text_in_range(None, "x", window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(shown(cx), None, "typing closes it");
}

/// The images a host was sent, and where they go.
fn images_added(events: &Events) -> Vec<(AddedImages, usize)> {
    events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            DocumentEvent::ImagesAdded { images, at } => Some((images.clone(), *at)),
            _ => None,
        })
        .collect()
}

fn image_item(bytes: &[u8]) -> ClipboardItem {
    ClipboardItem {
        entries: vec![ClipboardEntry::Image(gpui::Image {
            format: ImageFormat::Png,
            bytes: bytes.to_vec(),
            id: 1,
        })],
    }
}

/// Pasting an image (#155) goes to a host that takes images, at the caret,
/// a selection deleted first; otherwise pasting is as it was.
#[gpui::test]
fn pasted_images_go_to_the_host(cx: &mut TestAppContext) {
    let (view, cx, events) = common::showing("Before after.\n", cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.click(7, false, window, cx)));
    cx.write_to_clipboard(image_item(b"png"));
    cx.simulate_keystrokes("cmd-v");
    assert!(images_added(&events).is_empty(), "not unless the host asks");
    assert_eq!(view.read_with(cx, |view, _| view.text()), "Before after.\n");

    view.update(cx, |view, _| view.set_accepts_images(true));
    cx.simulate_keystrokes("cmd-v");
    let data = AddedImages::Data {
        bytes: b"png".to_vec(),
        format: ImageFormat::Png,
    };
    assert_eq!(images_added(&events), [(data.clone(), 7)]);

    // A selection goes, as a paste would replace it.
    events.borrow_mut().clear();
    cx.update(|window, cx| view.update(cx, |view, cx| view.select_range(0..7, window, cx)));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(view.read_with(cx, |view, _| view.text()), "after.\n");
    assert_eq!(images_added(&events), [(data, 0)]);

    // Image files copied in the Finder, which puts their names there as
    // text too, go as files; text alone is pasted as text.
    events.borrow_mut().clear();
    let files = ClipboardItem {
        entries: vec![
            ClipboardEntry::String(ClipboardString::new("map.png notes.md".into())),
            ClipboardEntry::ExternalPaths(ExternalPaths(
                vec![PathBuf::from("/a/map.png"), PathBuf::from("/a/notes.md")].into(),
            )),
        ],
    };
    cx.write_to_clipboard(files);
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        images_added(&events),
        [(AddedImages::Files(vec![PathBuf::from("/a/map.png")]), 0)]
    );
    cx.write_to_clipboard(ClipboardItem::new_string("Text ".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(view.read_with(cx, |view, _| view.text()), "Text after.\n");
}

/// Image files dropped on a block (#155) go to the end of it; other files
/// aren't taken.
#[gpui::test]
fn dropped_images_go_to_the_host(cx: &mut TestAppContext) {
    let (view, cx, events) = common::showing("First block.\n\nSecond block.\n", cx);
    view.update(cx, |view, _| view.set_accepts_images(true));
    let drop = |cx: &mut gpui::VisualTestContext, position, paths: Vec<PathBuf>| {
        cx.simulate_event(FileDropEvent::Entered {
            position,
            paths: ExternalPaths(paths.into()),
        });
        cx.simulate_event(FileDropEvent::Submit { position });
        cx.run_until_parked();
    };
    let on_first = common::origin(&view, cx, 0) + point(px(20.), px(8.));
    drop(cx, on_first, vec![PathBuf::from("/a/notes.md")]);
    assert!(images_added(&events).is_empty(), "only images");
    drop(
        cx,
        on_first,
        vec![PathBuf::from("/a/one.jpg"), PathBuf::from("/a/two.svg")],
    );
    assert_eq!(
        images_added(&events),
        [(
            AddedImages::Files(vec![
                PathBuf::from("/a/one.jpg"),
                PathBuf::from("/a/two.svg")
            ]),
            "First block.".len()
        )]
    );
}
