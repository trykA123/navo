use gpui_kit::Stateful;
use gpui_kit::base::TestSupportExt;
use rusty_automotive::{Body, Entry, Frame, FrameKind};

use super::text::{binary, direction, kind, period, span, time};
use super::{Inspected, TraceView};
use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;

fn caption(t: &Theme, text: &'static str) -> Div {
    div()
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(text)
}

fn field(t: &Theme, label: &'static str, value: String) -> Div {
    div()
        .flex()
        .justify_between()
        .gap(t.spacing.md)
        .child(div().text_color(t.colors.muted).child(label))
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .child(value),
        )
}

fn section(t: &Theme, title: &'static str, rows: Vec<Div>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .child(caption(t, title))
        .children(rows)
}

fn frame_fields(t: &Theme, entry: &Entry, frame: &Frame) -> Vec<Div> {
    let mut rows = vec![
        field(t, "Time", entry.time_ns.map(time).unwrap_or_default()),
        field(t, "Channel", frame.channel.to_string()),
    ];
    if let Some(found) = frame.id {
        let width = if found.extended {
            "extended"
        } else {
            "standard"
        };
        rows.push(field(
            t,
            "ID",
            format!("0x{:X} · {} · {width}", found.raw, found.raw),
        ));
    }
    rows.push(field(t, "Direction", direction(frame.direction).to_owned()));
    rows.push(field(t, "Type", kind(frame.kind).to_owned()));
    if let FrameKind::Fd { brs, esi } = frame.kind {
        let flag = |on: bool| if on { "1" } else { "0" }.to_owned();
        rows.push(field(t, "BRS", flag(brs)));
        rows.push(field(t, "ESI", flag(esi)));
    }
    rows.push(field(t, "DLC", frame.dlc.to_string()));
    rows.push(field(
        t,
        "Length",
        format::count(frame.data.len() as u64, "byte", "bytes"),
    ));
    rows.push(field(t, "Line", format::bytes(entry.line)));
    rows
}

fn byte_rows(t: &Theme, data: &[u8]) -> Vec<Div> {
    data.iter()
        .enumerate()
        .map(|(at, byte)| {
            div()
                .flex()
                .gap(t.spacing.xl)
                .font_family(t.typography.mono_font.clone())
                .child(
                    div()
                        .w(px(24.))
                        .text_color(t.colors.muted)
                        .child(at.to_string()),
                )
                .child(format!("{byte:02X}"))
                .child(div().text_color(t.colors.muted).child(binary(*byte)))
        })
        .collect()
}

fn delta(inspected: &Inspected) -> String {
    let now = inspected.current.as_ref().and_then(|entry| entry.time_ns);
    let before = inspected.previous.as_ref().and_then(|entry| entry.time_ns);
    match (now, before) {
        (Some(now), Some(before)) => format!("+{}", span(now.saturating_sub(before))),
        _ => "first frame of this ID".to_owned(),
    }
}

fn timing(view: &TraceView, t: &Theme, inspected: &Inspected) -> Div {
    let stat = view
        .trace
        .key_of(inspected.row)
        .and_then(|key| view.trace.stats.get(key as usize));
    let since = delta(inspected);
    let mut rows = vec![
        div().child(
            div()
                .id("trace-delta")
                .aria_label(since.clone())
                .test_support()
                .child(field(t, "Since previous", since)),
        ),
    ];
    if let Some(stat) = stat {
        rows.push(field(t, "Frames of this ID", format::bytes(stat.count)));
        rows.push(field(t, "Mean period", period(stat)));
    }
    section(t, "SAME ID", rows)
}

fn other(t: &Theme, entry: &Entry) -> Vec<Div> {
    let (text, reason) = match &entry.body {
        Body::Malformed { text, reason } => (text.clone(), Some(*reason)),
        Body::Other(text) => (text.clone(), None),
        Body::Frame(_) => (String::new(), None),
    };
    let mut rows = vec![field(t, "Line", format::bytes(entry.line))];
    rows.extend(reason.map(|reason| {
        div()
            .text_color(t.colors.warn)
            .child(format!("Malformed: {reason}"))
    }));
    rows.push(
        div()
            .font_family(t.typography.mono_font.clone())
            .text_color(t.colors.muted)
            .child(text),
    );
    rows
}

pub(super) fn inspector_body(view: &TraceView, t: &Theme) -> Stateful<Div> {
    let body = div()
        .id("trace-inspector-body")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap(t.spacing.xl)
        .p(t.spacing.xl);
    let current = view.actual(view.cursor);
    let Some(inspected) = view
        .inspected
        .as_ref()
        .filter(|found| Some(found.row) == current)
    else {
        return body.child(div().text_color(t.colors.muted).child("Reading the row…"));
    };
    let Some(entry) = &inspected.current else {
        return body;
    };
    match &entry.body {
        Body::Frame(frame) => body
            .child(section(t, "FRAME", frame_fields(t, entry, frame)))
            .when(frame.id.is_some(), |body| {
                body.child(timing(view, t, inspected))
            })
            .when(!frame.data.is_empty(), |body| {
                body.child(section(
                    t,
                    "BYTES · HEX · BINARY",
                    byte_rows(t, &frame.data),
                ))
            }),
        Body::Other(_) | Body::Malformed { .. } => body.child(section(t, "LINE", other(t, entry))),
    }
}
