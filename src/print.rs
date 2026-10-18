//! Printing (#17): File > Print… shows the standard macOS print panel, as a
//! sheet on the document's window. Its PDF menu saves PDFs.
//!
//! The document is laid out for printing by [`PrintLayout`], turned into an
//! `NSAttributedString` here, and printed from an offscreen `NSTextView`.
//! The view uses TextKit 1: TextKit 2 silently drops tables
//! (`NSTextTable`) and code boxes (`NSTextBlock`) when printing.

use std::ops::Range;

use crate::theme::Theme;
use kertas::links::ImageLocation;
use kertas::model::Model;
use kertas::print_layout::{Alignment, Frame, PrintLayout};
use kertas::rendered::heading_scale;
use kertas::settings::Settings;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAttachmentAttributeName, NSBackgroundColorAttributeName, NSColor, NSFont,
    NSFontAttributeName, NSFontManager, NSFontTraitMask, NSForegroundColorAttributeName, NSImage,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSPrintInfo, NSPrintOperation,
    NSPrintingPaginationMode, NSStrikethroughStyleAttributeName, NSTextAlignment, NSTextAttachment,
    NSTextBlock, NSTextBlockDimension, NSTextBlockLayer, NSTextBlockValueType, NSTextTable,
    NSTextTableBlock, NSTextView, NSUnderlineStyleAttributeName, NSWindow,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSArray, NSCopying, NSData, NSMutableAttributedString, NSNumber, NSRange, NSRectEdge, NSString,
};

/// Body text size, in points.
const BODY: f64 = 11.;
/// Code text size, in points.
const CODE: f64 = 9.5;
/// Page margins, in points.
const MARGIN: f64 = 54.;

/// Shows the print panel for `model` as a sheet on `window`, laid out with
/// `settings`. Web images print if `web_images` is set and they've been
/// loaded (#97).
pub fn print(
    model: &Model,
    document: Option<&std::path::Path>,
    web_images: bool,
    settings: &Settings,
    title: &str,
    window: &NSWindow,
    mtm: MainThreadMarker,
) {
    let operation = operation(model, document, web_images, settings, mtm);
    operation.setJobTitle(Some(&NSString::from_str(title)));
    // SAFETY: no delegate or callback, so nothing to keep alive.
    unsafe {
        operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
            window,
            None,
            None,
            std::ptr::null_mut(),
        );
    }
}

/// Prints `model` to a PDF at `path` without any panels, for `kt
/// --convert` and the smoke test. Returns whether it worked.
pub fn save_pdf(
    model: &Model,
    document: Option<&std::path::Path>,
    settings: &Settings,
    path: &std::path::Path,
    mtm: MainThreadMarker,
) -> bool {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPrintJobSavingURL, NSPrintSaveJob};
    use objc2_foundation::NSURL;

    let operation = operation(model, document, false, settings, mtm);
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

/// A print operation for `model`, on a copy of the shared print settings.
fn operation(
    model: &Model,
    document: Option<&std::path::Path>,
    web_images: bool,
    settings: &Settings,
    mtm: MainThreadMarker,
) -> Retained<NSPrintOperation> {
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticallyCentered(false);
    info.setTopMargin(MARGIN);
    info.setBottomMargin(MARGIN);
    info.setLeftMargin(MARGIN);
    info.setRightMargin(MARGIN);
    let paper = info.paperSize();

    // TextKit 1, so tables and code boxes print.
    let view = NSTextView::initUsingTextLayoutManager(mtm.alloc(), false);
    view.setFrameSize(CGSize::new(paper.width - 2. * MARGIN, paper.height));
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
    let text = attributed(&layout, settings, paper.width - 2. * MARGIN, mtm);
    // SAFETY: the view was just created with a text storage.
    unsafe {
        view.textStorage()
            .expect("text storage")
            .setAttributedString(&text)
    };
    view.setVerticallyResizable(true);
    view.setHorizontallyResizable(false);
    view.sizeToFit();
    NSPrintOperation::printOperationWithView_printInfo(&view, &info)
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
/// `settings` choose, with images no wider than `width`.
fn attributed(
    layout: &PrintLayout,
    settings: &Settings,
    width: f64,
    mtm: MainThreadMarker,
) -> Retained<NSMutableAttributedString> {
    let palette = Palette::light();
    let text = &layout.text;
    // Attributed string ranges are in UTF-16 code units. Map every byte
    // offset in one pass; converting each range from scratch would be
    // quadratic in the document's length.
    let mut utf16_at = vec![0; text.len() + 1];
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        utf16_at[byte..byte + character.len_utf8()].fill(units);
        units += character.len_utf16();
    }
    utf16_at[text.len()] = units;
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
            Some(settings.code_font())
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
