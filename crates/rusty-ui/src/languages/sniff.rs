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
    let text = String::from_utf8_lossy(head);
    text.strip_prefix(BOM).unwrap_or(&text).to_owned()
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
}

fn is_xml(text: &str) -> bool {
    text.trim_start().starts_with("<?xml")
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
