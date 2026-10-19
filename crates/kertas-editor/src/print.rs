//! Printing (#17), for the app and for hosts (#127): [`print()`] shows the
//! standard macOS print panel, as a sheet on a window, whose PDF menu
//! saves PDFs, and [`save_pdf`] writes a PDF without any panels. Both
//! take any document, as a [`Model`] of its text, in the light palette
//! and the fonts its [`TextSettings`] choose. macOS only.
//!
//! The document is laid out for printing by [`PrintLayout`], turned into an
//! `NSAttributedString` here, and printed from an offscreen `NSTextView`.
//! The view uses TextKit 1: TextKit 2 silently drops tables
//! (`NSTextTable`) and code boxes (`NSTextBlock`) when printing.

use std::ops::Range;

use kertas_core::links::ImageLocation;
use kertas_core::model::Model;
use kertas_core::print_layout::{Alignment, Frame, PrintLayout};
use kertas_core::rendered::heading_scale;
use raw_window_handle::HasWindowHandle;

use crate::{TextSettings, Theme};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAttachmentAttributeName, NSBackgroundColorAttributeName, NSColor, NSFont,
    NSFontAttributeName, NSFontManager, NSFontTraitMask, NSForegroundColorAttributeName, NSImage,
    NSLayoutManager, NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSPrintInfo,
    NSPrintOperation, NSPrintingPaginationMode, NSStrikethroughStyleAttributeName, NSTextAlignment,
    NSTextAttachment, NSTextBlock, NSTextBlockDimension, NSTextBlockLayer, NSTextBlockValueType,
    NSTextContainer, NSTextStorage, NSTextTable, NSTextTableBlock, NSTextView,
    NSUnderlineStyleAttributeName, NSView,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSArray, NSAttributedString, NSCopying, NSData, NSMutableAttributedString, NSNumber, NSRange,
    NSRectEdge, NSString,
};

/// Body text size, in points.
const BODY: f64 = 11.;
/// Code text size, in points.
const CODE: f64 = 9.5;
/// Page margins, in points.
const MARGIN: f64 = 54.;

/// What a host can add to how a document prints. The default prints as
/// Kertas does: one flow of text, cut into pages, without numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrintOptions {
    /// Where new pages start, as offsets in the document's source: each
    /// starts a page with the block it's in. One in the first block is
    /// ignored, so the first page is never empty.
    pub page_breaks: Vec<usize>,
    /// The first page, counting from 1, to show its number at its foot;
    /// `None` numbers no pages. The pages before it are counted, but their
    /// numbers aren't shown, as on a book's title page.
    pub numbered_from: Option<usize>,
}

impl PrintOptions {
    /// Whether the document needs laying out a page at a time.
    fn paged(&self) -> bool {
        !self.page_breaks.is_empty() || self.numbered_from.is_some()
    }
}

/// Shows the print panel for `model` as a sheet on `window`, a GPUI
/// window, laid out with `settings` and `options`, the job named `title`.
/// `document` is the document's file, which images' relative paths start
/// from. Web images print if `web_images` is set and they've been loaded
/// (#97). Does nothing off the main thread.
pub fn print(
    model: &Model,
    document: Option<&std::path::Path>,
    web_images: bool,
    settings: &TextSettings,
    options: &PrintOptions,
    title: &str,
    window: &impl HasWindowHandle,
) {
    let (Some(mtm), Some(window)) = (MainThreadMarker::new(), crate::ns_window::ns_window(window))
    else {
        return;
    };
    let operation = operation(model, document, web_images, settings, options, mtm);
    operation.setJobTitle(Some(&NSString::from_str(title)));
    // SAFETY: no delegate or callback, so nothing to keep alive.
    unsafe {
        operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
            &window,
            None,
            None,
            std::ptr::null_mut(),
        );
    }
}

/// Prints `model` to a PDF at `path` without any panels, as [`print()`]
/// does, but without web images. Returns whether it worked, which it
/// can't off the main thread.
pub fn save_pdf(
    model: &Model,
    document: Option<&std::path::Path>,
    settings: &TextSettings,
    options: &PrintOptions,
    path: &std::path::Path,
) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPrintJobSavingURL, NSPrintSaveJob};
    use objc2_foundation::NSURL;

    let operation = operation(model, document, false, settings, options, mtm);
    let info = operation.printInfo();
    // SAFETY: `NSPrintSaveJob` is an AppKit constant, set before any code
    // runs.
    info.setJobDisposition(unsafe { NSPrintSaveJob });
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    // SAFETY: the print info's dictionary takes a URL for this key.
    unsafe {
        info.dictionary()
            .setObject_forKey(&url, ProtocolObject::from_ref(NSPrintJobSavingURL));
    }
    operation.setShowsPrintPanel(false);
    operation.setShowsProgressPanel(false);
    operation.runOperation()
}

/// The page, counting from 1, that each of `offsets` in `model`'s source
/// prints on, laid out as [`save_pdf`] lays it out with `options`, say for
/// a table of contents. Empty off the main thread.
pub fn pages(
    model: &Model,
    document: Option<&std::path::Path>,
    settings: &TextSettings,
    options: &PrintOptions,
    offsets: &[usize],
) -> Vec<usize> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    let paper = print_info().paperSize();
    let prepared = Prepared::new(model, document, false, settings, options, paper, mtm);
    let pages = Pages::lay_out(&prepared.text, paper, options);
    offsets
        .iter()
        .map(|&offset| pages.page_of(prepared.char_index(model, offset)) + 1)
        .collect()
}

/// A copy of the shared print settings, with Kertas's margins.
fn print_info() -> Retained<NSPrintInfo> {
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticallyCentered(false);
    info.setTopMargin(MARGIN);
    info.setBottomMargin(MARGIN);
    info.setLeftMargin(MARGIN);
    info.setRightMargin(MARGIN);
    info
}

/// A print operation for `model`, on a copy of the shared print settings.
fn operation(
    model: &Model,
    document: Option<&std::path::Path>,
    web_images: bool,
    settings: &TextSettings,
    options: &PrintOptions,
    mtm: MainThreadMarker,
) -> Retained<NSPrintOperation> {
    let info = print_info();
    let paper = info.paperSize();
    let prepared = Prepared::new(model, document, web_images, settings, options, paper, mtm);
    if options.paged() {
        let pages = Pages::lay_out(&prepared.text, paper, options);
        let view = pages.view(paper, settings, options, mtm);
        return NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    }

    // TextKit 1, so tables and code boxes print.
    let view = NSTextView::initUsingTextLayoutManager(mtm.alloc(), false);
    view.setFrameSize(CGSize::new(paper.width - 2. * MARGIN, paper.height));
    // SAFETY: the view was just created with a text storage.
    unsafe {
        view.textStorage()
            .expect("text storage")
            .setAttributedString(&prepared.text)
    };
    view.setVerticallyResizable(true);
    view.setHorizontallyResizable(false);
    view.sizeToFit();
    NSPrintOperation::printOperationWithView_printInfo(&view, &info)
}

/// A document's text, styled for printing, with a form feed starting each
/// page break.
struct Prepared {
    text: Retained<NSMutableAttributedString>,
    layout: PrintLayout,
    /// The UTF-16 offset of each byte of the layout's text.
    utf16: Vec<usize>,
    /// Where the form feeds went, as UTF-16 offsets in the text without
    /// them, in order.
    breaks: Vec<usize>,
}

impl Prepared {
    fn new(
        model: &Model,
        document: Option<&std::path::Path>,
        web_images: bool,
        settings: &TextSettings,
        options: &PrintOptions,
        paper: CGSize,
        mtm: MainThreadMarker,
    ) -> Self {
        let web_images = if web_images {
            crate::web_images::fetched_in(model)
        } else {
            Default::default()
        };
        let layout = PrintLayout::new(
            model,
            document,
            web_images,
            settings.tab_width,
            settings.list_indent,
        );
        let utf16 = utf16_offsets(&layout.text);
        let text = attributed(&layout, &utf16, settings, paper.width - 2. * MARGIN, mtm);
        let mut breaks: Vec<usize> = options
            .page_breaks
            .iter()
            .map(|&offset| utf16[layout.text_offset(model, offset)])
            .filter(|&at| at > 0)
            .collect();
        breaks.sort_unstable();
        breaks.dedup();
        // From the end, so the offsets before each one stay as they were.
        for &at in breaks.iter().rev() {
            // The form feed takes the paragraph's attributes, as the first
            // character of a paragraph sets its style.
            let attributes = (at < text.length()).then(|| unsafe {
                text.attributesAtIndex_effectiveRange(at, std::ptr::null_mut())
            });
            let feed = NSString::from_str("\u{000C}");
            let feed = match &attributes {
                // SAFETY: the attributes came from an attributed string.
                Some(attributes) => unsafe {
                    NSAttributedString::initWithString_attributes(
                        NSAttributedString::alloc(),
                        &feed,
                        Some(attributes),
                    )
                },
                None => NSAttributedString::initWithString(NSAttributedString::alloc(), &feed),
            };
            text.insertAttributedString_atIndex(&feed, at);
        }
        Self {
            text,
            layout,
            utf16,
            breaks,
        }
    }

    /// Where the block holding `offset` in `model`'s source starts in the
    /// text, as a UTF-16 offset, after the form feed if one starts it.
    fn char_index(&self, model: &Model, offset: usize) -> usize {
        let at = self.utf16[self.layout.text_offset(model, offset)];
        at + self.breaks.partition_point(|&feed| feed <= at)
    }
}

/// The UTF-16 offset of each byte offset in `text`, and of its end.
fn utf16_offsets(text: &str) -> Vec<usize> {
    let mut utf16_at = vec![0; text.len() + 1];
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        utf16_at[byte..byte + character.len_utf8()].fill(units);
        units += character.len_utf16();
    }
    utf16_at[text.len()] = units;
    utf16_at
}

/// Text laid out a page at a time, as TextEdit lays out a page view: a
/// text container per page, sharing one layout manager, which starts a
/// new container at each form feed. TextKit 1, so tables and code boxes
/// print.
struct Pages {
    _storage: Retained<NSTextStorage>,
    manager: Retained<NSLayoutManager>,
    containers: Vec<Retained<NSTextContainer>>,
}

/// More pages than any document should print, to stop a layout that
/// never ends.
const MAX_PAGES: usize = 20_000;

/// The strip at the foot of a numbered page that holds its number.
const FOOTER: f64 = 24.;

/// The size of what a page prints inside the margins, which AppKit prints
/// a view in slices of.
fn page_size(paper: CGSize) -> CGSize {
    CGSize::new(paper.width - 2. * MARGIN, paper.height - 2. * MARGIN)
}

impl Pages {
    /// Lays `text` out on pages of `paper`'s size, inside the margins,
    /// leaving room for page numbers if `options` number any pages.
    fn lay_out(text: &NSAttributedString, paper: CGSize, options: &PrintOptions) -> Self {
        let storage = NSTextStorage::new();
        storage.setAttributedString(text);
        let manager = NSLayoutManager::new();
        storage.addLayoutManager(&manager);
        let page = page_size(paper);
        let footer = if options.numbered_from.is_some() {
            FOOTER
        } else {
            0.
        };
        let size = CGSize::new(page.width, page.height - footer);
        let glyphs = manager.numberOfGlyphs();
        let mut containers = Vec::new();
        loop {
            let container = NSTextContainer::initWithSize(NSTextContainer::alloc(), size);
            manager.addTextContainer(&container);
            let range = manager.glyphRangeForTextContainer(&container);
            containers.push(container);
            if range.end() >= glyphs || containers.len() >= MAX_PAGES {
                break;
            }
        }
        Self {
            _storage: storage,
            manager,
            containers,
        }
    }

    /// The page, from 0, holding the character at `index`.
    fn page_of(&self, index: usize) -> usize {
        let glyph = self.manager.glyphIndexForCharacterAtIndex(index);
        self.containers
            .iter()
            .position(|container| {
                let range = self.manager.glyphRangeForTextContainer(container);
                glyph < range.end()
            })
            .unwrap_or(self.containers.len().saturating_sub(1))
    }

    /// A view of the pages, one under another, each the size of a page
    /// inside its margins, so that AppKit prints each on its own sheet,
    /// with its number at its foot from the page `options` say.
    fn view(
        &self,
        paper: CGSize,
        settings: &TextSettings,
        options: &PrintOptions,
        mtm: MainThreadMarker,
    ) -> Retained<NSView> {
        let page = page_size(paper);
        let count = self.containers.len();
        let view = NSView::initWithFrame(
            mtm.alloc(),
            CGRect::new(
                CGPoint::new(0., 0.),
                CGSize::new(page.width, page.height * count as f64),
            ),
        );
        for (index, container) in self.containers.iter().enumerate() {
            // A view isn't flipped, so the first page is the highest, and
            // the text sits at the top of its page, above the footer.
            let bottom = page.height * (count - 1 - index) as f64;
            let text_size = container.size();
            let frame = CGRect::new(
                CGPoint::new(0., bottom + page.height - text_size.height),
                text_size,
            );
            let text = NSTextView::initWithFrame_textContainer(mtm.alloc(), frame, Some(container));
            text.setEditable(false);
            view.addSubview(&text);
            let number = index + 1;
            if options.numbered_from.is_some_and(|first| number >= first) {
                view.addSubview(&page_number(number, bottom, page.width, settings, mtm));
            }
        }
        view
    }
}

/// Page `number`'s number, centred across `width` at the foot of the page
/// whose bottom is at `bottom`.
fn page_number(
    number: usize,
    bottom: f64,
    width: f64,
    settings: &TextSettings,
    mtm: MainThreadMarker,
) -> Retained<NSTextView> {
    let size = BODY * 0.85;
    let frame = CGRect::new(CGPoint::new(0., bottom), CGSize::new(width, size * 1.6));
    let label = NSTextView::initWithFrame(mtm.alloc(), frame);
    label.setEditable(false);
    label.setDrawsBackground(false);
    label.setString(&NSString::from_str(&number.to_string()));
    label.setAlignment(NSTextAlignment::Center);
    let font = settings
        .body_font
        .as_deref()
        .and_then(|family| NSFont::fontWithName_size(&NSString::from_str(family), size))
        .unwrap_or_else(|| NSFont::systemFontOfSize(size));
    label.setFont(Some(&font));
    label.setTextColor(Some(&NSColor::colorWithWhite_alpha(0.35, 1.)));
    label
}

/// The colours a document prints in.
struct Palette {
    text: Retained<NSColor>,
    muted: Retained<NSColor>,
    link: Retained<NSColor>,
    theme: Theme,
    code_background: Retained<NSColor>,
    header_background: Retained<NSColor>,
    rule: Retained<NSColor>,
}

impl Palette {
    /// Black on white.
    fn light() -> Self {
        let grey = |white: f64| NSColor::colorWithWhite_alpha(white, 1.);
        Self {
            text: NSColor::blackColor(),
            muted: grey(0.35),
            link: NSColor::colorWithSRGBRed_green_blue_alpha(0.035, 0.41, 0.85, 1.),
            theme: Theme::light(),
            code_background: grey(0.94),
            header_background: grey(0.96),
            rule: grey(0.75),
        }
    }
}

/// The layout as an attributed string, in the light palette, in the fonts
/// `settings` choose, with images no wider than `width`. `utf16_at` is
/// the UTF-16 offset of each byte of the layout's text: attributed string
/// ranges are in UTF-16 code units, and mapping every byte offset in one
/// pass saves converting each range from scratch, which would be
/// quadratic in the document's length.
fn attributed(
    layout: &PrintLayout,
    utf16_at: &[usize],
    settings: &TextSettings,
    width: f64,
    mtm: MainThreadMarker,
) -> Retained<NSMutableAttributedString> {
    let palette = Palette::light();
    let text = &layout.text;
    let utf16 = |byte: usize| utf16_at[byte];
    let range = |bytes: &Range<usize>| {
        let start = utf16(bytes.start);
        NSRange::new(start, utf16(bytes.end) - start)
    };
    let string = NSMutableAttributedString::initWithString(
        NSMutableAttributedString::alloc(),
        &NSString::from_str(text),
    );
    let add = |key: &NSString, value: &AnyObject, range: NSRange| {
        // SAFETY: every value passed is the right type for its key.
        unsafe { string.addAttribute_value_range(key, value, range) };
    };
    let fonts = NSFontManager::sharedFontManager(mtm);
    let code_background = &palette.code_background;

    for run in &layout.runs {
        let range = range(&run.range);
        if range.length == 0 {
            continue;
        }
        let style = run.style;
        let size = match style.heading {
            Some(level) => BODY * heading_scale(level),
            None if style.code => CODE,
            None => BODY,
        };
        let family = if style.code {
            Some(&*settings.code_font)
        } else {
            settings.body_font.as_deref()
        };
        let mut font = family
            .and_then(|family| {
                // A regular (weight 5) font with no traits.
                fonts.fontWithFamily_traits_weight_size(
                    &NSString::from_str(family),
                    NSFontTraitMask::empty(),
                    5,
                    size,
                )
            })
            .unwrap_or_else(|| NSFont::systemFontOfSize(size));
        if style.bold {
            font = fonts.convertFont_toHaveTrait(&font, NSFontTraitMask::BoldFontMask);
        }
        if style.italic {
            font = fonts.convertFont_toHaveTrait(&font, NSFontTraitMask::ItalicFontMask);
        }
        let color = if style.link {
            palette.link.clone()
        } else if let Some(kind) = style.alert {
            let rgba = gpui::Rgba::from(palette.theme.alert(kind));
            NSColor::colorWithSRGBRed_green_blue_alpha(
                rgba.r.into(),
                rgba.g.into(),
                rgba.b.into(),
                1.,
            )
        } else if style.muted {
            palette.muted.clone()
        } else {
            palette.text.clone()
        };
        // SAFETY (here and below): reading AppKit's attribute-name constants,
        // which are set before any code runs.
        unsafe {
            add(NSFontAttributeName, &font, range);
            add(NSForegroundColorAttributeName, &color, range);
            if style.link {
                add(NSUnderlineStyleAttributeName, &NSNumber::new_i32(1), range);
            }
            if style.strikethrough {
                add(
                    NSStrikethroughStyleAttributeName,
                    &NSNumber::new_i32(1),
                    range,
                );
            }
        }
    }

    let tables: Vec<Retained<NSTextTable>> = layout
        .tables
        .iter()
        .map(|table| {
            let text_table = NSTextTable::new();
            text_table.setNumberOfColumns(table.columns);
            text_table.setCollapsesBorders(true);
            full_width(&text_table);
            text_table
        })
        .collect();
    for paragraph in &layout.paragraphs {
        let style = NSMutableParagraphStyle::new();
        style.setFirstLineHeadIndent(paragraph.indent);
        style.setHeadIndent(paragraph.indent);
        style.setParagraphSpacing(paragraph.space_after);
        style.setParagraphSpacingBefore(paragraph.space_before);
        style.setLineHeightMultiple(1.15);
        style.setAlignment(match paragraph.alignment {
            Alignment::Natural => NSTextAlignment::Natural,
            Alignment::Center => NSTextAlignment::Center,
            Alignment::Right => NSTextAlignment::Right,
        });
        let block: Option<Retained<NSTextBlock>> = match paragraph.frame {
            Frame::None => None,
            Frame::Code => {
                let block = NSTextBlock::new();
                block.setBackgroundColor(Some(code_background));
                block.setWidth_type_forLayer(
                    6.,
                    NSTextBlockValueType::AbsoluteValueType,
                    NSTextBlockLayer::Padding,
                );
                full_width(&block);
                Some(block)
            }
            Frame::Rule => {
                let block = NSTextBlock::new();
                block.setWidth_type_forLayer_edge(
                    1.,
                    NSTextBlockValueType::AbsoluteValueType,
                    NSTextBlockLayer::Border,
                    NSRectEdge::MaxY,
                );
                block.setBorderColor(Some(&palette.rule));
                full_width(&block);
                Some(block)
            }
            Frame::Cell {
                table,
                row,
                column,
                header,
            } => {
                let cell =
                    NSTextTableBlock::initWithTable_startingRow_rowSpan_startingColumn_columnSpan(
                        NSTextTableBlock::alloc(),
                        &tables[table],
                        row as isize,
                        1,
                        column as isize,
                        1,
                    );
                cell.setWidth_type_forLayer(
                    4.,
                    NSTextBlockValueType::AbsoluteValueType,
                    NSTextBlockLayer::Padding,
                );
                cell.setWidth_type_forLayer(
                    0.5,
                    NSTextBlockValueType::AbsoluteValueType,
                    NSTextBlockLayer::Border,
                );
                cell.setBorderColor(Some(&palette.rule));
                if header {
                    cell.setBackgroundColor(Some(&palette.header_background));
                }
                Some(Retained::into_super(cell))
            }
        };
        if let Some(block) = &block {
            style.setTextBlocks(&NSArray::from_retained_slice(std::slice::from_ref(block)));
        }
        unsafe {
            add(
                NSParagraphStyleAttributeName,
                &style,
                range(&paragraph.range),
            )
        };
    }

    // Inline code gets a grey background; code blocks have their box.
    for run in layout.runs.iter().filter(|run| run.style.code) {
        // Paragraphs are in order, so find the run's by binary search.
        let index = layout
            .paragraphs
            .partition_point(|paragraph| paragraph.range.end <= run.range.start);
        let in_block = layout
            .paragraphs
            .get(index)
            .is_some_and(|paragraph| paragraph.frame == Frame::Code);
        if !in_block {
            unsafe {
                add(
                    NSBackgroundColorAttributeName,
                    code_background,
                    range(&run.range),
                )
            };
        }
    }

    // Images at their natural size, shrunk to fit the page.
    for image in &layout.images {
        let picture = match &image.location {
            ImageLocation::File(path) => NSImage::initWithContentsOfFile(
                NSImage::alloc(),
                &NSString::from_str(&path.to_string_lossy()),
            ),
            ImageLocation::Web(url) => crate::web_images::fetched_image(url).and_then(|bytes| {
                NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&bytes))
            }),
        };
        let Some(picture) = picture else {
            continue;
        };
        let size = picture.size();
        if size.width <= 0. || size.height <= 0. {
            continue;
        }
        let indent = layout
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.range.contains(&image.range.start))
            .map_or(0., |paragraph| paragraph.indent);
        let scale = ((width - indent) / size.width).min(1.);
        let attachment = NSTextAttachment::new();
        attachment.setImage(Some(&picture));
        attachment.setBounds(CGRect::new(
            CGPoint::new(0., 0.),
            CGSize::new(size.width * scale, size.height * scale),
        ));
        unsafe { add(NSAttachmentAttributeName, &attachment, range(&image.range)) };
    }
    string
}

/// Makes a text block span the page.
fn full_width(block: &NSTextBlock) {
    block.setValue_type_forDimension(
        100.,
        NSTextBlockValueType::PercentageValueType,
        NSTextBlockDimension::Width,
    );
}
