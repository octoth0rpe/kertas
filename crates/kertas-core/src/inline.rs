//! Converts a text block's Markdown into display text with inline styles.
//!
//! The result keeps a source mapping: each segment of display text records
//! the source range it came from, so positions in the rendered text can be
//! mapped back to the source (needed for caret placement).

use std::ops::Range;

use pulldown_cmark::{Event, LinkType, Tag, TagEnd};

use crate::autolinks;
use crate::html_tags;

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

/// A link in display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Byte range in [`InlineText::text`].
    pub display: Range<usize>,
    /// Where the link points, as written (reference links resolved).
    pub destination: String,
}

/// An image in display text. Its alt text is in the display text, so the
/// text still reads where the image can't be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The alt text's byte range in [`InlineText::text`]; empty if it has
    /// none.
    pub display: Range<usize>,
    /// The image's path or URL, as written (reference images resolved).
    pub destination: String,
}

/// A block's text as displayed: Markdown syntax removed, formatting kept as
/// styled segments that cover the whole text in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineText {
    pub text: String,
    pub segments: Vec<Segment>,
    /// The links in the text, in order.
    pub links: Vec<Link>,
    /// The images in the text, in order.
    pub images: Vec<Image>,
    /// The names `#name` links can use to reach this text: the `id`s in its
    /// HTML tags, and `<a name>`s (#96).
    pub anchors: Vec<String>,
}

impl InlineText {
    /// The link whose text includes byte `index` of [`Self::text`].
    pub fn link_at(&self, index: usize) -> Option<&Link> {
        self.links.iter().find(|link| link.display.contains(&index))
    }

    /// The link around `image`, if it's wrapped in one
    /// (`[![alt](pic.png)](https://…)`).
    pub fn link_around(&self, image: &Image) -> Option<&Link> {
        self.links.iter().find(|link| {
            link.display.start <= image.display.start && image.display.end <= link.display.end
        })
    }

    /// Adds back-links after the text: one `↩` per reference to the note
    /// with `label`, the second and later numbered (`↩²`), as on GitHub
    /// (#90). They take up no source, so a click on them places the caret
    /// at the text's end.
    pub fn push_back_links(&mut self, label: &str, references: usize) {
        let end = self.segments.last().map_or(0, |segment| segment.source.end);
        for reference in 1..=references {
            if !self.text.is_empty() {
                let start = self.text.len();
                self.text.push(' ');
                self.segments.push(Segment {
                    display: start..self.text.len(),
                    source: end..end,
                    style: InlineStyle::default(),
                });
            }
            let start = self.text.len();
            self.text.push('↩');
            if reference > 1 {
                self.text.push_str(&superscript(reference));
            }
            let display = start..self.text.len();
            self.segments.push(Segment {
                display: display.clone(),
                source: end..end,
                style: InlineStyle {
                    link: true,
                    ..InlineStyle::default()
                },
            });
            self.links.push(Link {
                display,
                destination: back_link_destination(label, reference),
            });
        }
    }

    /// The part of the text in `range`, with its segments and links cut to
    /// fit and no images. Source ranges are unchanged, so offsets still map
    /// to the block's source.
    pub fn slice(&self, range: Range<usize>) -> InlineText {
        let clip = |display: &Range<usize>| {
            let start = display.start.max(range.start);
            let end = display.end.min(range.end);
            (start < end).then(|| start - range.start..end - range.start)
        };
        let segments = self
            .segments
            .iter()
            .filter_map(|segment| {
                let display = clip(&segment.display)?;
                // Exact when the text matches its source byte for byte;
                // otherwise the whole segment's source is kept.
                let source = if segment.display.len() == segment.source.len() {
                    let start = segment.source.start + (display.start + range.start)
                        - segment.display.start;
                    start..start + display.len()
                } else {
                    segment.source.clone()
                };
                Some(Segment {
                    display,
                    source,
                    style: segment.style,
                })
            })
            .collect();
        let links = self
            .links
            .iter()
            .filter_map(|link| {
                Some(Link {
                    display: clip(&link.display)?,
                    destination: link.destination.clone(),
                })
            })
            .collect();
        InlineText {
            text: self.text[range].to_owned(),
            segments,
            links,
            images: Vec::new(),
            anchors: Vec::new(),
        }
    }

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
    let text = definitions.prepare(source);
    for (event, range) in definitions.parse(&text, source.len()) {
        // Stop at the end of the paragraph or heading.
        if matches!(event, Event::End(TagEnd::Paragraph | TagEnd::Heading(_))) {
            break;
        }
        if let Event::FootnoteReference(label) = &event
            && let Some(number) = definitions.footnote_number(label)
        {
            builder.footnote_reference(label, number, range);
            continue;
        }
        builder.handle(&event, range);
    }
    builder.finish()
}

/// `number` in superscript digits, as a footnote reference shows it.
pub fn superscript(number: usize) -> String {
    number
        .to_string()
        .chars()
        .map(|digit| match digit {
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            digit => char::from_u32(0x2070 + digit.to_digit(10).unwrap_or(0)).unwrap_or(digit),
        })
        .collect()
}

/// Where a note's back-link to its `reference`th reference (counting from
/// 1) leads: `fnref-label`, then `fnref-label-2` and so on, as on GitHub.
pub fn back_link_destination(label: &str, reference: usize) -> String {
    let label = crate::blocks::normalize_label(label);
    match reference {
        1 => format!("#fnref-{label}"),
        n => format!("#fnref-{label}-{n}"),
    }
}

/// Where a reference to the footnote `label` leads: the note, which has
/// the anchor `fn-label`, as on GitHub.
pub fn footnote_destination(label: &str) -> String {
    format!("#fn-{}", crate::blocks::normalize_label(label))
}

/// Accumulates inline events into an [`InlineText`].
#[derive(Debug, Default)]
pub(crate) struct InlineBuilder {
    output: InlineText,
    strong: usize,
    emphasis: usize,
    strikethrough: usize,
    /// The links being built: where each one's text starts, and its
    /// destination.
    links: Vec<(usize, String)>,
    /// The images being built, likewise.
    images: Vec<(usize, String)>,
    /// How many hidden `<a id>` tags are waiting for their `</a>`.
    hidden_anchors: usize,
}

impl InlineBuilder {
    /// Applies an inline event. Returns false for events that aren't
    /// inline, which the caller should handle.
    pub(crate) fn handle(&mut self, event: &Event, range: Range<usize>) -> bool {
        match event {
            Event::Text(text) => self.push(text, range, false),
            Event::InlineHtml(html) => self.inline_html(html, range),
            Event::Code(text) => self.push(text, range, true),
            // Nothing shows before it when it follows a hidden
            // `<a id="x"></a>` line.
            Event::SoftBreak if self.output.text.is_empty() => {}
            Event::SoftBreak => self.push(" ", range, false),
            Event::HardBreak => self.push("\n", range, false),
            Event::Start(Tag::Emphasis) => self.emphasis += 1,
            Event::End(TagEnd::Emphasis) => self.emphasis -= 1,
            Event::Start(Tag::Strong) => self.strong += 1,
            Event::End(TagEnd::Strong) => self.strong -= 1,
            Event::Start(Tag::Strikethrough) => self.strikethrough += 1,
            Event::End(TagEnd::Strikethrough) => self.strikethrough -= 1,
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                // Email autolinks (`<me@example.com>`) arrive without
                // their scheme.
                let destination = match link_type {
                    LinkType::Email => format!("mailto:{dest_url}"),
                    _ => dest_url.to_string(),
                };
                self.links.push((self.output.text.len(), destination));
            }
            Event::End(TagEnd::Link) => {
                if let Some((start, destination)) = self.links.pop() {
                    let end = self.output.text.len();
                    // An image with no alt text has no text, but can still
                    // be clicked.
                    let has_image = self
                        .output
                        .images
                        .last()
                        .is_some_and(|image| image.display.start >= start);
                    if start < end || has_image {
                        self.output.links.push(Link {
                            display: start..end,
                            destination,
                        });
                    }
                }
            }
            // Alt text arrives as text events, and is kept as text.
            Event::Start(Tag::Image { dest_url, .. }) => {
                self.images
                    .push((self.output.text.len(), dest_url.to_string()));
            }
            Event::End(TagEnd::Image) => {
                if let Some((start, destination)) = self.images.pop() {
                    self.output.images.push(Image {
                        display: start..self.output.text.len(),
                        destination,
                    });
                }
            }
            _ => return false,
        }
        true
    }

    /// A reference to footnote `number`, shown as the number in
    /// superscript, which links to the note (#90).
    pub(crate) fn footnote_reference(&mut self, label: &str, number: usize, range: Range<usize>) {
        let start = self.output.text.len();
        self.links.push((start, footnote_destination(label)));
        self.push(&superscript(number), range, false);
        self.handle(&Event::End(TagEnd::Link), 0..0);
    }

    /// An inline HTML tag, shown as its source, except for `<a id>` tags
    /// that only mark a link target: those, and their `</a>`, are hidden.
    fn inline_html(&mut self, html: &str, range: Range<usize>) {
        let trimmed = html.trim();
        if self.hidden_anchors > 0 && trimmed.eq_ignore_ascii_case("</a>") {
            self.hidden_anchors -= 1;
            return;
        }
        let tags = html_tags::start_tags(html);
        self.output.anchors.extend(
            tags.iter()
                .filter_map(|tag| tag.anchor().map(str::to_owned)),
        );
        if let [tag] = tags.as_slice()
            && tag.is_target_only()
            && trimmed.starts_with('<')
            && trimmed.ends_with('>')
        {
            if !trimmed.ends_with("/>") {
                self.hidden_anchors += 1;
            }
            return;
        }
        self.push(html, range, false);
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
            link: !self.links.is_empty(),
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

    /// The finished text, with bare URLs and email addresses in plain
    /// text made into links (#93).
    pub(crate) fn finish(mut self) -> InlineText {
        self.add_autolinks();
        self.output.links.sort_by_key(|link| link.display.start);
        self.output
    }

    /// The finished text with no links added, for code blocks.
    pub(crate) fn finish_code(self) -> InlineText {
        self.output
    }

    /// Finds bare links in each run of text that isn't code or already a
    /// link, and makes them links. The source is unchanged.
    fn add_autolinks(&mut self) {
        let plain = |segment: &Segment| !segment.style.code && !segment.style.link;
        let mut found = Vec::new();
        let mut index = 0;
        let segments = &self.output.segments;
        while index < segments.len() {
            if !plain(&segments[index]) {
                index += 1;
                continue;
            }
            let start = segments[index].display.start;
            while index < segments.len() && plain(&segments[index]) {
                index += 1;
            }
            let end = segments[index - 1].display.end;
            found.extend(
                autolinks::find(&self.output.text[start..end])
                    .into_iter()
                    .map(|link| {
                        (
                            start + link.range.start..start + link.range.end,
                            link.destination,
                        )
                    }),
            );
        }
        // Alt text stands in for its image, so it isn't searched.
        let images: Vec<Range<usize>> = self
            .output
            .images
            .iter()
            .map(|image| image.display.clone())
            .collect();
        found.retain(|(display, _)| {
            !images
                .iter()
                .any(|image| image.start < display.end && display.start < image.end)
        });
        for (display, destination) in found {
            self.split_at(display.start);
            self.split_at(display.end);
            for segment in &mut self.output.segments {
                if display.start <= segment.display.start && segment.display.end <= display.end {
                    segment.style.link = true;
                }
            }
            self.output.links.push(Link {
                display,
                destination,
            });
        }
    }

    /// Splits the segment that `index` falls inside, so a segment starts
    /// there.
    fn split_at(&mut self, index: usize) {
        let Some(position) = self
            .output
            .segments
            .iter()
            .position(|segment| segment.display.start < index && index < segment.display.end)
        else {
            return;
        };
        let segment = &mut self.output.segments[position];
        let offset = index - segment.display.start;
        // Exact when the text matches its source byte for byte; otherwise
        // (e.g. an escape) the source stays with the first part.
        let source_split = if segment.display.len() == segment.source.len() {
            segment.source.start + offset
        } else {
            segment.source.end
        };
        let second = Segment {
            display: index..segment.display.end,
            source: source_split..segment.source.end,
            style: segment.style,
        };
        segment.display.end = index;
        segment.source.end = source_split;
        self.output.segments.insert(position + 1, second);
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
        let source = "See [kertas][k].";
        assert_eq!(
            render_inline(source, &Definitions::default()).text,
            "See [kertas][k]."
        );

        let inline = render_inline(source, &Definitions::from_text("[k]: https://example.com"));
        assert_eq!(inline.text, "See kertas.");
        assert_eq!(
            segments(&inline),
            [
                ("See ", PLAIN),
                (
                    "kertas",
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
    fn links_record_their_destinations() {
        let inline = render_inline(
            "See [the guide](docs/guide.md#setup), <https://example.com> and \
             <me@example.com>.",
            &Definitions::default(),
        );
        let links: Vec<(&str, &str)> = inline
            .links
            .iter()
            .map(|link| {
                (
                    &inline.text[link.display.clone()],
                    link.destination.as_str(),
                )
            })
            .collect();
        assert_eq!(
            links,
            [
                ("the guide", "docs/guide.md#setup"),
                ("https://example.com", "https://example.com"),
                ("me@example.com", "mailto:me@example.com"),
            ]
        );
        assert_eq!(
            inline.link_at(4).map(|link| link.display.clone()),
            Some(4..13)
        );
        assert!(inline.link_at(3).is_none());
        assert!(inline.link_at(13).is_none());
    }

    #[test]
    fn reference_links_record_resolved_destinations() {
        let inline = render_inline(
            "[kertas][k] and [**bold**][k]",
            &Definitions::from_text("[k]: other.md"),
        );
        assert_eq!(inline.links.len(), 2);
        assert!(
            inline
                .links
                .iter()
                .all(|link| link.destination == "other.md")
        );
        assert_eq!(&inline.text[inline.links[1].display.clone()], "bold");
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

#[cfg(test)]
mod autolink_tests {
    use super::*;
    use crate::blocks::Blocks;
    use crate::rendered::{RenderedBlock, render_blocks};

    fn render(source: &str) -> InlineText {
        render_inline(source, &Definitions::default())
    }

    /// Each link's text and destination.
    fn links(inline: &InlineText) -> Vec<(&str, &str)> {
        inline
            .links
            .iter()
            .map(|link| {
                (
                    &inline.text[link.display.clone()],
                    link.destination.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn bare_urls_become_links() {
        let inline = render("See https://example.com, www.example.org and me@example.com.");
        assert_eq!(
            links(&inline),
            [
                ("https://example.com", "https://example.com"),
                ("www.example.org", "https://www.example.org"),
                ("me@example.com", "mailto:me@example.com"),
            ]
        );
        let link = |text: &str| {
            inline
                .segments
                .iter()
                .find(|segment| &inline.text[segment.display.clone()] == text)
                .is_some_and(|segment| segment.style.link)
        };
        assert!(link("https://example.com"));
        assert!(!link("See "));
        assert!(!link(", "));
        assert!(inline.link_at(6).is_some());
    }

    #[test]
    fn source_is_unchanged_and_still_maps() {
        let source = "a **https://example.com/x** b";
        let inline = render(source);
        assert_eq!(inline.text, "a https://example.com/x b");
        assert_eq!(
            links(&inline),
            [("https://example.com/x", "https://example.com/x")]
        );
        // The `x` maps back to its place in the source.
        let x = inline.text.find("/x").unwrap() + 1;
        assert_eq!(&source[inline.source_offset(x)..][..1], "x");
    }

    #[test]
    fn code_and_existing_links_are_left_alone() {
        let inline = render("`https://a.com` [https://b.com](https://c.com) <https://d.com>");
        assert_eq!(
            links(&inline),
            [
                ("https://b.com", "https://c.com"),
                ("https://d.com", "https://d.com"),
            ]
        );
        let text = "```\nhttps://example.com\n```\n";
        let blocks = render_blocks(text, &Blocks::parse(text));
        let [RenderedBlock::CodeBlock { code, .. }] = blocks.as_slice() else {
            panic!("{blocks:?}");
        };
        assert!(code.links.is_empty());
        assert!(code.segments.iter().all(|segment| !segment.style.link));
    }

    #[test]
    fn text_after_code_can_start_a_link() {
        let inline = render("`x`https://example.com");
        assert_eq!(
            links(&inline),
            [("https://example.com", "https://example.com")]
        );
    }

    #[test]
    fn links_inside_tables_and_lists() {
        let text = "- www.example.com\n\n| https://example.com |\n|---|\n";
        let blocks = render_blocks(text, &Blocks::parse(text));
        let RenderedBlock::List { items, .. } = &blocks[0] else {
            panic!()
        };
        let RenderedBlock::Paragraph(item) = &items[0].children[0] else {
            panic!()
        };
        assert_eq!(item.links.len(), 1);
        let RenderedBlock::Table(table) = &blocks[1] else {
            panic!()
        };
        assert_eq!(table.header[0].links.len(), 1);
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;

    fn render(source: &str) -> InlineText {
        render_inline(source, &Definitions::default())
    }

    /// Each image's alt text and destination.
    fn images(inline: &InlineText) -> Vec<(&str, &str)> {
        inline
            .images
            .iter()
            .map(|image| {
                (
                    &inline.text[image.display.clone()],
                    image.destination.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn images_keep_their_alt_text() {
        let inline = render("Logo: ![The *logo*](img/logo.png) and ![](dot.svg).");
        assert_eq!(inline.text, "Logo: The logo and .");
        assert_eq!(
            images(&inline),
            [("The logo", "img/logo.png"), ("", "dot.svg")]
        );
    }

    #[test]
    fn reference_images_resolve() {
        let text = "![alt][pic]\n\n[pic]: pics/a.png\n";
        let blocks = crate::blocks::Blocks::parse(text);
        let source = &text[blocks.as_slice()[0].range.clone()];
        let mut builder = InlineBuilder::default();
        for (event, range) in blocks.definitions().parse(source, source.len()) {
            if matches!(event, Event::End(TagEnd::Paragraph)) {
                break;
            }
            builder.handle(&event, range);
        }
        assert_eq!(images(&builder.finish()), [("alt", "pics/a.png")]);
    }

    #[test]
    fn linked_images() {
        let inline = render("[![badge](b.svg)](https://example.com) [![](x.png)](y.md)");
        assert_eq!(inline.links.len(), 2);
        let first = inline.link_around(&inline.images[0]).unwrap();
        assert_eq!(first.destination, "https://example.com");
        // No alt text, but still a link.
        let second = inline.link_around(&inline.images[1]).unwrap();
        assert_eq!(second.destination, "y.md");
    }

    #[test]
    fn alt_text_is_not_autolinked() {
        let inline = render("![see https://example.com](a.png) https://example.org");
        assert_eq!(inline.links.len(), 1);
        assert_eq!(inline.links[0].destination, "https://example.org");
    }

    #[test]
    fn slices_keep_source_offsets() {
        let source = "a **bold** [link](x) ![img](p.png) end";
        let inline = render(source);
        let image = inline.images[0].display.clone();
        let before = inline.slice(0..image.start);
        assert_eq!(before.text, "a bold link ");
        assert_eq!(before.links[0].display, 7..11);
        let bold = before.text.find("old").unwrap();
        assert_eq!(&source[before.source_offset(bold)..][..3], "old");
        let after = inline.slice(image.end..inline.text.len());
        assert_eq!(after.text, " end");
        assert_eq!(&source[after.source_offset(1)..], "end");
    }
}

#[cfg(test)]
mod anchor_tag_tests {
    use super::*;

    fn render(source: &str) -> InlineText {
        render_inline(source, &Definitions::default())
    }

    #[test]
    fn target_only_anchors_are_hidden() {
        let inline = render(r#"<a id="install"></a>Install it"#);
        assert_eq!(inline.text, "Install it");
        assert_eq!(inline.anchors, ["install"]);
        let inline = render(r#"<a name="old"/>Old"#);
        assert_eq!(inline.text, "Old");
        assert_eq!(inline.anchors, ["old"]);
    }

    #[test]
    fn a_target_on_its_own_line_leaves_no_space() {
        let inline = render("<a id=\"top\"></a>\nText");
        assert_eq!(inline.text, "Text");
    }

    #[test]
    fn text_inside_a_target_stays() {
        let inline = render(r#"See <a id="here">this part</a>."#);
        assert_eq!(inline.text, "See this part.");
    }

    #[test]
    fn other_html_is_shown_with_its_ids() {
        let inline = render(r#"<span id="s">x</span> <a href="/y">y</a>"#);
        assert_eq!(inline.text, r#"<span id="s">x</span> <a href="/y">y</a>"#);
        assert_eq!(inline.anchors, ["s"]);
    }

    #[test]
    fn a_self_closed_target_does_not_hide_a_later_closing_tag() {
        let inline = render(r#"<a id="x"/> <a href="/y">y</a>"#);
        assert_eq!(inline.text, r#" <a href="/y">y</a>"#);
    }

    #[test]
    fn footnote_references_are_superscript_links() {
        let text = "See[^a] and[^b], then[^a] again[^none].\n\n[^b]: B.\n[^a]: A.\n";
        let blocks = crate::blocks::Blocks::parse(text);
        let source = &text[blocks.as_slice()[0].range.clone()];
        let inline = render_inline(source, blocks.definitions());
        assert_eq!(inline.text, "See¹ and², then¹ again[^none].");
        let destinations: Vec<&str> = inline
            .links
            .iter()
            .map(|link| link.destination.as_str())
            .collect();
        assert_eq!(destinations, ["#fn-a", "#fn-b", "#fn-a"]);
        // Clicking the number puts the caret at the reference.
        let first = &inline.links[0];
        assert_eq!(inline.source_offset(first.display.start), 3);
    }

    #[test]
    fn superscript_numbers() {
        assert_eq!(superscript(1), "¹");
        assert_eq!(superscript(10), "¹⁰");
        assert_eq!(superscript(234), "²³⁴");
        assert_eq!(superscript(56789), "⁵⁶⁷⁸⁹");
    }

    #[test]
    fn back_links_follow_the_text() {
        let mut inline = render("A note.");
        inline.push_back_links("Label", 2);
        assert_eq!(inline.text, "A note. ↩ ↩²");
        let destinations: Vec<&str> = inline
            .links
            .iter()
            .map(|link| link.destination.as_str())
            .collect();
        assert_eq!(destinations, ["#fnref-label", "#fnref-label-2"]);
        assert_eq!(inline.source_offset(inline.text.len() - 1), 7);

        let mut empty = InlineText::default();
        empty.push_back_links("x", 1);
        assert_eq!(empty.text, "↩");
    }
}
