//! Showing tabs as spaces. A tab is shown as `tab_width` spaces wherever
//! Kami shows it, rendered, revealed or printed, so a block doesn't change
//! width when it's revealed. The source keeps its tabs.

use std::borrow::Cow;

/// The tab widths allowed, in spaces.
pub const TAB_WIDTHS: &[u32] = &[2, 4, 8];

/// The default tab width, in spaces.
pub const DEFAULT_TAB_WIDTH: u32 = 4;

/// `source` with each tab shown as `width` spaces, and the mapping between
/// offsets in the source and in the shown text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedTabs {
    /// The text as shown.
    pub text: String,
    /// The source offset of each tab, in order.
    tabs: Vec<usize>,
    width: usize,
}

impl ExpandedTabs {
    pub fn new(source: &str, width: u32) -> Self {
        let width = width as usize;
        let tabs: Vec<usize> = source.match_indices('\t').map(|(at, _)| at).collect();
        Self {
            text: expand(source, width as u32).into_owned(),
            tabs,
            width,
        }
    }

    /// The offset in the shown text of `offset` in the source.
    pub fn to_shown(&self, offset: usize) -> usize {
        let before = self.tabs.partition_point(|&tab| tab < offset);
        offset + before * (self.width - 1)
    }

    /// The source offset of `shown`, an offset in the shown text. An offset
    /// inside a tab's spaces maps to the tab.
    pub fn to_source(&self, shown: usize) -> usize {
        let mut offset = shown;
        for &tab in &self.tabs {
            // Where this tab starts in the shown text.
            let start = self.to_shown(tab);
            if shown <= start {
                break;
            }
            if shown < start + self.width {
                return tab;
            }
            offset -= self.width - 1;
        }
        offset
    }
}

/// `text` with each tab as `width` spaces.
pub fn expand(text: &str, width: u32) -> Cow<'_, str> {
    if text.contains('\t') {
        Cow::Owned(text.replace('\t', &" ".repeat(width as usize)))
    } else {
        Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tabs() {
        let tabs = ExpandedTabs::new("a\tb\t\tc", 4);
        assert_eq!(tabs.text, "a    b        c");
        assert_eq!(expand("no tabs", 4), "no tabs");
        assert_eq!(expand("\tx", 2), "  x");
    }

    #[test]
    fn maps_offsets_both_ways() {
        // "a\tb": a(0) tab(1) b(2), shown as a(0) spaces(1..5) b(5).
        let tabs = ExpandedTabs::new("a\tb", 4);
        assert_eq!(
            (0..=3)
                .map(|offset| tabs.to_shown(offset))
                .collect::<Vec<_>>(),
            [0, 1, 5, 6]
        );
        assert_eq!(
            (0..=6)
                .map(|shown| tabs.to_source(shown))
                .collect::<Vec<_>>(),
            [0, 1, 1, 1, 1, 2, 3],
            "inside the tab maps to the tab"
        );
    }

    #[test]
    fn round_trips_every_source_offset() {
        for width in TAB_WIDTHS.iter().copied() {
            for source in ["\t\tx", "é\tü\t", "", "plain", "\t"] {
                let tabs = ExpandedTabs::new(source, width);
                for offset in 0..=source.len() {
                    assert_eq!(tabs.to_source(tabs.to_shown(offset)), offset, "{source:?}");
                }
                assert_eq!(tabs.to_shown(source.len()), tabs.text.len());
            }
        }
    }
}
