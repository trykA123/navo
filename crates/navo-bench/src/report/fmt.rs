use super::targets::Unit;
use crate::privacy::Html;

pub(super) fn millis(micros: u64) -> String {
    format!("{}.{} ms", micros / 1000, (micros % 1000) / 100)
}

fn tenths(value: u64, per: u64) -> String {
    format!("{}.{}", value / per, (value % per) * 10 / per)
}

pub(super) fn megabytes(bytes: u64) -> String {
    format!("{} MB", tenths(bytes, 1_000_000))
}

pub(super) fn mebibytes(bytes: u64) -> String {
    format!("{} MiB", tenths(bytes, 1 << 20))
}

pub(super) fn per_second(milli: u64) -> String {
    format!("{} /s", tenths(milli, 1000))
}

pub(super) fn rate(value: f64) -> String {
    if value.is_finite() && value >= 0.0 {
        format!("{value:.2} /s")
    } else {
        "n/a".into()
    }
}

pub(super) fn amount(value: u64, unit: Unit) -> String {
    match unit {
        Unit::Micros => millis(value),
        Unit::Bytes => megabytes(value),
        Unit::BytesBinary => mebibytes(value),
        Unit::MilliPerSecond => per_second(value),
    }
}

pub(super) fn limit(value: u64, unit: Unit, strict: bool) -> String {
    let sign = if strict { "< " } else { "\u{2264} " };
    format!("{sign}{}", amount(value, unit))
}

pub(super) fn open_table(html: &mut Html, label: &str, heads: &[(&str, bool)]) {
    html.markup("<div class=\"scroll\" role=\"region\" tabindex=\"0\" aria-label=\"")
        .text(label)
        .markup(if heads.len() > 2 {
            "\"><table class=\"wide\"><thead><tr>"
        } else {
            "\"><table><thead><tr>"
        });
    for (name, numeric) in heads {
        html.markup(if *numeric { "<th class=\"n\">" } else { "<th>" })
            .text(name)
            .markup("</th>");
    }
    html.markup("</tr></thead><tbody>");
}

pub(super) fn close_table(html: &mut Html) {
    html.markup("</tbody></table></div>");
}

/// A row; a cell is numeric when its column header was.
pub(super) fn row(html: &mut Html, cells: &[(&str, bool)]) {
    html.markup("<tr>");
    for (text, numeric) in cells {
        html.markup(if *numeric { "<td class=\"n\">" } else { "<td>" })
            .text(text)
            .markup("</td>");
    }
    html.markup("</tr>");
}

pub(super) fn count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
