type Resolve = fn(&str) -> Option<&'static str>;

const AMBIGUOUS: [(&str, Resolve); 4] = [
    ("m", objective_c),
    ("cls", class_file),
    ("inc", include_file),
    ("conf", conf_file),
];

const OBJC_MARKS: [&str; 6] = [
    "#import",
    "#include",
    "@interface",
    "@implementation",
    "@protocol",
    "@class",
];
const LATEX_MARKS: [&str; 3] = ["\\documentclass", "\\ProvidesClass", "\\NeedsTeXFormat"];
const C_MARKS: [&str; 6] = [
    "#include", "#define", "#ifndef", "#ifdef", "#if ", "#pragma",
];
const ASM_MARKS: [&str; 6] = [";", "%macro", "%define", ".macro", "section ", ".equ"];

pub(super) fn resolver(ext: &str) -> Option<Resolve> {
    AMBIGUOUS
        .iter()
        .find(|(known, _)| *known == ext)
        .map(|(_, resolve)| *resolve)
}

fn starts_any(text: &str, marks: &[&str]) -> bool {
    text.lines()
        .map(str::trim_start)
        .any(|line| marks.iter().any(|mark| line.starts_with(mark)))
}

fn objective_c(text: &str) -> Option<&'static str> {
    starts_any(text, &OBJC_MARKS).then_some("objc")
}

fn class_file(text: &str) -> Option<&'static str> {
    starts_any(text, &LATEX_MARKS).then_some("latex")
}

fn include_file(text: &str) -> Option<&'static str> {
    if text.trim_start().starts_with("<?php") {
        Some("php")
    } else if starts_any(text, &C_MARKS) {
        Some("c")
    } else if starts_any(text, &ASM_MARKS) {
        Some("asm")
    } else {
        None
    }
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
        return Some("ini");
    }
    let (key, _) = lines.next()?.split_once('=')?;
    let key = key.trim();
    (!key.is_empty() && !key.contains(char::is_whitespace)).then_some("properties")
}
