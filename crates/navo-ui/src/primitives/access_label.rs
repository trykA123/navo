//! Parses `&` access-key markers in labels and draws the key underlined while Alt is held.

use gpui_kit::{AnyElement, IntoElement, ParentElement, Styled, div};

/// A label split around its access key: text before, the key letter, text after.
#[derive(Debug, PartialEq, Eq)]
pub struct AccessParts {
    pub before: String,
    pub key: Option<char>,
    pub after: String,
}

/// Splits `text` at its first `&x` marker; `&&` is a literal `&`, later markers are plain letters.
pub fn parse(text: &str) -> AccessParts {
    let mut parts = AccessParts {
        before: String::new(),
        key: None,
        after: String::new(),
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let mut out = c;
        if c == '&' {
            match chars.peek().copied() {
                Some('&') => {
                    chars.next();
                }
                Some(next) if parts.key.is_none() => {
                    chars.next();
                    parts.key = Some(next);
                    continue;
                }
                Some(next) => {
                    chars.next();
                    out = next;
                }
                None => {}
            }
        }
        if parts.key.is_some() {
            parts.after.push(out);
        } else {
            parts.before.push(out);
        }
    }
    parts
}

/// Returns the lowercase access key of `text`, or None when it has no marker.
pub fn access_key(text: &str) -> Option<char> {
    parse(text)
        .key
        .map(|key| key.to_lowercase().next().unwrap_or(key))
}

/// Returns `text` as shown and read aloud: markers removed, `&&` turned into `&`.
pub fn plain_text(text: &str) -> String {
    let parts = parse(text);
    let key = parts.key.map(String::from).unwrap_or_default();
    format!("{}{key}{}", parts.before, parts.after)
}

/// Builds the label element; the key letter is underlined only when `show_keys` is true.
pub fn access_label(text_with_amp: &str, show_keys: bool) -> AnyElement {
    let parts = parse(text_with_amp);
    let key = parts.key.map(|key| {
        let letter = div().flex_none().child(key.to_string());
        if show_keys {
            letter.underline()
        } else {
            letter
        }
    });
    div()
        .flex()
        .flex_none()
        .child(parts.before)
        .children(key)
        .child(parts.after)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> (String, Option<char>, String) {
        let p = parse(text);
        (p.before, p.key, p.after)
    }

    #[test]
    fn marker_at_the_start() {
        assert_eq!(split("&Next"), (String::new(), Some('N'), "ext".into()));
    }

    #[test]
    fn marker_in_the_middle() {
        assert_eq!(
            split("I &accept it"),
            ("I ".into(), Some('a'), "ccept it".into())
        );
    }

    #[test]
    fn marker_at_the_end() {
        assert_eq!(split("Bac&k"), ("Bac".into(), Some('k'), String::new()));
    }

    #[test]
    fn double_ampersand_is_a_literal() {
        assert_eq!(split("R&&D"), ("R&D".into(), None, String::new()));
        assert_eq!(access_key("R&&D"), None);
    }

    #[test]
    fn no_marker_has_no_key() {
        assert_eq!(split("Close"), ("Close".into(), None, String::new()));
    }

    #[test]
    fn the_first_marker_wins() {
        assert_eq!(
            split("&Back &up"),
            (String::new(), Some('B'), "ack up".into())
        );
        assert_eq!(access_key("&Back &up"), Some('b'));
        assert_eq!(plain_text("&Back &up"), "Back up");
    }

    #[test]
    fn a_lone_trailing_ampersand_stays() {
        assert_eq!(plain_text("Tom &"), "Tom &");
    }
}
