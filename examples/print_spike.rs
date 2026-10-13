//! Spike for #17 (Print): turn Kertas's rendered blocks into an
//! `NSAttributedString`, lay it out in an `NSTextView`, and print it with
//! `NSPrintOperation`, saving to a PDF so the result can be inspected.
//!
//!   cargo run --example print_spike -- input.md output.pdf
//!
//! Not part of Kertas; see docs/planning/spike-17-print.md.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSBackgroundColorAttributeName, NSColor, NSFont, NSFontAttributeName,
    NSFontManager, NSFontTraitMask, NSForegroundColorAttributeName, NSMutableParagraphStyle,
    NSParagraphStyleAttributeName, NSPrintInfo, NSPrintJobSavingURL, NSPrintOperation,
    NSPrintSaveJob, NSPrintingPaginationMode, NSStrikethroughStyleAttributeName, NSTextBlock,
    NSTextBlockDimension, NSTextBlockLayer, NSTextBlockValueType, NSTextTable, NSTextTableBlock,
    NSTextView, NSUnderlineStyleAttributeName,
};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSArray, NSMutableAttributedString, NSNumber, NSRange, NSString, NSURL};

use kertas::inline::InlineText;
use kertas::model::Model;
use kertas::rendered::RenderedBlock;

const BODY: f64 = 11.;
const CODE: f64 = 9.5;

/// How a run of text looks.
#[derive(Clone, Default)]
struct Look {
    size: Option<f64>,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    link: bool,
    muted: bool,
}

/// A paragraph's layout.
#[derive(Clone, Default)]
struct Para {
    indent: f64,
    space_before: f64,
    space_after: f64,
    block: Option<Retained<NSTextBlock>>,
}

/// Text built up as runs, with paragraph styles over ranges, then turned
/// into an attributed string (whose ranges are in UTF-16 units).
#[derive(Default)]
struct Builder {
    text: String,
    runs: Vec<(std::ops::Range<usize>, Look)>,
    paras: Vec<(std::ops::Range<usize>, Para)>,
}

impl Builder {
    fn push(&mut self, text: &str, look: Look) {
        let start = self.text.len();
        self.text.push_str(text);
        self.runs.push((start..self.text.len(), look));
    }

    /// Text inside a paragraph: its line breaks (hard breaks) stay within
    /// the paragraph, as U+2028.
    fn push_inline(&mut self, text: &str, look: Look) {
        self.push(&text.replace('\n', "\u{2028}"), look);
    }

    fn inline(&mut self, text: &InlineText, base: &Look) {
        let mut position = 0;
        for segment in &text.segments {
            if segment.display.start > position {
                self.push_inline(&text.text[position..segment.display.start], base.clone());
            }
            let style = segment.style;
            self.push_inline(
                &text.text[segment.display.clone()],
                Look {
                    bold: base.bold || style.strong,
                    italic: base.italic || style.emphasis,
                    code: base.code || style.code,
                    strike: style.strikethrough,
                    link: style.link,
                    ..base.clone()
                },
            );
            position = segment.display.end;
        }
        if position < text.text.len() {
            self.push_inline(&text.text[position..], base.clone());
        }
    }

    /// Ends a paragraph started at `start` with a newline and styles it.
    fn end_para(&mut self, start: usize, para: Para) {
        self.push("\n", Look::default());
        self.paras.push((start..self.text.len(), para));
    }

    fn block(&mut self, block: &RenderedBlock, indent: f64, look: &Look) {
        let start = self.text.len();
        let para = Para {
            indent,
            space_after: 7.,
            ..Default::default()
        };
        match block {
            RenderedBlock::Paragraph(text) => {
                self.inline(text, look);
                self.end_para(start, para);
            }
            RenderedBlock::Heading { level, text } => {
                let scale = [2.0, 1.5, 1.25, 1.0, 0.875, 0.85][(*level as usize).clamp(1, 6) - 1];
                self.inline(
                    text,
                    &Look {
                        size: Some(BODY * scale),
                        bold: true,
                        ..look.clone()
                    },
                );
                self.end_para(
                    start,
                    Para {
                        space_before: 10.,
                        ..para
                    },
                );
            }
            RenderedBlock::ThematicBreak => {
                self.push(
                    "⸻",
                    Look {
                        muted: true,
                        ..Look::default()
                    },
                );
                self.end_para(start, para);
            }
            RenderedBlock::List {
                start: first,
                items,
            } => {
                for (number, item) in items.iter().enumerate() {
                    let marker = match (item.task.map(|task| task.checked), first) {
                        (Some(true), _) => "☑".to_owned(),
                        (Some(false), _) => "☐".to_owned(),
                        (None, Some(first)) => format!("{}.", first + number as u64),
                        (None, None) => "•".to_owned(),
                    };
                    for (index, child) in item.children.iter().enumerate() {
                        if index == 0 {
                            let item_start = self.text.len();
                            self.push(&format!("{marker}  "), look.clone());
                            // Lay the first child inline after the marker.
                            let before = self.paras.len();
                            self.block(child, indent + 16., look);
                            if let Some((range, _)) = self.paras.get_mut(before) {
                                range.start = item_start;
                            }
                        } else {
                            self.block(child, indent + 16., look);
                        }
                    }
                }
            }
            RenderedBlock::BlockQuote(children) => {
                for child in children {
                    self.block(
                        child,
                        indent + 14.,
                        &Look {
                            muted: true,
                            ..look.clone()
                        },
                    );
                }
            }
            RenderedBlock::CodeBlock { code, .. } => {
                let block = NSTextBlock::new();
                block.setBackgroundColor(Some(&NSColor::colorWithWhite_alpha(0.94, 1.)));
                block.setValue_type_forDimension(
                    100.,
                    NSTextBlockValueType::PercentageValueType,
                    NSTextBlockDimension::Width,
                );
                block.setWidth_type_forLayer(
                    6.,
                    NSTextBlockValueType::AbsoluteValueType,
                    NSTextBlockLayer::Padding,
                );
                self.push(
                    &code.text.trim_end_matches('\n').replace('\n', "\u{2028}"),
                    Look {
                        code: true,
                        ..Look::default()
                    },
                );
                self.end_para(
                    start,
                    Para {
                        block: Some(block),
                        ..para
                    },
                );
            }
            RenderedBlock::Table(table) => {
                let columns = table.header.len().max(1);
                let text_table = NSTextTable::new();
                text_table.setNumberOfColumns(columns);
                text_table.setCollapsesBorders(true);
                text_table.setValue_type_forDimension(
                    100.,
                    NSTextBlockValueType::PercentageValueType,
                    NSTextBlockDimension::Width,
                );
                let rows = std::iter::once(&table.header).chain(&table.rows);
                for (row, cells) in rows.enumerate() {
                    for column in 0..columns {
                        let cell = NSTextTableBlock::initWithTable_startingRow_rowSpan_startingColumn_columnSpan(
                            NSTextTableBlock::alloc(),
                            &text_table,
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
                        cell.setBorderColor(Some(&NSColor::colorWithWhite_alpha(0.75, 1.)));
                        if row == 0 {
                            cell.setBackgroundColor(Some(&NSColor::colorWithWhite_alpha(0.96, 1.)));
                        }
                        let cell_start = self.text.len();
                        if let Some(text) = cells.get(column) {
                            self.inline(
                                text,
                                &Look {
                                    bold: row == 0,
                                    ..look.clone()
                                },
                            );
                        }
                        self.end_para(
                            cell_start,
                            Para {
                                block: Some(Retained::into_super(cell)),
                                ..Default::default()
                            },
                        );
                    }
                }
                // Space after the table.
                let after = self.text.len();
                self.end_para(after, Para::default());
            }
            RenderedBlock::Html(source) | RenderedBlock::Raw(source) => {
                self.push(
                    source.trim_end(),
                    Look {
                        code: true,
                        muted: true,
                        ..Look::default()
                    },
                );
                self.end_para(start, para);
            }
        }
    }

    fn attributed(&self, mtm: MainThreadMarker) -> Retained<NSMutableAttributedString> {
        let utf16 = |byte: usize| self.text[..byte].encode_utf16().count();
        let range = |r: &std::ops::Range<usize>| {
            let start = utf16(r.start);
            NSRange::new(start, utf16(r.end) - start)
        };
        let string = NSMutableAttributedString::initWithString(
            NSMutableAttributedString::alloc(),
            &NSString::from_str(&self.text),
        );
        let manager = NSFontManager::sharedFontManager(mtm);
        let add = |key: &objc2_foundation::NSString, value: &AnyObject, r: NSRange| unsafe {
            string.addAttribute_value_range(key, value, r);
        };
        for (r, look) in &self.runs {
            let r = range(r);
            if r.length == 0 {
                continue;
            }
            let size = look.size.unwrap_or(if look.code { CODE } else { BODY });
            let mut font = if look.code {
                NSFont::fontWithName_size(&NSString::from_str("Menlo"), size)
                    .unwrap_or_else(|| NSFont::systemFontOfSize(size))
            } else {
                NSFont::systemFontOfSize(size)
            };
            if look.bold {
                font = manager.convertFont_toHaveTrait(&font, NSFontTraitMask::BoldFontMask);
            }
            if look.italic {
                font = manager.convertFont_toHaveTrait(&font, NSFontTraitMask::ItalicFontMask);
            }
            unsafe {
                add(NSFontAttributeName, &font, r);
                let color = if look.link {
                    NSColor::colorWithSRGBRed_green_blue_alpha(0.035, 0.41, 0.85, 1.)
                } else if look.muted {
                    NSColor::colorWithWhite_alpha(0.35, 1.)
                } else {
                    NSColor::blackColor()
                };
                add(NSForegroundColorAttributeName, &color, r);
                if look.code && look.size.is_none() {
                    add(
                        NSBackgroundColorAttributeName,
                        &NSColor::colorWithWhite_alpha(0.94, 1.),
                        r,
                    );
                }
                if look.link {
                    add(NSUnderlineStyleAttributeName, &NSNumber::new_i32(1), r);
                }
                if look.strike {
                    add(NSStrikethroughStyleAttributeName, &NSNumber::new_i32(1), r);
                }
            }
        }
        for (r, para) in &self.paras {
            let style = NSMutableParagraphStyle::new();
            style.setFirstLineHeadIndent(para.indent);
            style.setHeadIndent(para.indent);
            style.setParagraphSpacing(para.space_after);
            style.setParagraphSpacingBefore(para.space_before);
            style.setLineHeightMultiple(1.15);
            if let Some(block) = &para.block {
                style.setTextBlocks(&NSArray::from_retained_slice(std::slice::from_ref(block)));
            }
            unsafe { add(NSParagraphStyleAttributeName, &style, range(r)) };
        }
        string
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("input.md");
    let output = args.next().expect("output.pdf");
    let text = std::fs::read_to_string(&input).unwrap();
    let model = Model::new(&text);

    let mut builder = Builder::default();
    for item in &model.items {
        builder.block(&item.rendered, 0., &Look::default());
    }
    let mtm = MainThreadMarker::new().unwrap();
    let attributed = builder.attributed(mtm);
    let _app = NSApplication::sharedApplication(mtm);
    let info = NSPrintInfo::sharedPrintInfo();
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticallyCentered(false);
    for set in [
        NSPrintInfo::setTopMargin,
        NSPrintInfo::setBottomMargin,
        NSPrintInfo::setLeftMargin,
        NSPrintInfo::setRightMargin,
    ] {
        set(&info, 54.);
    }
    let paper = info.paperSize();
    let width = paper.width - 108.;
    // KERTAS_TEXTKIT picks the text system, to compare them:
    // - "1" (default): TextKit 1 from the start.
    // - "2": TextKit 2, as NSTextView comes.
    // - "2-touch": TextKit 2, then ask for the NSLayoutManager after the
    //   content is set, which is the documented way to switch to TextKit 1.
    let mode = std::env::var("KERTAS_TEXTKIT").unwrap_or_else(|_| "1".into());
    let view = NSTextView::initUsingTextLayoutManager(mtm.alloc(), mode != "1");
    view.setFrameSize(CGSize::new(width, paper.height));
    let textkit = |view: &NSTextView| {
        if view.textLayoutManager().is_some() {
            2
        } else {
            1
        }
    };
    println!("mode {mode}: created with TextKit {}", textkit(&view));
    unsafe {
        view.textStorage().unwrap().setAttributedString(&attributed);
    }
    println!("after setting the text: TextKit {}", textkit(&view));
    if mode == "2-touch" {
        let _ = unsafe { view.layoutManager() };
        println!(
            "after asking for the layout manager: TextKit {}",
            textkit(&view)
        );
    }
    view.setVerticallyResizable(true);
    view.setHorizontallyResizable(false);
    view.sizeToFit();

    info.setJobDisposition(unsafe { NSPrintSaveJob });
    let url = NSURL::fileURLWithPath(&NSString::from_str(&output));
    unsafe {
        info.dictionary().setObject_forKey(
            &url,
            objc2::runtime::ProtocolObject::from_ref(NSPrintJobSavingURL),
        );
    }
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    operation.setShowsPrintPanel(false);
    operation.setShowsProgressPanel(false);
    let ok = operation.runOperation();
    println!("after printing: TextKit {}", textkit(&view));
    println!("printed: {ok} -> {output}");
}
