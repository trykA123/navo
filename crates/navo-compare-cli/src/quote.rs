//! Strips the double quotes Windows Explorer's "Copy as path" puts around a pasted path.
//! Used by `args::side`, so the Start fields and the command line share it.

/// Trims whitespace, then removes one surrounding pair of double quotes if present.
/// A lone quote, or quotes only in the middle, are left alone.
pub fn unquote(text: &str) -> &str {
    let trimmed = text.trim();
    match trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        Some(inner) => inner,
        None => trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::unquote;

    /// A quoted Windows path loses its quotes and keeps its inner space.
    #[test]
    fn strips_one_surrounding_pair() {
        assert_eq!(unquote(r#""C:\a b""#), r"C:\a b");
    }

    /// A lone quote, or quotes in the middle, stay as typed.
    #[test]
    fn leaves_lone_and_inner_quotes() {
        assert_eq!(unquote(r#""x"#), r#""x"#);
        assert_eq!(unquote(r#"a"b""#), r#"a"b""#);
    }

    /// Whitespace outside the quotes is trimmed first.
    #[test]
    fn trims_before_stripping() {
        assert_eq!(unquote("  \"p\"  "), "p");
    }
}
