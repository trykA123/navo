//! Comma-separated list settings typed as one line of text.

/// Splits on commas outside `{}`, trims each item and drops empty ones.
#[must_use]
pub fn split_list(text: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    items.push(&text[start..]);
    items
        .into_iter()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Joins items with `", "`, the form [`split_list`] reads back unchanged.
#[must_use]
pub fn join_list(items: &[String]) -> String {
    items.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commas_inside_braces_stay_in_one_item() {
        assert_eq!(split_list("*.{a,b}, c"), ["*.{a,b}", "c"]);
    }

    #[test]
    fn plain_lists_split_trim_and_drop_empties() {
        assert_eq!(split_list(" a ,, b ,"), ["a", "b"]);
        assert!(split_list("  ").is_empty());
    }

    #[test]
    fn an_unclosed_brace_keeps_the_rest_together() {
        assert_eq!(split_list("{a,b, c"), ["{a,b, c"]);
    }

    #[test]
    fn join_then_split_is_unchanged() {
        let items = vec!["*.{o,obj}".to_owned(), "target".to_owned()];
        assert_eq!(split_list(&join_list(&items)), items);
    }
}
