use rusty_automotive::map::{MapFlavor, MapModel, MapProblem};

pub(super) use super::super::elf::parse_address;
use super::loaded::Loaded;
use super::select::{Column, Head, Table, per_mille};
use crate::format;

pub(super) const LOSSY: &str = "lossy UTF-8 decoding was required";
pub(super) const MAU: &str = "only 8-bit minimum addressable units are supported";
pub(super) const INCONSISTENT: &str =
    "memory code, data and reserved do not match total minus free";

pub(super) const EMPTY: &str = "–";

pub(super) struct Check {
    pub text: String,
    pub is_bad: bool,
}

fn check(text: String, is_bad: bool) -> Check {
    Check { text, is_bad }
}

pub(super) fn address(at: u64) -> String {
    if at > u64::from(u32::MAX) {
        format!("0x{at:016X}")
    } else {
        format!("0x{at:08X}")
    }
}

pub(super) fn span(start: u64, size: u64) -> String {
    if size == 0 {
        return address(start);
    }
    format!(
        "{}–{}",
        address(start),
        address(start.saturating_add(size - 1))
    )
}

pub(super) fn percent(used: u64, length: u64) -> String {
    per_mille(used, length).map_or_else(
        || EMPTY.to_owned(),
        |tenths| format!("{}.{}%", tenths / 10, tenths % 10),
    )
}

pub(super) fn flavor(flavor: MapFlavor) -> &'static str {
    match flavor {
        MapFlavor::Tasking => "TASKING",
        _ => "Linker",
    }
}

fn optional(text: Option<&str>) -> String {
    text.map_or_else(|| EMPTY.to_owned(), str::to_owned)
}

pub(super) fn heads(loaded: &Loaded, table: Table) -> Vec<Head> {
    let hides_origin = |head: &Head| {
        table == Table::Memory && head.column == Column::Address && !loaded.has_origins
    };
    table
        .heads()
        .iter()
        .filter(|head| !hides_origin(head))
        .copied()
        .collect()
}

pub(super) fn cells(loaded: &Loaded, table: Table, row: usize) -> Vec<String> {
    let model = &loaded.model;
    match table {
        Table::Memory => model.regions.get(row).map_or_else(Vec::new, |region| {
            let origin = loaded
                .has_origins
                .then(|| region.origin.map_or_else(|| EMPTY.to_owned(), address));
            [Some(region.name.clone()), origin]
                .into_iter()
                .flatten()
                .chain([
                    format::bytes(region.length),
                    format::bytes(region.used),
                    format::bytes(region.free),
                    percent(region.used, region.length),
                ])
                .collect()
        }),
        Table::Sections => model.sections.get(row).map_or_else(Vec::new, |section| {
            vec![
                section.name.clone(),
                address(section.address),
                format::bytes(section.size),
                optional(section.region.as_deref()),
                optional(section.address_space.as_deref()),
            ]
        }),
        Table::Symbols => model.symbols.get(row).map_or_else(Vec::new, |symbol| {
            vec![
                symbol.name.clone(),
                address(symbol.address),
                symbol.size.map_or_else(|| EMPTY.to_owned(), format::bytes),
                optional(symbol.address_space.as_deref()),
            ]
        }),
    }
}

pub(super) fn row_label(loaded: &Loaded, table: Table, row: usize) -> String {
    cells(loaded, table, row).join(", ")
}

pub(super) fn summary(loaded: &Loaded, table: Table, row: usize) -> Option<String> {
    let cells = cells(loaded, table, row);
    let (name, second) = (cells.first()?, cells.get(1)?);
    Some(match table {
        Table::Memory => format!("{name} · {}", cells.last()?),
        Table::Sections | Table::Symbols => format!("{name} · {second}"),
    })
}

#[derive(Default)]
struct Tally {
    lossy: bool,
    mau: usize,
    inconsistent: usize,
    other: usize,
    first_other: Option<(u64, &'static str)>,
    caps: Vec<(u64, &'static str)>,
}

fn tally(model: &MapModel) -> Tally {
    let mut tally = Tally::default();
    for problem in &model.problems {
        match problem {
            MapProblem::Malformed { reason, .. } if *reason == LOSSY => tally.lossy = true,
            MapProblem::Malformed { reason, .. } if *reason == MAU => tally.mau += 1,
            MapProblem::Malformed { reason, .. } if *reason == INCONSISTENT => {
                tally.inconsistent += 1;
            }
            MapProblem::Malformed { line, reason } => {
                tally.other += 1;
                tally.first_other.get_or_insert((*line, reason));
            }
            MapProblem::CapReached { line, limit } => tally.caps.push((*line, limit)),
        }
    }
    let kept = u64::try_from(model.problems.len()).unwrap_or(u64::MAX);
    let unlisted = model.problem_total.saturating_sub(kept);
    tally.other += usize::try_from(unlisted).unwrap_or(usize::MAX);
    tally
}

fn problem_checks(model: &MapModel) -> Vec<Check> {
    let tally = tally(model);
    let mut checks = Vec::new();
    if tally.lossy {
        checks.push("some text was not valid UTF-8 and was decoded lossily".to_owned());
    }
    if tally.mau > 0 {
        let spaces = format::count(tally.mau as u64, "address space", "address spaces");
        checks.push(format!("{spaces} with a unit other than 8 bit skipped"));
    }
    if tally.inconsistent > 0 {
        let regions = format::count(tally.inconsistent as u64, "region", "regions");
        checks.push(format!(
            "{regions} where code + data + reserved differ from used"
        ));
    }
    if tally.other > 0 {
        let rows = format::count(tally.other as u64, "row", "rows");
        let first = tally
            .first_other
            .map_or_else(String::new, |(line, reason)| {
                format!(" (line {line}: {reason})")
            });
        checks.push(format!("{rows} could not be read{first}"));
    }
    checks.extend(tally.caps.iter().map(|(line, limit)| {
        format!("limit reached at line {line} ({limit}): the lists are incomplete")
    }));
    checks.into_iter().map(|text| check(text, true)).collect()
}

pub(super) fn status(loaded: &Loaded) -> Vec<Check> {
    let model = &loaded.model;
    let count = |n: usize, one, many| format::count(n as u64, one, many);
    let mut checks = problem_checks(model);
    checks.extend([
        check(flavor(model.flavor).to_owned(), false),
        check(count(model.regions.len(), "region", "regions"), false),
        check(count(model.sections.len(), "section", "sections"), false),
        check(count(model.symbols.len(), "symbol", "symbols"), false),
        check(
            format!(
                "used {} · free {}",
                format::size(loaded.used()),
                format::size(loaded.free())
            ),
            false,
        ),
    ]);
    if !model.skipped_parts.is_empty() {
        let parts = format::count(
            model.skipped_parts.len() as u64,
            "other part",
            "other parts",
        );
        checks.push(check(format!("{parts} not shown"), false));
    }
    checks
}
