//! Spike for #17: can a TextKit 2 NSTextView print a code block's box
//! without NSTextBlock (which only TextKit 1 lays out)? A custom
//! NSTextLayoutFragment draws the box, chosen by an NSTextLayoutManager
//! delegate for paragraphs marked as code.
//!
//!   cargo run --example textkit2_spike -- output.pdf
//!
//! Not part of Kertas.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, MainThreadMarker, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSBezierPath, NSColor, NSFont, NSFontAttributeName, NSGraphicsContext,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSPrintInfo, NSPrintJobSavingURL,
    NSPrintOperation, NSPrintSaveJob, NSTextElement, NSTextLayoutFragment, NSTextLayoutManager,
    NSTextLayoutManagerDelegate, NSTextLocation, NSTextParagraph, NSTextView,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGContext;
use objc2_foundation::{NSMutableAttributedString, NSRange, NSString, NSURL};

/// Marks a paragraph as part of a code block.
const CODE_KEY: &str = "KertasCodeBlock";

thread_local! {
    /// The text width, so code boxes can span it.
    static WIDTH: Cell<f64> = const { Cell::new(0.) };
}

define_class!(
    /// A layout fragment that draws a grey box behind its paragraph.
    #[unsafe(super(NSTextLayoutFragment, NSObject))]
    #[name = "KertasCodeFragment"]
    struct CodeFragment;

    impl CodeFragment {
        #[unsafe(method(renderingSurfaceBounds))]
        fn rendering_surface_bounds(&self) -> CGRect {
            code_box(self.layoutFragmentFrame())
        }

        #[unsafe(method(drawAtPoint:inContext:))]
        fn draw_at_point(&self, point: CGPoint, context: &CGContext) {
            let bounds = code_box(self.layoutFragmentFrame());
            let graphics = NSGraphicsContext::graphicsContextWithCGContext_flipped(context, true);
            NSGraphicsContext::saveGraphicsState_class();
            NSGraphicsContext::setCurrentContext(Some(&graphics));
            NSColor::colorWithWhite_alpha(0.94, 1.).setFill();
            NSBezierPath::fillRect(CGRect::new(
                CGPoint::new(point.x + bounds.origin.x, point.y + bounds.origin.y),
                bounds.size,
            ));
            NSGraphicsContext::restoreGraphicsState_class();
            unsafe {
                let _: () = msg_send![super(self), drawAtPoint: point, inContext: context];
            }
        }
    }
);

define_class!(
    /// Gives code paragraphs a CodeFragment.
    #[unsafe(super(NSObject))]
    #[name = "KertasLayoutDelegate"]
    struct LayoutDelegate;

    unsafe impl NSObjectProtocol for LayoutDelegate {}

    unsafe impl NSTextLayoutManagerDelegate for LayoutDelegate {
        #[unsafe(method_id(textLayoutManager:textLayoutFragmentForLocation:inTextElement:))]
        fn fragment(
            &self,
            _manager: &NSTextLayoutManager,
            _location: &ProtocolObject<dyn NSTextLocation>,
            element: &NSTextElement,
        ) -> Retained<NSTextLayoutFragment> {
            let range = element.elementRange();
            let is_code = element
                .downcast_ref::<NSTextParagraph>()
                .is_some_and(|paragraph| {
                    let text = paragraph.attributedString();
                    text.length() > 0
                        && unsafe {
                            text.attribute_atIndex_effectiveRange(
                                &NSString::from_str(CODE_KEY),
                                0,
                                std::ptr::null_mut(),
                            )
                        }
                        .is_some()
                });
            if is_code {
                let fragment: Retained<CodeFragment> = unsafe {
                    msg_send![CodeFragment::alloc(), initWithTextElement: element, range: range.as_deref()]
                };
                Retained::into_super(fragment)
            } else {
                NSTextLayoutFragment::initWithTextElement_range(
                    NSTextLayoutFragment::alloc(),
                    element,
                    range.as_deref(),
                )
            }
        }
    }
);

fn main() {
    let output = std::env::args().nth(1).expect("output.pdf");
    let mtm = MainThreadMarker::new().unwrap();
    let _app = NSApplication::sharedApplication(mtm);

    let text = "Code block, drawn by a custom TextKit 2 layout fragment:\n\
                fn main() {\n    println!(\"hello\");\n}\n\
                And a paragraph after it.\n";
    let string = NSMutableAttributedString::initWithString(
        NSMutableAttributedString::alloc(),
        &NSString::from_str(text),
    );
    let all = NSRange::new(0, string.length());
    let code_start = text.find("fn main").unwrap();
    let code_end = text.find("And a").unwrap();
    let code = NSRange::new(code_start, code_end - code_start);
    unsafe {
        string.addAttribute_value_range(NSFontAttributeName, &NSFont::systemFontOfSize(12.), all);
        let menlo = NSFont::fontWithName_size(&NSString::from_str("Menlo"), 10.).unwrap();
        string.addAttribute_value_range(NSFontAttributeName, &menlo, code);
        string.addAttribute_value_range(
            &NSString::from_str(CODE_KEY),
            &objc2_foundation::NSNumber::new_bool(true),
            code,
        );
        let style = NSMutableParagraphStyle::new();
        style.setParagraphSpacing(8.);
        string.addAttribute_value_range(NSParagraphStyleAttributeName, &style, all);
    }

    let info = NSPrintInfo::sharedPrintInfo();
    let width = info.paperSize().width - 108.;
    WIDTH.set(width);
    let view = NSTextView::initUsingTextLayoutManager(mtm.alloc(), true);
    view.setFrameSize(CGSize::new(width, 400.));
    let delegate: Retained<LayoutDelegate> = unsafe { msg_send![LayoutDelegate::alloc(), init] };
    let manager = view.textLayoutManager().expect("TextKit 2");
    manager.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    unsafe { view.textStorage().unwrap().setAttributedString(&string) };
    println!(
        "TextKit {}",
        if view.textLayoutManager().is_some() {
            2
        } else {
            1
        }
    );

    info.setJobDisposition(unsafe { NSPrintSaveJob });
    let url = NSURL::fileURLWithPath(&NSString::from_str(&output));
    unsafe {
        info.dictionary()
            .setObject_forKey(&url, ProtocolObject::from_ref(NSPrintJobSavingURL));
    }
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    operation.setShowsPrintPanel(false);
    operation.setShowsProgressPanel(false);
    println!("printed: {}", operation.runOperation());
    println!(
        "still TextKit {}",
        if view.textLayoutManager().is_some() {
            2
        } else {
            1
        }
    );
}

/// A code fragment's box, relative to the fragment: across the whole text
/// width, with a little padding.
fn code_box(frame: CGRect) -> CGRect {
    CGRect::new(
        CGPoint::new(-frame.origin.x - 4., -2.),
        CGSize::new(WIDTH.get() + 8., frame.size.height + 4.),
    )
}
