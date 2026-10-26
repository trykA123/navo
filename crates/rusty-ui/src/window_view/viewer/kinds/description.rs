use std::io::BufReader;

use rusty_automotive::{DescRow, Watch, a2l, dbc, index_a2l, index_dbc, sniff_a2l, sniff_dbc};
use rusty_structured::table::{Dialect, Table};

use super::{Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::background::{Ranged, ranged};
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::table::{TableFile, TableLoad};

pub(super) struct DbcKind;
pub(super) struct A2lKind;

impl ViewerKind for DbcKind {
    fn id(&self) -> &'static str {
        "dbc"
    }

    fn name(&self) -> &'static str {
        "DBC"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if named(sniff.name, &["dbc"]) && !sniff.looks_binary() && sniff_dbc(sniff.head) {
            70
        } else {
            0
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        load.ensure_open()?;
        let model = index_dbc(
            reader(&load),
            Watch {
                cancel: load.cancel,
                progress: &mut |_| {},
            },
        )
        .map_err(|error| error.to_string())?;
        let (headers, rows) = dbc::rows(&model);
        load.ensure_open()?;
        Ok(Box::new(flatten(load.sniff.len, headers, rows)))
    }

    fn attach(
        &self,
        attach: Attach,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        super::table::TableKind.attach(attach, window, cx)
    }
}

impl ViewerKind for A2lKind {
    fn id(&self) -> &'static str {
        "a2l"
    }

    fn name(&self) -> &'static str {
        "A2L"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if named(sniff.name, &["a2l"]) && !sniff.looks_binary() && sniff_a2l(sniff.head) {
            70
        } else {
            0
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        load.ensure_open()?;
        let model = index_a2l(
            reader(&load),
            Watch {
                cancel: load.cancel,
                progress: &mut |_| {},
            },
        )
        .map_err(|error| error.to_string())?;
        let (headers, rows) = a2l::rows(&model);
        load.ensure_open()?;
        Ok(Box::new(flatten(load.sniff.len, headers, rows)))
    }

    fn attach(
        &self,
        attach: Attach,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        super::table::TableKind.attach(attach, window, cx)
    }
}

fn reader(load: &Load<'_>) -> impl std::io::BufRead + Send + 'static {
    BufReader::new(Ranged {
        read: ranged(load.hooks, load.location),
        at: 0,
        len: load.sniff.len,
    })
}

fn flatten(len: u64, headers: Vec<&str>, rows: Vec<DescRow>) -> TableFile {
    let columns = headers.len();
    let rows = rows
        .into_iter()
        .map(|row| {
            let mut cells: Vec<String> = row.cells.iter().map(ToString::to_string).collect();
            if row.depth == 1
                && let Some(first) = cells.first_mut()
            {
                first.insert_str(0, "  ");
            }
            cells
        })
        .collect();
    TableFile {
        len,
        load: TableLoad::Memory {
            table: Table {
                header: headers.into_iter().map(str::to_owned).collect(),
                rows,
                columns,
                widest: columns,
            },
            dialect: Dialect {
                delimiter: b'\t',
                has_header: true,
            },
        },
    }
}

#[cfg(test)]
mod tests;
