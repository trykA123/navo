//! A small scan of Git config text for `core.worktree` values, so the pointer check can refuse
//! a network work tree before gix opens it. No includes, no gix; fails closed on odd syntax.

use crate::repo::GitProblem;

/// Every `core.worktree` value in config `text`, unquoted and unescaped. Keys and the section
/// name match without case; `[core "sub"]` is another section. Fails with `Unsupported` for a
/// section header without `]`, a `core.worktree` value with an open quote or a continuation, or
/// any line (in any section) ending in an odd number of `\`, which Git reads as a continuation.
pub fn core_worktrees(text: &str) -> Result<Vec<String>, GitProblem> {
    let mut in_core = false;
    let mut found = Vec::new();
    let text = text.trim_start_matches('\u{feff}');
    if text.lines().any(ends_in_continuation) {
        return Err(GitProblem::Unsupported);
    }
    for line in text.lines() {
        let mut rest = line.trim_start();
        if rest.starts_with('[') {
            let (header, after) = split_header(rest)?;
            in_core = header.trim().eq_ignore_ascii_case("core");
            rest = after.trim_start();
        }
        if !in_core || rest.starts_with('#') || rest.starts_with(';') {
            continue;
        }
        let Some((key, value)) = rest.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("worktree") {
            found.push(unquote(value)?);
        }
    }
    Ok(found)
}

/// True when `line` ends in an odd number of `\`: the last one escapes the line break.
fn ends_in_continuation(line: &str) -> bool {
    let line = line.strip_suffix('\r').unwrap_or(line);
    (line.len() - line.trim_end_matches('\\').len()) % 2 == 1
}

/// Splits `line` (starting with `[`) into the header inside the brackets and the text after
/// the `]` that ends it; a `]` inside quotes does not end it. `Unsupported` without a `]`.
fn split_header(line: &str) -> Result<(&str, &str), GitProblem> {
    let mut quoted = false;
    let mut escaped = false;
    for (at, char) in line.char_indices().skip(1) {
        match char {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            ']' if !quoted => return Ok((&line[1..at], &line[at + 1..])),
            _ => {}
        }
    }
    Err(GitProblem::Unsupported)
}

/// The value text after `=` as Git reads it: quotes removed, `\\`, `\"`, `\n`, `\t` and `\b`
/// unescaped, a `#` or `;` outside quotes ends it, outer spaces trimmed. `Unsupported` for an
/// open quote, a trailing `\` (a continuation line) or an unknown escape.
fn unquote(value: &str) -> Result<String, GitProblem> {
    let mut out = String::new();
    let mut quoted = false;
    let mut chars = value.chars();
    while let Some(char) = chars.next() {
        match char {
            '"' => quoted = !quoted,
            '#' | ';' if !quoted => break,
            '\\' => out.push(match chars.next() {
                Some('\\') => '\\',
                Some('"') => '"',
                Some('n') => '\n',
                Some('t') => '\t',
                Some('b') => '\u{8}',
                _ => return Err(GitProblem::Unsupported),
            }),
            _ => out.push(char),
        }
    }
    if quoted {
        return Err(GitProblem::Unsupported);
    }
    Ok(out.trim().to_owned())
}
