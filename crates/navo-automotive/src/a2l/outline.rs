use std::collections::{BTreeMap, HashMap};

use super::{A2lModel, Module};
use crate::{ItemRef, Outline, OutlineRow};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Links {
    pub users: Vec<Vec<ItemRef>>,
    pub missing: Vec<ItemRef>,
}

#[must_use]
pub fn links(model: &A2lModel) -> Links {
    let methods: HashMap<_, _> = model
        .compu_methods
        .iter()
        .enumerate()
        .map(|(index, method)| ((method.module, method.name.as_str()), index))
        .collect();
    let mut links = Links {
        users: vec![Vec::new(); model.compu_methods.len()],
        missing: Vec::new(),
    };
    let characteristics = model
        .characteristics
        .iter()
        .enumerate()
        .map(|(index, item)| {
            (
                item.module,
                item.conversion.as_str(),
                ItemRef::Characteristic(index as u32),
            )
        });
    let measurements = model.measurements.iter().enumerate().map(|(index, item)| {
        (
            item.module,
            item.conversion.as_str(),
            ItemRef::Measurement(index as u32),
        )
    });
    for (module, conversion, item) in characteristics.chain(measurements) {
        if conversion == "NO_COMPU_METHOD" {
            continue;
        }
        if let Some(&index) = methods.get(&(module, conversion)) {
            links.users[index].push(item);
        } else {
            links.missing.push(item);
        }
    }
    links
}

#[must_use]
pub fn outline(model: &A2lModel) -> Outline {
    let mut outline = Outline::default();
    if let Some(module) = model.modules.first() {
        outline.rows.push(module_row(module));
    }
    append_characteristics(&mut outline, model);
    append_group(&mut outline, "Measurements", model.measurements.len());
    for (index, item) in model.measurements.iter().enumerate() {
        let aux = format!(
            "{} {}",
            item.data_type,
            item.unit.as_deref().unwrap_or_default()
        );
        outline.rows.push(item_row(
            ItemRef::Measurement(index as u32),
            &item.name,
            aux.trim_end(),
            item.line,
        ));
    }
    append_group(&mut outline, "Compu methods", model.compu_methods.len());
    let links = links(model);
    for (index, item) in model.compu_methods.iter().enumerate() {
        let aux = format!("{} · {}", item.conversion_type, links.users[index].len());
        outline.rows.push(item_row(
            ItemRef::Compu(index as u32),
            &item.name,
            &aux,
            item.line,
        ));
    }
    outline
}

fn append_characteristics(outline: &mut Outline, model: &A2lModel) {
    append_group(outline, "Characteristics", model.characteristics.len());
    let mut kinds = BTreeMap::<_, Vec<_>>::new();
    for (index, item) in model.characteristics.iter().enumerate() {
        let rank = ["MAP", "CURVE", "VAL_BLK", "VALUE"]
            .iter()
            .position(|&kind| kind == item.kind)
            .unwrap_or(4);
        kinds
            .entry((rank, item.kind.as_str()))
            .or_default()
            .push((index, item));
    }
    for ((_, kind), items) in kinds {
        outline
            .rows
            .push(group_row(1, kind, &items.len().to_string()));
        for (index, item) in items {
            outline.rows.push(item_row(
                ItemRef::Characteristic(index as u32),
                &item.name,
                &format!("0x{:X}", item.address),
                item.line,
            ));
        }
    }
}

fn append_group(outline: &mut Outline, label: &str, count: usize) {
    if count > 0 {
        outline
            .rows
            .push(group_row(0, &format!("{label} ({count})"), ""));
    }
}

fn module_row(module: &Module) -> OutlineRow {
    let protocol = if module.interface_data.xcp {
        " · XCP"
    } else if module.interface_data.ccp {
        " · CCP"
    } else {
        ""
    };
    let mut row = group_row(0, &format!("Module {}{protocol}", module.name), "");
    row.group = false;
    row.line = module.line;
    row
}

fn group_row(depth: u8, label: &str, aux: &str) -> OutlineRow {
    OutlineRow {
        depth,
        group: true,
        item: None,
        label: label.into(),
        aux: aux.into(),
        line: 0,
        search: format!("{label} {aux}").to_lowercase().into(),
    }
}

fn item_row(item: ItemRef, label: &str, aux: &str, line: u64) -> OutlineRow {
    let depth = if matches!(item, ItemRef::Characteristic(_)) {
        2
    } else {
        1
    };
    OutlineRow {
        group: false,
        item: Some(item),
        line,
        ..group_row(depth, label, aux)
    }
}
