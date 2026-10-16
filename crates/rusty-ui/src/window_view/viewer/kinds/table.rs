use rusty_core::Location;
use rusty_structured::SMALL_LIMIT;
use rusty_structured::table::{Table, first_record, header_of, parse_table, sniff as dialect_of};

use super::{Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::FileHooks;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::table::{TableBody, TableFile, TableLoad};

const TABLE: u8 = 70;
const SNIFF_BYTES: usize = 64 * 1024;

fn big(head: &[u8]) -> TableLoad {
    let dialect = dialect_of(head);
    let first = first_record(head, dialect);
    let columns = first.len();
    let header = header_of(dialect.has_header.then_some(first), columns);
    TableLoad::Indexed {
        table: Table {
            header,
            rows: Vec::new(),
            columns,
            widest: columns,
        },
        dialect,
    }
}

pub(super) struct TableKind;

impl ViewerKind for TableKind {
    fn id(&self) -> &'static str {
        "table"
    }

    fn name(&self) -> &'static str {
        "Table"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if named(sniff.name, &["csv", "tsv"]) && !sniff.looks_binary() {
            TABLE
        } else {
            0
        }
    }

    fn load(
        &self,
        hooks: &FileHooks,
        location: &Location,
        sniff: &Sniff<'_>,
    ) -> Result<Loaded, String> {
        let load = if sniff.len > SMALL_LIMIT {
            big(sniff.head)
        } else {
            let bytes = (hooks.bytes)(location, SMALL_LIMIT)?;
            let dialect = dialect_of(bytes.get(..SNIFF_BYTES).unwrap_or(&bytes));
            let table = parse_table(&bytes, dialect).map_err(|error| error.to_string())?;
            TableLoad::Memory { table, dialect }
        };
        Ok(Box::new(TableFile {
            len: sniff.len,
            load,
        }))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let file = attach
            .loaded
            .downcast::<TableFile>()
            .map_err(|_| "The table view got data it cannot show.".to_owned())?;
        Ok(Box::new(TableBody::new(*file, attach.host, cx)))
    }
}
