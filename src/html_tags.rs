//! Reading just enough of HTML tags to find link targets (#96): the `id`
//! of any tag, and the `name` of an `<a>` tag. HTML is never run or drawn
//! as HTML (N4).

/// One start tag's name (lowercase) and attributes, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub attributes: Vec<(String, String)>,
}

impl Tag {
    /// The value of attribute `name` (case-insensitive), if it's there.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The name a `#name` link can use to reach this tag: its `id`, or an
    /// `<a>` tag's `name`.
    pub fn anchor(&self) -> Option<&str> {
        self.attribute("id")
            .or_else(|| (self.name == "a").then(|| self.attribute("name")).flatten())
            .filter(|anchor| !anchor.is_empty())
    }

    /// Whether this is an `<a>` tag that's only a link target, with no
    /// `href`, so it shows nothing.
    pub fn is_target_only(&self) -> bool {
        self.name == "a" && self.attribute("href").is_none() && self.anchor().is_some()
    }
}

/// Whether `html` holds nothing but tags that only mark link targets
/// (`<a id="x"></a>`), so it shows nothing.
pub fn only_targets(html: &str) -> bool {
    let mut rest = html.trim_start();
    let mut any = false;
    while !rest.is_empty() {
        if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("</a>") {
            rest = &rest[4..];
        } else {
            let Some((tag, after)) = rest.strip_prefix('<').and_then(start_tag) else {
                return false;
            };
            if !tag.is_target_only() {
                return false;
            }
            any = true;
            rest = after;
        }
        rest = rest.trim_start();
    }
    any
}

/// Every start tag in `html`, in order. Comments, closing tags and
/// anything that isn't a well-formed tag are skipped.
pub fn start_tags(html: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        if let Some(after) = rest.strip_prefix("!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        if let Some((tag, after)) = start_tag(rest) {
            tags.push(tag);
            rest = after;
        }
    }
    tags
}

/// The start tag at the start of `text` (just after its `<`), and the text
/// after it.
fn start_tag(text: &str) -> Option<(Tag, &str)> {
    let name_end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(text.len());
    let name = &text[..name_end];
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut tag = Tag {
        name: name.to_ascii_lowercase(),
        attributes: Vec::new(),
    };
    let mut rest = &text[name_end..];
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("/>").or_else(|| rest.strip_prefix('>')) {
            return Some((tag, after));
        }
        let key_end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/' | '"' | '\''))
            .unwrap_or(rest.len());
        if key_end == 0 {
            return None;
        }
        let key = rest[..key_end].to_owned();
        rest = rest[key_end..].trim_start();
        let value = match rest.strip_prefix('=') {
            Some(after) => {
                let after = after.trim_start();
                let (value, after) = match after.chars().next()? {
                    quote @ ('"' | '\'') => {
                        let inner = &after[1..];
                        let end = inner.find(quote)?;
                        (&inner[..end], &inner[end + 1..])
                    }
                    _ => {
                        let end = after
                            .find(|c: char| c.is_whitespace() || c == '>')
                            .unwrap_or(after.len());
                        (&after[..end], &after[end..])
                    }
                };
                rest = after;
                value.to_owned()
            }
            None => String::new(),
        };
        tag.attributes.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors(html: &str) -> Vec<String> {
        start_tags(html)
            .iter()
            .filter_map(|tag| tag.anchor().map(str::to_owned))
            .collect()
    }

    #[test]
    fn ids_and_anchor_names() {
        assert_eq!(anchors(r#"<a id="install"></a>"#), ["install"]);
        assert_eq!(anchors(r#"<a name='old-style'></a>"#), ["old-style"]);
        assert_eq!(anchors(r#"<div class="x" ID=box>"#), ["box"]);
        // Only <a> tags are named by `name`.
        assert!(anchors(r#"<input name="field">"#).is_empty());
    }

    #[test]
    fn several_tags_comments_and_closing_tags() {
        assert_eq!(
            anchors("<p id=one>\n<!-- <a id=hidden> -->\n</p><span id=\"two\"/>"),
            ["one", "two"]
        );
    }

    #[test]
    fn malformed_tags_are_skipped() {
        assert!(anchors("a < b and <3 and <a id=\"unclosed").is_empty());
        assert_eq!(anchors("<a id=\"\"> <b id=x>"), ["x"]);
    }

    #[test]
    fn html_of_only_targets() {
        assert!(only_targets("<a id=\"x\">\n"));
        assert!(only_targets("<a name=x></a> <a id=y />"));
        assert!(!only_targets("<a id=x>text</a>"));
        assert!(!only_targets("<div id=x>"));
        assert!(!only_targets(""));
    }

    #[test]
    fn target_only_anchors() {
        let tag = |html: &str| start_tags(html).remove(0);
        assert!(tag(r#"<a id="x">"#).is_target_only());
        assert!(tag(r#"<a name="x">"#).is_target_only());
        assert!(!tag(r#"<a id="x" href="/y">"#).is_target_only());
        assert!(!tag(r#"<a href="/y">"#).is_target_only());
        assert!(!tag(r#"<span id="x">"#).is_target_only());
    }
}
