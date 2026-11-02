use std::collections::HashMap;

use super::{DbcModel, Message};
use crate::{ItemRef, Outline, OutlineRow};

#[must_use]
pub fn outline(model: &DbcModel) -> Outline {
    let nodes: HashMap<_, _> = model
        .nodes
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect();
    let mut groups = vec![Vec::new(); model.nodes.len() + 1];
    for (index, message) in model.messages.iter().enumerate() {
        let node = message
            .sender
            .as_deref()
            .filter(|name| !name.is_empty() && *name != "Vector__XXX")
            .and_then(|name| nodes.get(name))
            .copied()
            .unwrap_or(model.nodes.len());
        groups[node].push((index, message));
    }
    let mut outline = Outline::default();
    for (index, messages) in groups.iter().enumerate() {
        let node = model.nodes.get(index);
        if node.is_none() && messages.is_empty() {
            continue;
        }
        let label = node.map_or("No sender", String::as_str);
        let aux = format!("{} msg", messages.len());
        outline.rows.push(OutlineRow {
            depth: 0,
            group: true,
            item: node.map(|_| ItemRef::Node(index as u32)),
            label: label.into(),
            search: format!("{label} {aux}").to_lowercase().into(),
            aux: aux.into(),
            line: 0,
        });
        for &(index, message) in messages {
            append_message(&mut outline, index as u32, message);
        }
    }
    outline
}

fn append_message(outline: &mut Outline, index: u32, message: &Message) {
    let suffix = if message.id.extended {
        " (extended)"
    } else {
        ""
    };
    let label = format!("0x{:X} {}{suffix}", message.id.raw, message.name);
    let aux = message
        .cycle_time_ms
        .map(|cycle| format!("{cycle} ms"))
        .unwrap_or_default();
    outline.rows.push(OutlineRow {
        depth: 1,
        group: true,
        item: Some(ItemRef::Message(index)),
        search: format!("{label} {aux}").to_lowercase().into(),
        label: label.into(),
        aux: aux.into(),
        line: message.line,
    });
    for (signal_index, signal) in message.signals.iter().enumerate() {
        let aux = format!("{}|{} {}", signal.start_bit, signal.length, signal.unit);
        let aux = aux.trim_end();
        outline.rows.push(OutlineRow {
            depth: 2,
            group: false,
            item: Some(ItemRef::Signal {
                message: index,
                signal: u32::try_from(signal_index).unwrap_or(u32::MAX),
            }),
            label: signal.name.as_str().into(),
            search: format!("{} {aux}", signal.name).to_lowercase().into(),
            aux: aux.into(),
            line: signal.line,
        });
    }
}
