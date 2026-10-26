use std::{collections::HashMap, sync::Arc};

use super::{CommentTarget, DbcModel, Message, Number, Signal, Value, ValueTarget};
use crate::{DescRow, Endian};

const HEADERS: [&str; 16] = [
    "Name",
    "ID",
    "DLC",
    "Sender",
    "Cycle time (ms)",
    "Comment",
    "Start bit",
    "Length",
    "Byte order",
    "Factor",
    "Offset",
    "Unit",
    "Min",
    "Max",
    "Receivers",
    "Values",
];

#[must_use]
pub fn rows(model: &DbcModel) -> (Vec<&'static str>, Vec<DescRow>) {
    let comments: HashMap<_, _> = model
        .comments
        .iter()
        .filter_map(|comment| match comment.target {
            CommentTarget::Message(id) => Some((id, comment.text.as_str())),
            _ => None,
        })
        .collect();
    let values: HashMap<_, _> = model
        .value_tables
        .iter()
        .filter_map(|table| match &table.target {
            ValueTarget::Signal { message, signal } => {
                Some(((*message, signal.as_str()), table.values.as_slice()))
            }
            _ => None,
        })
        .collect();
    let mut rows = Vec::new();
    for message in &model.messages {
        let comment = comments.get(&message.id).copied().unwrap_or_default();
        rows.push(message_row(message, comment));
        for signal in &message.signals {
            let values = values
                .get(&(message.id, signal.name.as_str()))
                .copied()
                .unwrap_or_default();
            rows.push(signal_row(signal, values));
        }
    }
    (HEADERS.to_vec(), rows)
}

fn message_row(message: &Message, comment: &str) -> DescRow {
    let suffix = if message.id.extended {
        " (extended)"
    } else {
        ""
    };
    let mut cells: Vec<Arc<str>> = vec![
        message.name.as_str().into(),
        format!("0x{:X}{suffix}", message.id.raw).into(),
        message.dlc.to_string().into(),
        message.sender.as_deref().unwrap_or_default().into(),
        message
            .cycle_time_ms
            .map(|cycle| cycle.to_string())
            .unwrap_or_default()
            .into(),
        comment.into(),
    ];
    cells.resize(HEADERS.len(), "".into());
    DescRow::new(0, cells)
}

fn signal_row(signal: &Signal, values: &[Value]) -> DescRow {
    let byte_order = match signal.byte_order {
        Endian::Little => "Intel",
        Endian::Big => "Motorola",
    };
    DescRow::new(
        1,
        vec![
            signal.name.as_str().into(),
            "".into(),
            "".into(),
            "".into(),
            "".into(),
            "".into(),
            signal.start_bit.to_string().into(),
            signal.length.to_string().into(),
            byte_order.into(),
            signal.factor.to_string().into(),
            signal.offset.to_string().into(),
            signal.unit.as_str().into(),
            number(signal.min).into(),
            number(signal.max).into(),
            signal.receivers.join(", ").into(),
            value_descriptions(values).into(),
        ],
    )
}

fn number(number: Number) -> String {
    match number {
        Number::Unsigned(value) => value.to_string(),
        Number::Signed(value) => value.to_string(),
        Number::Float(value) => value.to_string(),
    }
}

fn value_descriptions(values: &[Value]) -> String {
    let mut text = values
        .iter()
        .take(4)
        .map(|value| format!("{}={}", value.value, value.label))
        .collect::<Vec<_>>()
        .join(", ");
    if values.len() > 4 {
        text.push_str(", …");
    }
    text
}
