use std::sync::Arc;

use super::{A2lModel, Characteristic, CompuMethod, Measurement};
use crate::DescRow;

const HEADERS: [&str; 9] = [
    "Name",
    "Address",
    "Data type",
    "Kind",
    "Type",
    "Conversion",
    "Unit",
    "Min",
    "Max",
];

#[must_use]
pub fn rows(model: &A2lModel) -> (Vec<&'static str>, Vec<DescRow>) {
    let mut rows = Vec::new();
    append_group(
        &mut rows,
        "Measurements",
        &model.measurements,
        measurement_row,
    );
    append_group(
        &mut rows,
        "Characteristics",
        &model.characteristics,
        characteristic_row,
    );
    append_group(
        &mut rows,
        "Conversions",
        &model.compu_methods,
        conversion_row,
    );
    (HEADERS.to_vec(), rows)
}

fn append_group<T>(rows: &mut Vec<DescRow>, name: &str, items: &[T], row: fn(&T) -> DescRow) {
    if items.is_empty() {
        return;
    }
    let mut cells = vec![Arc::from(""); HEADERS.len()];
    cells[0] = format!("{name} ({})", items.len()).into();
    rows.push(DescRow::new(0, cells));
    rows.extend(items.iter().map(row));
}

fn measurement_row(item: &Measurement) -> DescRow {
    DescRow::new(
        1,
        vec![
            item.name.as_str().into(),
            item.address
                .map(|address| format!("0x{address:X}"))
                .unwrap_or_default()
                .into(),
            item.data_type.as_str().into(),
            "".into(),
            "".into(),
            item.conversion.as_str().into(),
            item.unit.clone().unwrap_or_else(|| "".into()),
            item.lower_limit.to_string().into(),
            item.upper_limit.to_string().into(),
        ],
    )
}

fn characteristic_row(item: &Characteristic) -> DescRow {
    let kind = match item.axes.len() {
        0 => item.kind.clone(),
        1 => format!("{} 1 axis", item.kind),
        count => format!("{} {count} axes", item.kind),
    };
    DescRow::new(
        1,
        vec![
            item.name.as_str().into(),
            format!("0x{:X}", item.address).into(),
            "".into(),
            kind.into(),
            "".into(),
            item.conversion.as_str().into(),
            "".into(),
            item.lower_limit.to_string().into(),
            item.upper_limit.to_string().into(),
        ],
    )
}

fn conversion_row(item: &CompuMethod) -> DescRow {
    DescRow::new(
        1,
        vec![
            item.name.as_str().into(),
            "".into(),
            "".into(),
            "".into(),
            item.conversion_type.as_str().into(),
            "".into(),
            item.unit.clone(),
            "".into(),
            "".into(),
        ],
    )
}
