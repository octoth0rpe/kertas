//! Converts a text block's Markdown into display text with inline styles.
//!
//! The result keeps a source mapping: each segment of display text records
//! the source range it came from, so positions in the rendered text can be
//! mapped back to the source (needed for caret placement).

use std::ops::Range;

use pulldown_cmark::{Event, Tag, TagEnd};

#[cfg(test)]
use crate::definitions::Definitions;

/// Inline formatting applied to a segment of text.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InlineStyle {
    pub strong: bool,
    pub emphasis: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link: bool,
}

/// A run of display text with one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Byte range in [`InlineText::text`].
    pub display: Range<usize>,
    /// Byte range in the block's source that produced this text.
    pub source: Range<usize>,
    pub style: InlineStyle,
}

/// A block's text as displayed: Markdown syntax removed, formatting kept as
/// styled segments that cover the whole text in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineText {
    pub text: String,
    pub segments: Vec<Segment>,
}

impl InlineText {
    /// Maps a byte index in [`Self::text`] to a byte offset in the source.
    ///
    /// Inside a segment whose display text matches its source byte for byte,
    /// the mapping is exact. Otherwise (escapes, entities, line breaks) the
    /// index snaps to the segment's source start or end. An index between
    /// segments, or past the end, maps to the nearest segment edge.
    pub fn source_offset(&self, index: usize) -> usize {
        let Some(segment) = self
            .segments
            .iter()
            .find(|segment| index < segment.display.end)
            .or(self.segments.last())
        else {
            return 0;
        };
        if index <= segment.display.start {
            segment.source.start
        } else if index >= segment.display.end {
            segment.source.end
        } else if segment.display.len() == segment.source.len() {
            segment.source.start + (index - segment.display.start)
        } else {
            segment.source.start
        }
    }
}

/// Renders the inline content of a paragraph or heading. `definitions` are
/// the document's link reference definitions, so reference links like
/// `[text][label]` resolve. A test helper: the app renders whole blocks
/// (see [`crate::rendered::render_block`]).
#[cfg(test)]
pub(crate) fn render_inline(source: &str, definitions: &Definitions) -> InlineText {
    let mut builder = InlineBuilder::default();
    for (event, range) in definitions.parse(source) {
        // Stop at the end of the paragraph or heading.
        if matches!(event, Event::End(TagEnd::Paragraph | TagEnd::Heading(_))) {
            break;
        }
        builder.handle(&event, range);
    }
    builder.finish()
}

/// Accumulates inline events into an [`InlineText`].
#[derive(Debug, Default)]
pub(crate) struct InlineBuilder {
    output: InlineText,
    strong: usize,
    emphasis: usize,
    strikethrough: usize,
    link: usize,
}

impl InlineBuilder {
    /// Applies an inline event. Returns false for events that aren't
    /// inline, which the caller should handle.
    pub(crate) fn handle(&mut self, event: &Event, range: Range<usize>) -> bool {
        match event {
            Event::Text(text) | Event::InlineHtml(text) => self.push(text, range, false),
            Event::Code(text) => self.push(text, range, true),
            Event::SoftBreak => self.push(" ", range, false),
            Event::HardBreak => self.push("\n", range, false),
            Event::Start(Tag::Emphasis) => self.emphasis += 1,
            Event::End(TagEnd::Emphasis) => self.emphasis -= 1,
            Event::Start(Tag::Strong) => self.strong += 1,
            Event::End(TagEnd::Strong) => self.strong -= 1,
            Event::Start(Tag::Strikethrough) => self.strikethrough += 1,
            Event::End(TagEnd::Strikethrough) => self.strikethrough -= 1,
            Event::Start(Tag::Link { .. }) => self.link += 1,
            Event::End(TagEnd::Link) => self.link -= 1,
            // Image alt text arrives as text events and is shown as plain
            // text until images are supported.
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => {}
            _ => return false,
        }
        true
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.output.text.is_empty()
    }

    fn push(&mut self, text: &str, source: Range<usize>, code: bool) {
        if text.is_empty() {
            return;
        }
        let style = InlineStyle {
            strong: self.strong > 0,
            emphasis: self.emphasis > 0,
            strikethrough: self.strikethrough > 0,
            link: self.link > 0,
            code,
        };
        let start = self.output.text.len();
        self.output.text.push_str(text);
        let display = start..self.output.text.len();

        // Merge with the previous segment when the style matches and the
        // sources are contiguous, e.g. text split around an escape.
        if let Some(last) = self.output.segments.last_mut()
            && last.style == style
            && last.source.end == source.start
            && last.display.len() == last.source.len()
            && display.len() == source.len()
        {
            last.display.end = display.end;
            last.source.end = source.end;
            return;
        }
        self.output.segments.push(Segment {
            display,
            source,
            style,
        });
    }

    pub(crate) fn finish(self) -> InlineText {
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: InlineStyle = InlineStyle {
        strong: false,
        emphasis: false,
        strikethrough: false,
        code: false,
        link: false,
    };

    /// The display text of each segment, with its style.
    fn segments(inline: &InlineText) -> Vec<(&str, InlineStyle)> {
        inline
            .segments
            .iter()
            .map(|segment| (&inline.text[segment.display.clone()], segment.style))
            .collect()
    }

    #[test]
    fn plain_paragraph() {
        let inline = render_inline("Just text.", &Definitions::default());
        assert_eq!(inline.text, "Just text.");
        assert_eq!(segments(&inline), [("Just text.", PLAIN)]);
        assert_eq!(inline.segments[0].source, 0..10);
    }

    #[test]
    fn heading_text_drops_markers() {
        assert_eq!(
            render_inline("## Title", &Definitions::default()).text,
            "Title"
        );
        assert_eq!(
            render_inline("Title\n=====", &Definitions::default()).text,
            "Title"
        );
    }

    #[test]
    fn styles() {
        let inline = render_inline(
            "a **b** *c* ~~d~~ `e` [f](http://x)",
            &Definitions::default(),
        );
        assert_eq!(inline.text, "a b c d e f");
        let strong = InlineStyle {
            strong: true,
            ..PLAIN
        };
        let emphasis = InlineStyle {
            emphasis: true,
            ..PLAIN
        };
        let strike = InlineStyle {
            strikethrough: true,
            ..PLAIN
        };
        let code = InlineStyle {
            code: true,
            ..PLAIN
        };
        let link = InlineStyle {
            link: true,
            ..PLAIN
        };
        assert_eq!(
            segments(&inline),
            [
                ("a ", PLAIN),
                ("b", strong),
                (" ", PLAIN),
                ("c", emphasis),
                (" ", PLAIN),
                ("d", strike),
                (" ", PLAIN),
                ("e", code),
                (" ", PLAIN),
                ("f", link),
            ]
        );
    }

    #[test]
    fn nested_styles_combine() {
        let inline = render_inline(
            "***both*** and [**bold link**](/x)",
            &Definitions::default(),
        );
        let both = InlineStyle {
            strong: true,
            emphasis: true,
            ..PLAIN
        };
        let bold_link = InlineStyle {
            strong: true,
            link: true,
            ..PLAIN
        };
        assert_eq!(
            segments(&inline),
            [("both", both), (" and ", PLAIN), ("bold link", bold_link)]
        );
    }

    #[test]
    fn soft_break_becomes_space_and_hard_break_newline() {
        assert_eq!(
            render_inline("one\ntwo", &Definitions::default()).text,
            "one two"
        );
        assert_eq!(
            render_inline("one  \ntwo", &Definitions::default()).text,
            "one\ntwo"
        );
        assert_eq!(
            render_inline("one\\\ntwo", &Definitions::default()).text,
            "one\ntwo"
        );
    }

    #[test]
    fn source_ranges_point_at_source_text() {
        let source = "Hello **world**";
        let inline = render_inline(source, &Definitions::default());
        for segment in &inline.segments {
            assert_eq!(
                &inline.text[segment.display.clone()],
                &source[segment.source.clone()]
            );
        }
        assert_eq!(inline.segments[1].source, 8..13);
    }

    #[test]
    fn escapes_and_entities_are_unescaped() {
        let inline = render_inline(r"\*not emphasis\* &amp; done", &Definitions::default());
        assert_eq!(inline.text, "*not emphasis* & done");
    }

    #[test]
    fn reference_links_resolve_with_definitions() {
        let source = "See [kami][k].";
        assert_eq!(
            render_inline(source, &Definitions::default()).text,
            "See [kami][k]."
        );

        let inline = render_inline(source, &Definitions::from_text("[k]: https://example.com"));
        assert_eq!(inline.text, "See kami.");
        assert_eq!(
            segments(&inline),
            [
                ("See ", PLAIN),
                (
                    "kami",
                    InlineStyle {
                        link: true,
                        ..PLAIN
                    }
                ),
                (".", PLAIN),
            ]
        );
    }

    #[test]
    fn definitions_do_not_add_text() {
        let inline = render_inline("Text", &Definitions::from_text("[a]: /a\n[b]: /b"));
        assert_eq!(inline.text, "Text");
    }

    #[test]
    fn inline_html_is_shown_as_text() {
        assert_eq!(
            render_inline("a <b>bold</b> c", &Definitions::default()).text,
            "a <b>bold</b> c"
        );
    }

    #[test]
    fn non_ascii() {
        let inline = render_inline("**見出し** café 🦀", &Definitions::default());
        assert_eq!(inline.text, "見出し café 🦀");
    }

    #[test]
    fn source_offset_maps_display_to_source() {
        // "Hi **bold** \\*x" displays as "Hi bold *x".
        let inline = render_inline("Hi **bold** \\*x", &Definitions::default());
        assert_eq!(inline.text, "Hi bold *x");
        assert_eq!(inline.source_offset(0), 0);
        assert_eq!(inline.source_offset(2), 2, "end of `Hi `'s first word");
        assert_eq!(inline.source_offset(3), 5, "start of bold text, after `**`");
        assert_eq!(inline.source_offset(5), 7);
        assert_eq!(
            inline.source_offset(7),
            11,
            "just after bold text maps past the closing `**`"
        );
        assert_eq!(
            inline.source_offset(8),
            13,
            "the escaped `*`, after its backslash"
        );
        assert_eq!(inline.source_offset(10), 15, "end of text");
        assert_eq!(inline.source_offset(99), 15, "past the end");
    }

    #[test]
    fn source_offset_of_empty_text() {
        assert_eq!(InlineText::default().source_offset(0), 0);
    }

    #[test]
    fn empty_heading() {
        assert_eq!(
            render_inline("#", &Definitions::default()),
            InlineText::default()
        );
    }

    // Edge cases (#38).

    #[test]
    fn images_show_their_alt_text() {
        let inline = render_inline("See ![a diagram](d.png).", &Definitions::default());
        assert_eq!(inline.text, "See a diagram.");
    }

    #[test]
    fn autolinks_and_titles() {
        let inline = render_inline("<https://example.com>", &Definitions::default());
        assert_eq!(inline.text, "https://example.com");
        assert!(inline.segments.iter().all(|segment| segment.style.link));
        let inline = render_inline("[a](/x \"Title\")", &Definitions::default());
        assert_eq!(inline.text, "a");
        assert!(inline.segments[0].style.link);
    }

    #[test]
    fn entities_map_back_to_their_source() {
        let source = "a &amp; b";
        let inline = render_inline(source, &Definitions::default());
        assert_eq!(inline.text, "a & b");
        // The "&" maps to the start of "&amp;", and the text after it lines
        // up again.
        assert_eq!(inline.source_offset(2), 2);
        assert_eq!(inline.source_offset(4), 8);
    }

    #[test]
    fn inline_html_inside_emphasis_is_kept_as_text() {
        let inline = render_inline("*a <b>c</b>*", &Definitions::default());
        assert_eq!(inline.text, "a <b>c</b>");
        assert!(inline.segments.iter().all(|segment| segment.style.emphasis));
    }
}
