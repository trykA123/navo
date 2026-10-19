pub(super) const MALFORMED: &str = "missing column, invalid number or overflowing range";
pub(super) type Parsed<T> = Result<T, &'static str>;

#[derive(Default)]
pub(super) struct Table {
    pub columns: Vec<String>,
}

pub(super) fn cells(text: &str) -> Vec<&str> {
    let text = text.trim();
    let text = text.strip_prefix('|').unwrap_or(text);
    let text = text.strip_suffix('|').unwrap_or(text);
    text.split('|').map(str::trim).collect()
}

pub(super) fn number(text: &str) -> Parsed<u64> {
    let text = text.trim();
    let result = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else if let Some(hex) = text.strip_suffix('h').or_else(|| text.strip_suffix('H')) {
        u64::from_str_radix(hex, 16)
    } else {
        text.parse()
    };
    result.map_err(|_| MALFORMED)
}

impl Table {
    pub(super) fn from_cells(cells: &[&str]) -> Self {
        Self {
            columns: cells.iter().map(|cell| cell.to_ascii_lowercase()).collect(),
        }
    }

    pub(super) fn find(&self, names: &[&str]) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| names.contains(&column.as_str()))
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
        let text = self.cells.get(self.table.find(names)?)?.trim();
        (!text.is_empty() && text != "-").then_some(text)
    }

    pub(super) fn text(&self, names: &[&str]) -> Parsed<&'a str> {
        self.get(names).ok_or(MALFORMED)
    }

    pub(super) fn num(&self, names: &[&str]) -> Parsed<u64> {
        number(self.text(names)?)
    }

    pub(super) fn optional_num(&self, names: &[&str]) -> Parsed<Option<u64>> {
        self.get(names).map(number).transpose()
    }

    pub(super) fn owned(&self, names: &[&str]) -> Option<String> {
        self.get(names).map(str::to_owned)
    }
}
