//! Finding bare URLs and email addresses in text, using GitHub's rules
//! (GFM's "autolinks (extension)", as cmark-gfm implements them), so they
//! can be drawn and followed as links (#93). pulldown-cmark only finds
//! `<https://…>` autolinks.

use std::ops::Range;

/// A bare link found in text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Autolink {
    /// Byte range in the text.
    pub range: Range<usize>,
    /// Where it leads: the URL, `https://` + a `www.` address, or `mailto:`
    /// + an email address.
    pub destination: String,
}

/// Every bare link in `text`, in order.
pub fn find(text: &str) -> Vec<Autolink> {
    let mut links = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let found = url_at(text, index).or_else(|| email_at(text, index));
        match found {
            Some(link) => {
                index = link.range.end;
                links.push(link);
            }
            None => index += text[index..].chars().next().map_or(1, char::len_utf8),
        }
    }
    links
}

/// A `www.`, `http://` or `https://` link starting at `start`.
fn url_at(text: &str, start: usize) -> Option<Autolink> {
    if !starts_link(text, start) {
        return None;
    }
    let rest = &text[start..];
    let (domain_start, prefix) = if rest.starts_with("www.") {
        (start, Some("https://"))
    } else if starts_with_ignore_case(rest, "https://") {
        (start + "https://".len(), None)
    } else if starts_with_ignore_case(rest, "http://") {
        (start + "http://".len(), None)
    } else {
        return None;
    };
    let domain_end = domain_end(text, domain_start, prefix.is_none())?;
    // The path runs to the next space or `<`.
    let end = text[domain_end..]
        .find(|c: char| c.is_whitespace() || c == '<')
        .map_or(text.len(), |offset| domain_end + offset);
    let end = trim_end(&text[start..end]) + start;
    // Something has to follow `www.` or the scheme.
    let minimum = match prefix {
        Some(_) => start + "www.".len(),
        None => domain_start,
    };
    if end <= minimum {
        return None;
    }
    let link = &text[start..end];
    Some(Autolink {
        range: start..end,
        destination: match prefix {
            Some(prefix) => format!("{prefix}{link}"),
            None => link.to_owned(),
        },
    })
}

/// Whether a `www.` or `http` link can start at `start`: at the start of
/// the text, after a space, or after `*`, `_`, `~` or `(`.
fn starts_link(text: &str, start: usize) -> bool {
    text[..start]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || matches!(c, '*' | '_' | '~' | '('))
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// Where a valid domain starting at `start` ends: letters, digits, `-` and
/// `_`, in segments separated by `.`, with no `_` in the last two segments.
/// Unless `short` is allowed (after `http://`), it needs at least one `.`.
fn domain_end(text: &str, start: usize, short: bool) -> Option<usize> {
    let mut end = start;
    let mut periods = 0;
    // Underscores in the last two segments.
    let (mut previous_underscores, mut underscores) = (0, 0);
    for (offset, c) in text[start..].char_indices() {
        match c {
            '_' => underscores += 1,
            '.' => {
                previous_underscores = underscores;
                underscores = 0;
                periods += 1;
            }
            '-' => {}
            c if c.is_alphanumeric() => {}
            _ => break,
        }
        end = start + offset + c.len_utf8();
    }
    let valid =
        end > start && previous_underscores == 0 && underscores == 0 && (short || periods > 0);
    valid.then_some(end)
}

/// The length of `link` without trailing punctuation that GitHub leaves
/// out: `?`, `!`, `.`, `,`, `:`, `*`, `_`, `~` and quotes, a `)` that isn't
/// matched by a `(`, and an entity-like `&name;`.
fn trim_end(link: &str) -> usize {
    let bytes = link.as_bytes();
    let mut end = link.len();
    while end > 0 {
        match bytes[end - 1] {
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => end -= 1,
            b';' => {
                let name_start = link[..end - 1]
                    .rfind(|c: char| !c.is_ascii_alphanumeric())
                    .map_or(0, |index| index + 1);
                if name_start > 0 && name_start < end - 1 && bytes[name_start - 1] == b'&' {
                    end = name_start - 1;
                } else {
                    end -= 1;
                }
            }
            b')' => {
                let opening = link[..end].matches('(').count();
                let closing = link[..end].matches(')').count();
                if closing <= opening {
                    break;
                }
                end -= 1;
            }
            _ => break,
        }
    }
    end
}

/// An email address starting at `start`: a local part of letters, digits,
/// `.`, `+`, `-` and `_`, then `@` and a domain with at least one `.`. It
/// can't end in `-` or `_`, and a final `.` isn't part of it.
fn email_at(text: &str, start: usize) -> Option<Autolink> {
    let is_local = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-' | '_');
    // Only from the start of the local part.
    if text[..start].chars().next_back().is_some_and(is_local) {
        return None;
    }
    let at = start + text[start..].find(|c: char| !is_local(c))?;
    if at == start || !text[at..].starts_with('@') {
        return None;
    }
    let domain_start = at + 1;
    let mut end = domain_start;
    let mut periods = 0;
    let bytes = text.as_bytes();
    while end < text.len() {
        match bytes[end] {
            c if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' => {}
            // A `.` counts only if more of the domain follows it.
            b'.' if bytes.get(end + 1).is_some_and(u8::is_ascii_alphanumeric) => periods += 1,
            _ => break,
        }
        end += 1;
    }
    if periods == 0 || matches!(bytes[end - 1], b'-' | b'_') {
        return None;
    }
    let address = &text[start..end];
    Some(Autolink {
        range: start..end,
        destination: format!("mailto:{address}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The links found in `text`, as written.
    fn links(text: &str) -> Vec<&str> {
        find(text)
            .into_iter()
            .map(|link| &text[link.range])
            .collect()
    }

    fn destinations(text: &str) -> Vec<String> {
        find(text)
            .into_iter()
            .map(|link| link.destination)
            .collect()
    }

    #[test]
    fn www_links_open_as_https() {
        assert_eq!(
            links("Go to www.commonmark.org/help now"),
            ["www.commonmark.org/help"]
        );
        assert_eq!(
            destinations("www.commonmark.org/help"),
            ["https://www.commonmark.org/help"]
        );
    }

    #[test]
    fn http_and_https_links() {
        assert_eq!(
            links("Visit https://example.com/a?b=c#d or http://example.com."),
            ["https://example.com/a?b=c#d", "http://example.com"]
        );
        assert_eq!(destinations("https://example.com"), ["https://example.com"]);
        // After a scheme, a domain without a period is fine.
        assert_eq!(
            links("http://localhost:8080/x"),
            ["http://localhost:8080/x"]
        );
    }

    #[test]
    fn trailing_punctuation_is_left_out() {
        // GFM spec examples 622 and 623.
        assert_eq!(links("Visit www.commonmark.org."), ["www.commonmark.org"]);
        assert_eq!(
            links("Visit www.commonmark.org/a.b."),
            ["www.commonmark.org/a.b"]
        );
        assert_eq!(
            links("Is it https://example.com/?! Yes, https://example.com/x:"),
            ["https://example.com/", "https://example.com/x"]
        );
    }

    #[test]
    fn unmatched_closing_parentheses_are_left_out() {
        // GFM spec examples 624 to 626.
        assert_eq!(
            links("www.google.com/search?q=Markup+(business)"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("www.google.com/search?q=Markup+(business)))"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("(www.google.com/search?q=Markup+(business))"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("(www.google.com/search?q=Markup+(business)"),
            ["www.google.com/search?q=Markup+(business)"]
        );
        assert_eq!(
            links("www.google.com/search?q=(business))+ok"),
            ["www.google.com/search?q=(business))+ok"]
        );
    }

    #[test]
    fn entity_like_endings_are_left_out() {
        // GFM spec examples 627 and 628.
        assert_eq!(
            links("www.google.com/search?q=commonmark&hl=en"),
            ["www.google.com/search?q=commonmark&hl=en"]
        );
        assert_eq!(
            links("www.google.com/search?q=commonmark&hl;"),
            ["www.google.com/search?q=commonmark"]
        );
    }

    #[test]
    fn links_stop_at_a_less_than_sign() {
        // GFM spec example 629.
        assert_eq!(links("www.commonmark.org/he<lp"), ["www.commonmark.org/he"]);
    }

    #[test]
    fn underscores_in_the_last_two_domain_segments_are_invalid() {
        assert_eq!(links("www.a_b.example.com"), ["www.a_b.example.com"]);
        assert!(links("www.example.a_b.com").is_empty());
        assert!(links("www.example.com_x").is_empty());
    }

    #[test]
    fn links_start_after_space_or_emphasis_characters() {
        assert_eq!(links("(https://example.com)"), ["https://example.com"]);
        assert_eq!(links("*www.example.com*"), ["www.example.com"]);
        assert!(links("xhttps://example.com").is_empty());
        assert!(links("awww.example.com").is_empty());
    }

    #[test]
    fn a_scheme_alone_is_not_a_link() {
        assert!(links("http:// and https://").is_empty());
        assert!(links("www.").is_empty());
    }

    #[test]
    fn email_addresses() {
        // GFM spec examples 631 to 634.
        assert_eq!(links("foo@bar.baz"), ["foo@bar.baz"]);
        assert_eq!(destinations("foo@bar.baz"), ["mailto:foo@bar.baz"]);
        assert_eq!(
            links("hello@mail+xyz.example isn't valid, but hello+xyz@mail.example is."),
            ["hello+xyz@mail.example"]
        );
        assert_eq!(links("a.b-c_d@a.b"), ["a.b-c_d@a.b"]);
        assert_eq!(links("a.b-c_d@a.b."), ["a.b-c_d@a.b"]);
        assert!(links("a.b-c_d@a.b-").is_empty());
        assert!(links("a.b-c_d@a.b_").is_empty());
        assert!(links("me@localhost").is_empty());
    }

    #[test]
    fn email_in_a_url_is_part_of_the_url() {
        assert_eq!(
            links("https://me@example.com/x"),
            ["https://me@example.com/x"]
        );
    }

    #[test]
    fn non_ascii_text_around_links() {
        assert_eq!(
            links("Voir « https://exemple.fr/é » ou café@x"),
            ["https://exemple.fr/é"]
        );
    }
}
