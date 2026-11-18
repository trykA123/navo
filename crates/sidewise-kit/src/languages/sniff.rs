use super::names;

const BOM: &str = "\u{feff}";
const HEAD_LIMIT: usize = 4096;

pub(super) const INTERPRETERS: [(&str, &str); 16] = [
    ("python", "python"),
    ("bash", "bash"),
    ("sh", "bash"),
    ("dash", "bash"),
    ("ksh", "bash"),
    ("zsh", "bash"),
    ("perl", "perl"),
    ("pwsh", "powershell"),
    ("powershell", "powershell"),
    ("rscript", "r"),
    ("node", "javascript"),
    ("nodejs", "javascript"),
    ("deno", "javascript"),
    ("ruby", "ruby"),
    ("lua", "lua"),
    ("php", "php"),
];

pub(super) fn head_text(head: &[u8]) -> String {
    let head = &head[..head.len().min(HEAD_LIMIT)];
    if let Some(bytes) = head.strip_prefix(&[0xff, 0xfe]) {
        return utf16(bytes, u16::from_le_bytes);
    }
    if let Some(bytes) = head.strip_prefix(&[0xfe, 0xff]) {
        return utf16(bytes, u16::from_be_bytes);
    }
    let text = String::from_utf8_lossy(head);
    text.strip_prefix(BOM).unwrap_or(&text).to_owned()
}

/// Decodes UTF-16 bytes with `pair` as the unit decoder, dropping an odd
/// trailing byte. Returns the lossy string and never fails.
fn utf16(bytes: &[u8], pair: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| pair(*chunk))
        .collect();
    String::from_utf16_lossy(&units)
}

pub(super) fn content(text: &str) -> Option<&'static str> {
    if is_xml(text) {
        return Some("xml");
    }
    let mut lines = text.lines();
    let first = lines.next()?;
    first
        .strip_prefix("#!")
        .and_then(shebang)
        .or_else(|| mode_hint(first))
        .or_else(|| lines.next().and_then(mode_hint))
        .or_else(|| is_markup_root(text).then_some("xml"))
        .or_else(|| is_json(text).then_some("json"))
}

fn is_xml(text: &str) -> bool {
    text.trim_start().starts_with("<?xml")
}

/// Recognises markup after leading whitespace or a BOM: a comment, a doctype,
/// or `<` before an ASCII letter or `_`. Returns false for syslog-like `<1>`.
fn is_markup_root(text: &str) -> bool {
    let root = text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let Some(rest) = root.strip_prefix('<') else {
        return false;
    };
    rest.starts_with("!--")
        || root
            .get(..9)
            .is_some_and(|doctype| doctype.eq_ignore_ascii_case("<!DOCTYPE"))
        || rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
}

/// Recognises a JSON object or array from the first non-whitespace bytes.
/// Returns false for section headings like `[network]` and bare `{abc`.
fn is_json(text: &str) -> bool {
    let text = text.trim_start();
    if let Some(rest) = text.strip_prefix('{') {
        let rest = rest.trim_start();
        return rest.starts_with('"') || rest.starts_with('}');
    }
    let Some(rest) = text.strip_prefix('[') else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(first) = rest.chars().next() else {
        return false;
    };
    match first {
        '{' | '[' | '"' | ']' | '-' => true,
        digit if digit.is_ascii_digit() => true,
        _ => ["true", "false", "null"].iter().any(|word| {
            rest.strip_prefix(word)
                .is_some_and(|tail| !tail.starts_with(char::is_alphabetic))
        }),
    }
}

fn shebang(line: &str) -> Option<&'static str> {
    let mut words = line.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    let program = if program == "env" {
        words.find(|word| !word.starts_with('-') && !word.contains('='))?
    } else {
        program
    };
    let program = program
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
        .to_ascii_lowercase();
    INTERPRETERS
        .iter()
        .find(|(name, _)| *name == program)
        .map(|(_, language)| *language)
}

fn mode_hint(line: &str) -> Option<&'static str> {
    emacs_mode(line)
        .or_else(|| vim_mode(line))
        .and_then(names::resolve)
}

fn emacs_mode(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once("-*-")?;
    let (inner, _) = rest.split_once("-*-")?;
    let mode = inner
        .split(';')
        .find_map(|part| part.trim().strip_prefix("mode:"))
        .unwrap_or(inner);
    Some(mode.trim()).filter(|mode| !mode.contains([':', ' ']))
}

fn vim_mode(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once("vim:").or_else(|| line.split_once("vi:"))?;
    rest.split([' ', ':', '\t'])
        .find_map(|part| {
            part.strip_prefix("ft=")
                .or_else(|| part.strip_prefix("filetype="))
                .or_else(|| part.strip_prefix("syntax="))
        })
        .filter(|mode| !mode.is_empty())
}
