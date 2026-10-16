//! Drawing blocks in rendered mode, and mapping clicks on them back to
//! source offsets.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use gpui::{
    AccessibleAction, AnyElement, App, Bounds, ElementId, Font, FontWeight, Hsla, ImageSource,
    ImgResourceLoader, InteractiveText, MouseButton, Pixels, Rems, Resource, Role, SharedString,
    StyledText, TextLayout, Toggled, WeakEntity, Window, canvas, div, img, point, prelude::*, px,
    rems, size, svg,
};
use kertas::inline::{Image, InlineText, Link};
use kertas::links::ImageLocation;
use kertas::model::Note;
use kertas::rendered::{AlertKind, ColumnAlignment, ListItem, RenderedBlock, Table, Task};
use kertas::settings::ListIndent;
use kertas::tabs::ExpandedTabs;
use kertas::{html_tags, links};

use crate::assets::alert_icon;
use crate::document_view::DocumentView;
use crate::text_style::{Look, code_text, heading_text, inline_run};
use crate::theme::Theme;

/// Space between blocks inside lists and blockquotes.
const NESTED_GAP: f32 = 8.;

/// The width of a list's marker column with the standard `list_indent`,
/// in rems (28 px at the default size): how far its items are indented.
const MARKER_COLUMN: f32 = 1.75;

/// Space between a list marker and its item, in rems.
const MARKER_GAP: f32 = 0.5;

/// Text styling inherited by nested blocks.
#[derive(Clone)]
struct TextStyle {
    font: Font,
    color: Hsla,
    theme: Theme,
    tab_width: u32,
    list_indent: ListIndent,
    code_font: SharedString,
    click: ClickTarget,
    /// How many accessibility nodes the block has so far, to number them.
    nodes: Rc<Cell<usize>>,
}

impl TextStyle {
    /// An ID for the block's next accessibility node, unique within the
    /// block. Nodes need IDs to be reported, and the block's own ID keeps
    /// them apart from other blocks' nodes.
    fn node_id(&self, kind: &'static str) -> ElementId {
        let index = self.nodes.get();
        self.nodes.set(index + 1);
        ElementId::named_usize(kind, index)
    }
}

/// Where clicks on a block's text are reported, and the text column the
/// block is drawn in.
#[derive(Clone)]
pub struct ClickTarget {
    pub view: WeakEntity<DocumentView>,
    /// The block's start offset in the document.
    pub block_start: usize,
    /// The text column's maximum width, or `None` for the full window.
    pub column_width: Option<Rems>,
    /// Whether the document can be edited, so checkboxes can be toggled.
    pub editable: bool,
    /// The document's file, which images' relative paths start from.
    pub document: Option<Rc<Path>>,
    /// Images still loading, by their block's start, so the view can
    /// re-measure the block once each one arrives.
    pub loading_images: LoadingImages,
    /// Whether web images are loaded (#97).
    pub web_images: bool,
    /// The footnote reference the view is scrolling to, if any.
    pub reference: Option<ReferenceTarget>,
}

/// Images still loading, and where their blocks start.
pub type LoadingImages = Rc<std::cell::RefCell<Vec<(usize, Resource)>>>;

/// A footnote reference that a note's back-link is scrolling to the top
/// of the window (#90).
#[derive(Clone, Copy)]
pub struct ReferenceTarget {
    /// Where the reference starts in the document.
    pub offset: usize,
    /// The height of the window's document area.
    pub viewport_height: Pixels,
}

impl ClickTarget {
    /// Places the caret at `offset` in the block's source (shift-click
    /// extends the selection).
    pub fn report(&self, offset: usize, extend: bool, window: &mut Window, cx: &mut App) {
        let offset = self.block_start + offset;
        self.view
            .update(cx, |view, cx| view.click(offset, extend, window, cx))
            .ok();
    }

    /// Extends the selection to `offset` in the block's source while the
    /// mouse is dragged.
    pub fn drag(&self, offset: usize, cx: &mut App) {
        let offset = self.block_start + offset;
        self.view
            .update(cx, |view, cx| view.drag_to(offset, cx))
            .ok();
    }

    /// Notes that the mouse went down on the checkbox whose `[ ]` starts at
    /// `marker` in the block's source.
    pub fn press_task(&self, marker: usize, cx: &mut App) {
        let marker = self.block_start + marker;
        self.view.update(cx, |view, _| view.press_task(marker)).ok();
    }

    /// Toggles the checkbox at `marker` in the block's source, if the mouse
    /// went down on it.
    pub fn release_task(&self, marker: usize, cx: &mut App) {
        let marker = self.block_start + marker;
        self.view
            .update(cx, |view, cx| view.release_task(marker, cx))
            .ok();
    }

    /// Toggles the checkbox at `marker` in the block's source, as VoiceOver
    /// asks.
    pub fn toggle_task(&self, marker: usize, cx: &mut App) {
        let marker = self.block_start + marker;
        self.view
            .update(cx, |view, cx| view.toggle_task(marker, cx))
            .ok();
    }

    /// Follows a clicked link to `destination`.
    pub fn follow_link(&self, destination: &str, window: &mut Window, cx: &mut App) {
        self.view
            .update(cx, |view, cx| view.follow_link(destination, window, cx))
            .ok();
    }

    /// Finishes editing, as Escape does.
    pub fn finish_editing(&self, cx: &mut App) {
        self.view
            .update(cx, |view, cx| view.finish_editing(cx))
            .ok();
    }
}

/// Renders one block in rendered mode as a list item.
pub fn render_block(
    block: &RenderedBlock,
    first: bool,
    look: &Look,
    click: ClickTarget,
) -> AnyElement {
    // Drawn with the other notes, at the end (#90).
    if let RenderedBlock::FootnoteDefinition { .. } = block {
        return div()
            .when(first, |this| this.pt(px(32.)))
            .into_any_element();
    }
    let content = rendered_content(block, look, click.clone());
    block_frame(content, rendered_heading(block), first, click)
}

/// The heading level of a rendered block, if it's a heading.
pub fn rendered_heading(block: &RenderedBlock) -> Option<u8> {
    match block {
        RenderedBlock::Heading { level, .. } => Some(*level),
        _ => None,
    }
}

/// A block's rendered content, without the list item's spacing.
pub fn rendered_content(block: &RenderedBlock, look: &Look, click: ClickTarget) -> AnyElement {
    let style = TextStyle {
        font: look.font.clone(),
        color: look.theme.text,
        theme: look.theme,
        tab_width: look.tab_width,
        list_indent: look.list_indent,
        code_font: look.code_font.clone(),
        nodes: Rc::default(),
        click,
    };
    div()
        .id(ElementId::named_usize("block", style.click.block_start))
        .child(render_content(block, &style, 0))
        .into_any_element()
}

/// Wraps a block's content as a list item: centered in the text column,
/// with spacing around it. Clicks in the column that miss the block's text
/// place the caret at the block's start; clicks outside the column finish
/// editing.
pub fn block_frame(
    content: AnyElement,
    heading: Option<u8>,
    first: bool,
    click: ClickTarget,
) -> AnyElement {
    let top = match heading {
        Some(level) if !first => heading_top_space(level),
        _ => 0.,
    };
    let outside = click.clone();
    let column_width = click.column_width;
    div()
        .w_full()
        .px_8()
        .when(first, |this| this.pt(px(32.)))
        .pb(px(16.))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            outside.finish_editing(cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .w_full()
                .when_some(column_width, |this, width| this.max_w(width))
                .mx_auto()
                .pt(px(top))
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    click.report(0, event.modifiers.shift, window, cx);
                    cx.stop_propagation();
                })
                .child(content),
        )
        .into_any_element()
}

/// Renders a block's content, without the spacing around it. `depth` is
/// how many lists the block is nested in, which picks the bullet style.
fn render_content(block: &RenderedBlock, style: &TextStyle, depth: usize) -> AnyElement {
    match block {
        RenderedBlock::Paragraph(text) => clickable_text(text, style, true),
        RenderedBlock::Heading { level, text } => {
            let mut heading_style = style.clone();
            heading_style.font.weight = FontWeight::SEMIBOLD;
            if *level == 6 {
                heading_style.color = style.theme.muted_text;
            }
            heading_text(div(), *level)
                .id(style.node_id("heading"))
                .role(Role::Heading)
                .aria_level(usize::from(*level))
                .aria_label(text.text.clone())
                // The heading's name is its text, so the text isn't
                // reported again.
                .child(clickable_text(text, &heading_style, false))
                .into_any_element()
        }
        RenderedBlock::ThematicBreak => div()
            .py(px(8.))
            .child(div().h(px(2.)).bg(style.theme.rule))
            .into_any_element(),
        RenderedBlock::List { start, items } => render_list(*start, items, style, depth),
        RenderedBlock::BlockQuote(children) => {
            let quote_style = TextStyle {
                color: style.theme.muted_text,
                ..style.clone()
            };
            div()
                .border_l_4()
                .border_color(style.theme.rule)
                .pl_4()
                .text_color(style.theme.muted_text)
                .child(render_children(children, &quote_style, depth))
                .into_any_element()
        }
        RenderedBlock::Alert { kind, children } => render_alert(*kind, children, style, depth),
        RenderedBlock::CodeBlock { code, .. } => code_text(div(), &style.code_font)
            .bg(style.theme.code_background)
            .rounded_md()
            .px_4()
            .py_3()
            .text_color(style.theme.text)
            .child({
                let shown = code.text.trim_end_matches(['\n', '\r']);
                let code = code.clone();
                clickable_source(shown, style, move |index| code.source_offset(index))
            })
            .into_any_element(),
        RenderedBlock::Table(table) => render_table(table, style),
        // Drawn with the other notes, at the end (#90).
        RenderedBlock::FootnoteDefinition { .. } => div().into_any_element(),
        // `<a id="x"></a>` only marks a link target (#96).
        RenderedBlock::Html(source) if html_tags::only_targets(source) => div().into_any_element(),
        RenderedBlock::Html(source) | RenderedBlock::Raw(source) => {
            code_text(div(), &style.code_font)
                .text_color(style.theme.muted_text)
                .child(clickable_source(source, style, |index| index))
                .into_any_element()
        }
    }
}

/// The size of footnotes' text, relative to body text.
const NOTE_SCALE: f32 = 0.875;

/// The width of the footnotes' number column, in rems at their size:
/// wide enough for the largest number.
pub fn note_marker_column(list_indent: ListIndent, notes: &[Note]) -> f32 {
    let markers: Vec<String> = notes
        .iter()
        .filter_map(|note| note.number.map(|number| format!("{number}.")))
        .collect();
    marker_column(list_indent, &markers)
}

/// A footnote, as one of the rows after the document's blocks (#90): its
/// number and text, with a back-link to each reference. The first has a
/// thin rule above it. A footnote that's never referenced shows its
/// label instead of a number. `column` is the number column's width, from
/// [`note_marker_column`].
pub fn render_note(
    note: &Note,
    first: bool,
    column: f32,
    look: &Look,
    click: ClickTarget,
) -> AnyElement {
    let style = TextStyle {
        font: look.font.clone(),
        color: look.theme.text,
        theme: look.theme,
        tab_width: look.tab_width,
        list_indent: look.list_indent,
        code_font: look.code_font.clone(),
        nodes: Rc::default(),
        click: click.clone(),
    };
    let mut children = note.children.clone();
    if note.references > 0 {
        match children.last_mut() {
            Some(RenderedBlock::Paragraph(text)) => {
                text.push_back_links(&note.label, note.references)
            }
            _ => {
                let mut text = InlineText::default();
                text.push_back_links(&note.label, note.references);
                children.push(RenderedBlock::Paragraph(text));
            }
        }
    }
    let (marker, name) = match note.number {
        Some(number) => (format!("{number}."), format!("Footnote {number}")),
        None => (
            format!("[^{}]", note.label),
            format!("Footnote {}", note.label),
        ),
    };
    let row = div()
        .id(ElementId::named_usize("note", click.block_start))
        .role(Role::Note)
        .aria_label(name)
        .flex()
        .text_size(rems(NOTE_SCALE))
        .child(
            div()
                .flex_none()
                .min_w(rems(column))
                .pr(rems(MARKER_GAP))
                .text_right()
                .when(note.number.is_none(), |this| {
                    this.text_color(look.theme.muted_text)
                })
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .child(render_children(&children, &style, 1)),
        );
    let content = div()
        .when(first, |this| {
            this.child(div().mt(px(16.)).mb(px(16.)).h(px(1.)).bg(look.theme.rule))
        })
        .child(row)
        .into_any_element();
    block_frame(content, None, false, click)
}

/// A GitHub alert: a colored bar, a title with an icon, and its contents.
fn render_alert(
    kind: AlertKind,
    children: &[RenderedBlock],
    style: &TextStyle,
    depth: usize,
) -> AnyElement {
    let color = style.theme.alert(kind);
    div()
        .id(style.node_id("alert"))
        .role(Role::Note)
        .aria_label(kind.title())
        .border_l_4()
        .border_color(color)
        .pl_4()
        .flex()
        .flex_col()
        .gap(px(NESTED_GAP))
        .child(
            div()
                .flex()
                .items_center()
                .gap(rems(0.5))
                .text_color(color)
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    svg()
                        .path(alert_icon(kind))
                        .flex_none()
                        .size(rems(1.))
                        .text_color(color),
                )
                .child(kind.title()),
        )
        .child(render_children(children, style, depth))
        .into_any_element()
}

/// Nested blocks stacked vertically, e.g. the contents of a list item.
fn render_children(children: &[RenderedBlock], style: &TextStyle, depth: usize) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(NESTED_GAP))
        .children(
            children
                .iter()
                .map(|child| render_content(child, style, depth)),
        )
        .into_any_element()
}

fn render_list(
    start: Option<u64>,
    items: &[ListItem],
    style: &TextStyle,
    depth: usize,
) -> AnyElement {
    let bullet = ["•", "◦", "▪"][depth % 3];
    let markers: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(index, item)| match (item.task, start) {
            (Some(task), _) if task.checked => "☑".to_owned(),
            (Some(_), _) => "☐".to_owned(),
            (None, Some(start)) => format!("{}.", start + index as u64),
            (None, None) => bullet.to_owned(),
        })
        .collect();
    let column = marker_column(style.list_indent, &markers);
    let count = items.len();
    div()
        .id(style.node_id("list"))
        .role(Role::List)
        .flex()
        .flex_col()
        .gap(px(4.))
        .children(
            items
                .iter()
                .zip(markers)
                .enumerate()
                .map(|(index, (item, marker))| {
                    div()
                        .id(style.node_id("item"))
                        .role(Role::ListItem)
                        .aria_position_in_set(index + 1)
                        .aria_size_of_set(count)
                        .flex()
                        .child(
                            div()
                                .flex_none()
                                .w(rems(column))
                                .pr(rems(MARKER_GAP))
                                .text_right()
                                .text_color(style.color)
                                .child(match item.task {
                                    Some(task) => task_checkbox(marker, task, item, style),
                                    None => marker.into_any_element(),
                                }),
                        )
                        .child(div().flex_1().min_w(px(0.)).child(render_children(
                            &item.children,
                            style,
                            depth + 1,
                        )))
                }),
        )
        .into_any_element()
}

/// A task list item's checkbox, which toggles when clicked unless the
/// document is view-only.
fn task_checkbox(checkbox: String, task: Task, item: &ListItem, style: &TextStyle) -> AnyElement {
    // Named after the task, for VoiceOver.
    let name = match item.children.first() {
        Some(RenderedBlock::Paragraph(text)) => text.text.clone(),
        _ => String::new(),
    };
    let marker = task.marker;
    let click = &style.click;
    let (press, release, toggle) = (click.clone(), click.clone(), click.clone());
    let checkbox = div()
        .id(style.node_id("checkbox"))
        .role(Role::CheckBox)
        .aria_label(name)
        .aria_toggled(if task.checked {
            Toggled::True
        } else {
            Toggled::False
        })
        .child(checkbox);
    let checkbox = if click.editable {
        checkbox
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                // Leave the list rendered.
                press.press_task(marker, cx);
                cx.stop_propagation();
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                release.release_task(marker, cx);
            })
            .on_a11y_action(AccessibleAction::Click, move |_, _, cx| {
                toggle.toggle_task(marker, cx);
            })
    } else {
        checkbox
    };
    div()
        .flex()
        .justify_end()
        .child(checkbox)
        .into_any_element()
}

/// The width of a list's marker column, in rems: the `list_indent`, or
/// wider if a marker (such as "100.") needs more room.
fn marker_column(list_indent: ListIndent, markers: &[String]) -> f32 {
    let widest = markers
        .iter()
        .map(|marker| marker_width(marker))
        .fold(0., f32::max);
    (MARKER_COLUMN * list_indent.scale()).max(widest + MARKER_GAP)
}

/// About how wide `marker` is, in ems, erring on the wide side.
fn marker_width(marker: &str) -> f32 {
    marker
        .chars()
        .map(|c| match c {
            '.' => 0.3,
            '☐' | '☑' => 1.,
            _ => 0.6,
        })
        .sum()
}

fn render_table(table: &Table, style: &TextStyle) -> AnyElement {
    let mut header_style = style.clone();
    header_style.font.weight = FontWeight::SEMIBOLD;

    let row = |cells: &[InlineText], style: &TextStyle, header: bool| {
        div()
            .flex()
            .when(header, |this| this.bg(style.theme.table_header_background))
            .children(cells.iter().enumerate().map(|(column, cell)| {
                let alignment = table
                    .alignments
                    .get(column)
                    .copied()
                    .unwrap_or(ColumnAlignment::None);
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .px_3()
                    .py_1()
                    .border_1()
                    .border_color(style.theme.rule)
                    .map(|this| match alignment {
                        ColumnAlignment::Center => this.text_center(),
                        ColumnAlignment::Right => this.text_right(),
                        ColumnAlignment::Left | ColumnAlignment::None => this,
                    })
                    .child(clickable_text(cell, style, true))
            }))
    };

    div()
        .flex()
        .flex_col()
        .child(row(&table.header, &header_style, true))
        .children(table.rows.iter().map(|cells| row(cells, style, false)))
        .into_any_element()
}

fn heading_top_space(level: u8) -> f32 {
    match level {
        1 | 2 => 24.,
        3 => 20.,
        _ => 16.,
    }
}

/// Inline text that places the caret at the matching source position
/// when clicked. A plain click on a link follows it instead, and the
/// pointer is a hand over links. The text is reported to VoiceOver as a
/// label if `accessible` is set.
fn clickable_text(text: &InlineText, style: &TextStyle, accessible: bool) -> AnyElement {
    if text.images.is_empty() {
        clickable_run(text, style, accessible)
    } else {
        text_with_images(text, style, accessible)
    }
}

/// Text with images in it (#92): the text between the images, and the
/// images, wrapping like words.
fn text_with_images(text: &InlineText, style: &TextStyle, accessible: bool) -> AnyElement {
    let mut pieces = Vec::new();
    let mut position = 0;
    for image in &text.images {
        if image.display.start > position {
            let run = text.slice(position..image.display.start);
            pieces.push(clickable_run(&run, style, accessible));
        }
        pieces.push(
            DocumentImage {
                image: image.clone(),
                alt: text.text[image.display.clone()].to_owned(),
                link: text.link_around(image).cloned(),
                source: text.source_offset(image.display.start),
                theme: style.theme,
                click: style.click.clone(),
                id: style.node_id("image"),
            }
            .into_any_element(),
        );
        position = image.display.end;
    }
    if position < text.text.len() {
        let run = text.slice(position..text.text.len());
        pieces.push(clickable_run(&run, style, accessible));
    }
    div()
        .flex()
        .flex_wrap()
        .items_end()
        .children(pieces)
        .into_any_element()
}

/// An image in the document, drawn at its natural size (scaled with zoom),
/// never wider than the space it's in. If it can't be shown, its alt text
/// is shown in a frame instead.
#[derive(IntoElement)]
struct DocumentImage {
    image: Image,
    alt: String,
    /// The link around the image, which a click follows.
    link: Option<Link>,
    /// Where the image starts in the block's source.
    source: usize,
    theme: crate::theme::Theme,
    click: ClickTarget,
    id: ElementId,
}

impl RenderOnce for DocumentImage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let location =
            links::image_location(&self.image.destination, self.click.document.as_deref());
        let resource = match location {
            Some(ImageLocation::File(path)) => Some(Resource::Path(path.into())),
            // Until the user allows them, web images show as missing.
            Some(ImageLocation::Web(url)) if self.click.web_images => {
                Some(Resource::Uri(url.into()))
            }
            _ => None,
        };
        let loaded = resource
            .as_ref()
            .map(|resource| window.use_asset::<ImgResourceLoader>(resource, cx));
        if let (Some(None), Some(resource)) = (&loaded, resource) {
            // Its block was measured without it, so it'll need measuring
            // again once it arrives, even if it's off screen.
            let mut loading = self.click.loading_images.borrow_mut();
            let entry = (self.click.block_start, resource);
            if !loading.contains(&entry) {
                loading.push(entry);
            }
        }
        let scale = crate::zoom::image_scale(cx);
        let (click, follow, link, source) = (
            self.click.clone(),
            self.click.clone(),
            self.link.clone(),
            self.source,
        );
        let content = match loaded {
            Some(Some(Ok(image))) => {
                let (width, height) = natural_size(&image, &self.image.destination);
                div()
                    .w(px(width * scale))
                    .max_w_full()
                    .when(width > 0. && height > 0., |this| {
                        this.aspect_ratio(width / height)
                    })
                    .child(img(ImageSource::Render(image)).size_full())
                    .into_any_element()
            }
            // Still loading: nothing yet, then the image.
            Some(None) => div().into_any_element(),
            // Missing, unreadable, from the web and not allowed or not
            // found, or no folder to find it in.
            _ => div()
                .px_2()
                .border_1()
                .border_color(self.theme.rule)
                .rounded_sm()
                .text_color(self.theme.muted_text)
                .child(if self.alt.is_empty() {
                    "Image".to_owned()
                } else {
                    self.alt.clone()
                })
                .into_any_element(),
        };
        div()
            .id(self.id)
            .role(Role::Image)
            .aria_label(if self.alt.is_empty() {
                SharedString::from("Image")
            } else {
                SharedString::from(self.alt.clone())
            })
            .max_w_full()
            .when(link.is_some(), |this| this.cursor_pointer())
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                // A plain click on a linked image leaves the block
                // rendered, as on a link.
                if link.is_none() || event.modifiers.modified() {
                    click.report(source, event.modifiers.shift, window, cx);
                }
                cx.stop_propagation();
            })
            .on_click(move |event, window, cx| {
                if let Some(link) = &self.link
                    && !event.modifiers().modified()
                {
                    follow.follow_link(&link.destination, window, cx);
                }
            })
            .child(content)
    }
}

/// An image's size as the web would show it, in pixels. SVGs are drawn at
/// twice their size, so they stay sharp on Retina screens.
pub(crate) fn natural_size(image: &gpui::RenderImage, destination: &str) -> (f32, f32) {
    let size = image.size(0);
    let (width, height) = (size.width.0 as f32, size.height.0 as f32);
    // A web image's address may have a query after its path.
    let path = destination.split(['?', '#']).next().unwrap_or_default();
    let svg = Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
    if svg {
        (
            width / gpui::SMOOTH_SVG_SCALE_FACTOR,
            height / gpui::SMOOTH_SVG_SCALE_FACTOR,
        )
    } else {
        (width, height)
    }
}

/// A run of text with no images: styled, with clickable links.
fn clickable_run(text: &InlineText, style: &TextStyle, accessible: bool) -> AnyElement {
    let styled = styled_text(text, style);
    let layout = styled.layout().clone();
    let scroll_to_reference = scroll_to_reference(text, &layout, &style.click);
    let inline = text.clone();
    let click = style.click.clone();
    let (drag_layout, drag_inline, drag) = (layout.clone(), inline.clone(), click.clone());
    let content = if text.links.is_empty() {
        styled.into_any_element()
    } else {
        // Links follow when the button goes up over the link it went down
        // on, like links elsewhere.
        let ranges = text.links.iter().map(|link| link.display.clone()).collect();
        let (links, follow) = (text.links.clone(), click.clone());
        // Text with links reports itself as a label.
        InteractiveText::new(style.node_id("links"), styled)
            .on_click(ranges, move |index, window, cx| {
                if window.modifiers().modified() {
                    return;
                }
                if let Some(link) = links.get(index) {
                    follow.follow_link(&link.destination, window, cx);
                }
            })
            .into_any_element()
    };
    let label =
        (accessible && text.links.is_empty() && !text.text.is_empty()).then(|| text.text.clone());
    div()
        .id(style.node_id("text"))
        .when_some(label, |this, label| {
            this.role(Role::Label).aria_value(label)
        })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let index = layout.index_for_position(event.position);
            // A plain click on a link leaves the block rendered.
            let on_link = matches!(index, Ok(index) if inline.link_at(index).is_some());
            if !(on_link && !event.modifiers.modified()) {
                let (Ok(index) | Err(index)) = index;
                click.report(
                    inline.source_offset(index),
                    event.modifiers.shift,
                    window,
                    cx,
                );
            }
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                let (Ok(index) | Err(index)) = drag_layout.index_for_position(event.position);
                drag.drag(drag_inline.source_offset(index), cx);
            }
        })
        .child(content)
        .children(scroll_to_reference)
        .into_any_element()
}

/// If the footnote reference the view is scrolling to is in `text`, drawn
/// with `layout`: an element that scrolls its line to the top of the
/// window as the frame is laid out.
fn scroll_to_reference(
    text: &InlineText,
    layout: &TextLayout,
    click: &ClickTarget,
) -> Option<AnyElement> {
    let target = click.reference?;
    let reference = text.links.iter().find(|link| {
        link.destination.starts_with("#fn-")
            && click.block_start + text.source_offset(link.display.start) == target.offset
    })?;
    let (layout, index) = (layout.clone(), reference.display.start);
    // After the text in the frame, so its layout has a position.
    let element = canvas(
        move |_, window, _| {
            if let Some(position) = layout.position_for_index(index) {
                // A window's height of space from just above the line: the
                // list scrolls it to fill the window, putting the line
                // near the top.
                let top = position.y - px(16.);
                let height = target.viewport_height.max(layout.line_height());
                window
                    .request_autoscroll(Bounds::new(point(position.x, top), size(px(1.), height)));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0();
    Some(element.into_any_element())
}

/// Plain text shown with tabs expanded. A click maps the clicked index back
/// through `to_source` (which takes an index into `text`).
fn clickable_source(
    text: &str,
    style: &TextStyle,
    to_source: impl Fn(usize) -> usize + 'static,
) -> AnyElement {
    let tabs = ExpandedTabs::new(text, style.tab_width);
    let shown = SharedString::from(tabs.text.clone());
    let styled = StyledText::new(shown.clone());
    let layout = styled.layout().clone();
    let click = style.click.clone();
    let to_source = Rc::new(move |shown: usize| to_source(tabs.to_source(shown)));
    let (drag_layout, drag_to_source, drag) = (layout.clone(), to_source.clone(), click.clone());
    div()
        .id(style.node_id("source"))
        .when(!shown.is_empty(), |this| {
            this.role(Role::Label).aria_value(shown)
        })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let (Ok(index) | Err(index)) = layout.index_for_position(event.position);
            click.report(to_source(index), event.modifiers.shift, window, cx);
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                let (Ok(index) | Err(index)) = drag_layout.index_for_position(event.position);
                drag.drag(drag_to_source(index), cx);
            }
        })
        .child(styled)
        .into_any_element()
}

/// Inline text with its formatting as GPUI text runs.
fn styled_text(text: &InlineText, style: &TextStyle) -> StyledText {
    let runs = text
        .segments
        .iter()
        .map(|segment| {
            inline_run(
                segment.style,
                segment.display.len(),
                &style.font,
                style.color,
                style.theme,
                &style.code_font,
            )
        })
        .collect();
    StyledText::new(SharedString::from(text.text.clone())).with_runs(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markers(markers: &[&str]) -> Vec<String> {
        markers.iter().map(|&marker| marker.to_owned()).collect()
    }

    #[test]
    fn marker_column_follows_the_list_indent() {
        let bullets = markers(&["•", "•"]);
        assert_eq!(marker_column(ListIndent::Standard, &bullets), 1.75);
        assert_eq!(marker_column(ListIndent::Compact, &bullets), 1.3125);
        assert_eq!(marker_column(ListIndent::Wide, &bullets), 2.625);
    }

    #[test]
    fn marker_column_widens_for_long_markers() {
        let short = markers(&["8.", "9."]);
        assert_eq!(marker_column(ListIndent::Standard, &short), 1.75);
        let long = markers(&["99.", "100."]);
        assert!(marker_column(ListIndent::Standard, &long) > 2.5);
        assert!(marker_column(ListIndent::Compact, &markers(&["☐"])) >= 1.5);
    }
}
