//! Boundary guard for the compare screen: the screen, diff and list modules
//! must not name the window or the browser (NC1, NC15).

#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use crate::l10n::files_with;
use crate::l10n::scan::live_tokens;
use crate::l10n::tokens::{Spanned, Token, tokenize};

const ROOTS: [&str; 3] = [
    "crates/navo-ui/src/window_view/compare_screen",
    "crates/navo-ui/src/compare_diff",
    "crates/navo-ui/src/compare_list",
];

/// Walk every `.rs` file under the compare screen roots, tests included, and
/// print `path:line: <text>` for each line naming the window or the browser.
///
/// Takes no arguments. Exits 0 when clean, 1 on hits or read errors, 2 on
/// unexpected arguments.
pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask seams");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("seams: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let sources = match load(root) {
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("seams: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut hits = 0;
    for (path, text) in &sources {
        for line in scan(text) {
            let source = text.lines().nth(line - 1).unwrap_or_default();
            eprintln!("{path}:{line}: {}", source.trim());
            hits += 1;
        }
    }
    if hits > 0 {
        return ExitCode::FAILURE;
    }
    println!(
        "seams: {} files checked, no window or browser name in the compare screen",
        sources.len()
    );
    ExitCode::SUCCESS
}

/// Read every Rust file under the guard roots as (relative path, text) pairs.
///
/// Fails when a root is missing or a file cannot be read.
fn load(root: &Path) -> io::Result<Vec<(String, String)>> {
    let mut sources = Vec::new();
    for folder in ROOTS {
        for path in files_with(&root.join(folder), "rs")? {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            sources.push((
                relative.to_string_lossy().replace('\\', "/"),
                fs::read_to_string(&path)?,
            ));
        }
    }
    Ok(sources)
}

/// Return the 1-based lines of `text` that name the window or the browser.
///
/// Matches the identifiers `NavoWindow` and `Browser` and the token run
/// `self . browser`; comments, strings and test items are skipped, and each
/// hit line is reported once.
pub fn scan(text: &str) -> Vec<usize> {
    let tokens = live_tokens(tokenize(text));
    let mut lines: Vec<usize> = Vec::new();
    for (index, spanned) in tokens.iter().enumerate() {
        let Token::Ident(name) = &spanned.token else {
            continue;
        };
        let hit = name == "NavoWindow"
            || name == "Browser"
            || (name == "self" && field_access(&tokens, index + 1, "browser"));
        if hit && !lines.contains(&spanned.line) {
            lines.push(spanned.line);
        }
    }
    lines
}

/// True when `tokens[index]` is `.` followed by the identifier `field`.
fn field_access(tokens: &[Spanned], index: usize, field: &str) -> bool {
    matches!(tokens.get(index), Some(s) if s.token == Token::Punct('.'))
        && matches!(
            tokens.get(index + 1),
            Some(s) if matches!(&s.token, Token::Ident(name) if name == field)
        )
}
