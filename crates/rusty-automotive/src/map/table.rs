pub(super) const MALFORMED: &str = "missing column, invalid number or overflowing range";
pub(super) type Parsed<T> = Result<T, &'static str>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    In,
    Out,
}

#[derive(Clone, Copy)]
enum Unit {
    Mau,
    Hex,
    Dec,
}

pub(super) struct Column {
    pub name: String,
    pub side: Option<Side>,
    unit: Option<Unit>,
}

impl Column {
    fn new(text: &str) -> Self {
        let text = text.trim().to_ascii_lowercase();
        let (side, name) = [
            ("[in]", Side::In),
            ("[out]", Side::Out),
            ("in ", Side::In),
            ("out ", Side::Out),
        ]
        .into_iter()
        .find_map(|(prefix, side)| text.strip_prefix(prefix).map(|name| (Some(side), name)))
        .unwrap_or((None, &text));
        let name = name.trim();
        let (name, unit) = [
            ("(mau)", Unit::Mau),
            ("(hex)", Unit::Hex),
            ("(dec)", Unit::Dec),
        ]
        .into_iter()
        .find_map(|(suffix, unit)| {
            name.strip_suffix(suffix)
                .map(|name| (name.trim(), Some(unit)))
        })
        .unwrap_or((name, None));
        Self {
            name: name.to_owned(),
            side,
            unit,
        }
    }
}

#[derive(Default)]
pub(super) struct Table {
    pub columns: Vec<Column>,
}

pub(super) fn raw_cells(text: &str) -> Vec<&str> {
    let text = text.trim();
    let text = text.strip_prefix('|').unwrap_or(text);
    let text = text.strip_suffix('|').unwrap_or(text);
    text.split('|').collect()
}

fn parse_number(text: &str, unit: Option<Unit>) -> Parsed<u64> {
    let text = text.trim();
    let result = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else if let Some(hex) = text.strip_suffix('h').or_else(|| text.strip_suffix('H')) {
        u64::from_str_radix(hex, 16)
    } else if matches!(unit, Some(Unit::Hex)) {
        u64::from_str_radix(text, 16)
    } else {
        text.parse()
    };
    result.map_err(|_| MALFORMED)
}

impl Table {
    pub(super) fn from_cells(cells: &[&str]) -> Self {
        Self {
            columns: cells.iter().map(|cell| Column::new(cell)).collect(),
        }
    }

    pub(super) fn find_side(&self, names: &[&str], side: Option<Side>) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| column.side == side && names.contains(&column.name.as_str()))
    }

    pub(super) fn find(&self, names: &[&str]) -> Option<usize> {
        self.find_side(names, None)
    }

    pub(super) fn has(&self, names: &[&str]) -> bool {
        self.find(names).is_some()
    }

    pub(super) fn row<'a>(&'a self, cells: &'a [&'a str]) -> Parsed<Row<'a>> {
        if cells.len() != self.columns.len() {
            return Err(MALFORMED);
        }
        Ok(Row { table: self, cells })
    }
}

pub(super) struct Row<'a> {
    pub table: &'a Table,
    pub cells: &'a [&'a str],
}

impl<'a> Row<'a> {
    pub(super) fn get(&self, names: &[&str]) -> Option<&'a str> {
        self.get_at(self.table.find(names)?)
    }

    pub(super) fn get_at(&self, at: usize) -> Option<&'a str> {
        let text = self.cells.get(at)?.trim();
        (!text.is_empty() && text != "-").then_some(text)
    }

    pub(super) fn text(&self, names: &[&str]) -> Parsed<&'a str> {
        self.get(names).ok_or(MALFORMED)
    }

    pub(super) fn num_at(&self, at: usize) -> Parsed<u64> {
        parse_number(
            self.get_at(at).ok_or(MALFORMED)?,
            self.table.columns[at].unit,
        )
    }

    pub(super) fn num(&self, names: &[&str]) -> Parsed<u64> {
        self.num_at(self.table.find(names).ok_or(MALFORMED)?)
    }

    pub(super) fn optional_num(&self, names: &[&str]) -> Parsed<Option<u64>> {
        self.table
            .find(names)
            .filter(|&at| self.get_at(at).is_some())
            .map(|at| self.num_at(at))
            .transpose()
    }

    pub(super) fn owned(&self, names: &[&str]) -> Option<String> {
        self.get(names).map(str::to_owned)
    }
}
