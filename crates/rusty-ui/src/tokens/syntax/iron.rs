use super::{Role, Style, role};
use crate::tokens::Colors;
use crate::tokens::design_b::ok;

pub(super) fn light(c: &Colors) -> Vec<Role> {
    vec![
        role("keyword", ok(50., 0.125, 40.), Style::Plain),
        role("type", ok(41., 0.08, 82.), Style::Plain),
        role("function", ok(48., 0.075, 210.), Style::Plain),
        role("constant", ok(49., 0.1, 330.), Style::Plain),
        role("boolean", ok(49., 0.1, 330.), Style::Plain),
        role("number", ok(49., 0.1, 330.), Style::Plain),
        role("string", ok(48., 0.1, 145.), Style::Plain),
        role("string.escape", ok(42., 0.1, 355.), Style::Bold),
        role("string.regex", ok(42., 0.1, 355.), Style::Plain),
        role("comment", ok(54., 0.015, 60.), Style::Italic),
        role("comment.doc", ok(48., 0.018, 60.), Style::Italic),
        role("attribute", ok(46., 0.07, 62.), Style::Plain),
        role("tag", ok(43., 0.12, 18.), Style::Plain),
        role("property", ok(43., 0.06, 280.), Style::Plain),
        role("variable", c.text, Style::Plain),
        role("variable.special", ok(43., 0.12, 18.), Style::Italic),
        role("constructor", ok(42., 0.07, 238.), Style::Plain),
        role("operator", ok(42., 0.03, 230.), Style::Plain),
        role("punctuation.delimiter", c.muted, Style::Plain),
        role("punctuation.bracket", c.muted, Style::Plain),
        role("embedded", ok(48., 0.035, 30.), Style::Plain),
        role("title", ok(43., 0.12, 18.), Style::Bold),
        role("link_text", ok(48., 0.075, 210.), Style::Plain),
        role("link_uri", ok(48., 0.035, 30.), Style::Underline),
        role("text.literal", ok(50., 0.125, 40.), Style::Plain),
    ]
}

pub(super) fn dark(c: &Colors) -> Vec<Role> {
    vec![
        role("keyword", ok(75., 0.095, 40.), Style::Plain),
        role("type", ok(85., 0.075, 82.), Style::Plain),
        role("function", ok(80., 0.065, 210.), Style::Plain),
        role("constant", ok(77., 0.075, 330.), Style::Plain),
        role("boolean", ok(77., 0.075, 330.), Style::Plain),
        role("number", ok(77., 0.075, 330.), Style::Plain),
        role("string", ok(80., 0.08, 145.), Style::Plain),
        role("string.escape", ok(86., 0.06, 355.), Style::Bold),
        role("string.regex", ok(86., 0.06, 355.), Style::Plain),
        role("comment", ok(64., 0.014, 60.), Style::Italic),
        role("comment.doc", ok(70., 0.016, 60.), Style::Italic),
        role("attribute", ok(80., 0.06, 62.), Style::Plain),
        role("tag", ok(69., 0.095, 18.), Style::Plain),
        role("property", ok(85., 0.04, 280.), Style::Plain),
        role("variable", c.text, Style::Plain),
        role("variable.special", ok(69., 0.095, 18.), Style::Italic),
        role("constructor", ok(87., 0.04, 238.), Style::Plain),
        role("operator", ok(70., 0.028, 230.), Style::Plain),
        role("punctuation.delimiter", c.muted, Style::Plain),
        role("punctuation.bracket", c.muted, Style::Plain),
        role("embedded", ok(83., 0.028, 30.), Style::Plain),
        role("title", ok(69., 0.095, 18.), Style::Bold),
        role("link_text", ok(80., 0.065, 210.), Style::Plain),
        role("link_uri", ok(83., 0.028, 30.), Style::Underline),
        role("text.literal", ok(75., 0.095, 40.), Style::Plain),
    ]
}
