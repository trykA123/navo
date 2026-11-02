use super::{Role, Style, role};
use crate::tokens::Colors;
use crate::tokens::design_b::ok;

pub(super) fn light(c: &Colors) -> Vec<Role> {
    vec![
        role("keyword", ok(50., 0.16, 305.), Style::Plain),
        role("type", ok(52., 0.1, 85.), Style::Plain),
        role("function", ok(50., 0.15, 262.), Style::Plain),
        role("constant", ok(52., 0.12, 55.), Style::Plain),
        role("boolean", ok(52., 0.12, 55.), Style::Plain),
        role("number", ok(52., 0.12, 55.), Style::Plain),
        role("string", ok(50., 0.12, 150.), Style::Plain),
        role("string.escape", ok(52., 0.15, 340.), Style::Plain),
        role("string.regex", ok(52., 0.15, 340.), Style::Plain),
        role("comment", ok(54., 0.018, 260.), Style::Italic),
        role("comment.doc", ok(48., 0.02, 260.), Style::Italic),
        role("attribute", ok(50., 0.12, 10.), Style::Plain),
        role("tag", ok(53., 0.2, 30.), Style::Plain),
        role("property", ok(50., 0.14, 282.), Style::Plain),
        role("variable", c.text, Style::Plain),
        role("variable.special", ok(53., 0.2, 30.), Style::Italic),
        role("constructor", ok(50., 0.11, 240.), Style::Plain),
        role("operator", ok(50., 0.09, 222.), Style::Plain),
        role("punctuation.delimiter", ok(48., 0.02, 260.), Style::Plain),
        role("punctuation.bracket", ok(48., 0.02, 260.), Style::Plain),
        role("embedded", ok(50., 0.06, 25.), Style::Plain),
        role("title", ok(53., 0.2, 30.), Style::Bold),
        role("link_text", ok(50., 0.14, 282.), Style::Plain),
        role("link_uri", ok(50., 0.06, 25.), Style::Plain),
        role("text.literal", ok(50., 0.085, 185.), Style::Plain),
    ]
}

pub(super) fn dark(c: &Colors) -> Vec<Role> {
    vec![
        role("keyword", ok(75., 0.12, 305.), Style::Plain),
        role("type", ok(82., 0.11, 85.), Style::Plain),
        role("function", ok(76., 0.1, 262.), Style::Plain),
        role("constant", ok(78., 0.11, 55.), Style::Plain),
        role("boolean", ok(78., 0.11, 55.), Style::Plain),
        role("number", ok(78., 0.11, 55.), Style::Plain),
        role("string", ok(75., 0.1, 150.), Style::Plain),
        role("string.escape", ok(75., 0.11, 340.), Style::Plain),
        role("string.regex", ok(75., 0.11, 340.), Style::Plain),
        role("comment", ok(64., 0.016, 260.), Style::Italic),
        role("comment.doc", ok(70., 0.018, 260.), Style::Italic),
        role("attribute", ok(80., 0.07, 10.), Style::Plain),
        role("tag", ok(72., 0.16, 30.), Style::Plain),
        role("property", ok(78., 0.09, 282.), Style::Plain),
        role("variable", c.text, Style::Plain),
        role("variable.special", ok(72., 0.16, 30.), Style::Italic),
        role("constructor", ok(76., 0.09, 240.), Style::Plain),
        role("operator", ok(80., 0.08, 222.), Style::Plain),
        role("punctuation.delimiter", ok(70., 0.018, 260.), Style::Plain),
        role("punctuation.bracket", ok(70., 0.018, 260.), Style::Plain),
        role("embedded", ok(84., 0.04, 25.), Style::Plain),
        role("title", ok(72., 0.16, 30.), Style::Bold),
        role("link_text", ok(78., 0.09, 282.), Style::Plain),
        role("link_uri", ok(84., 0.04, 25.), Style::Plain),
        role("text.literal", ok(78., 0.08, 185.), Style::Plain),
    ]
}
