type Marks = &'static [(&'static [&'static str], &'static str)];

pub(super) const MARKED: [(&str, Marks); 3] = [
    ("m", &[(OBJC_MARKS, "objc")]),
    ("cls", &[(LATEX_MARKS, "latex")]),
    (
        "inc",
        &[(PHP_MARKS, "php"), (C_MARKS, "c"), (ASM_MARKS, "asm")],
    ),
];

pub(super) const CONF: &str = "conf";
#[cfg(test)]
pub(super) const CONF_LANGUAGES: [&str; 2] = [INI, PROPERTIES];
const INI: &str = "ini";
const PROPERTIES: &str = "properties";

const OBJC_MARKS: &[&str] = &[
    "#import",
    "#include",
    "@interface",
    "@implementation",
    "@protocol",
    "@class",
];
const LATEX_MARKS: &[&str] = &["\\documentclass", "\\ProvidesClass", "\\NeedsTeXFormat"];
const PHP_MARKS: &[&str] = &["<?php"];
const C_MARKS: &[&str] = &[
    "#include", "#define", "#ifndef", "#ifdef", "#if ", "#pragma",
];
const ASM_MARKS: &[&str] = &[";", "%macro", "%define", ".macro", "section ", ".equ"];

pub(super) fn is_ambiguous(ext: &str) -> bool {
    ext == CONF || MARKED.iter().any(|(known, _)| *known == ext)
}

pub(super) fn resolve(ext: &str, text: &str) -> Option<&'static str> {
    if ext == CONF {
        return conf_file(text);
    }
    let (_, marks) = MARKED.iter().find(|(known, _)| *known == ext)?;
    marks
        .iter()
        .find(|(marks, _)| starts_any(text, marks))
        .map(|(_, language)| *language)
}

fn starts_any(text: &str, marks: &[&str]) -> bool {
    text.lines()
        .map(str::trim_start)
        .any(|line| marks.iter().any(|mark| line.starts_with(mark)))
}

fn conf_file(text: &str) -> Option<&'static str> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(['#', ';']));
    if lines
        .clone()
        .any(|line| line.starts_with('[') && line.ends_with(']'))
    {
        return Some(INI);
    }
    let (key, _) = lines.next()?.split_once('=')?;
    let key = key.trim();
    (!key.is_empty() && !key.contains(char::is_whitespace)).then_some(PROPERTIES)
}
