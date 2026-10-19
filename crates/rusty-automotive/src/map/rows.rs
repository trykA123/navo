use super::model::{Contribution, DiscardedSection, MemoryRegion, OutputSection, Symbol};
use super::table::{MALFORMED, Parsed, Row, number};

pub(super) fn memory(row: &Row<'_>) -> Parsed<MemoryRegion> {
    let length = row.num(&["total", "length"])?;
    let free = row.num(&["free"])?;
    let used = length.checked_sub(free).ok_or(MALFORMED)?;
    let code = row.num(&["code"])?;
    let data = row.num(&["data"])?;
    let reserved = row.num(&["reserved"])?;
    if code.checked_add(data).and_then(|n| n.checked_add(reserved)) != Some(used) {
        return Err(MALFORMED);
    }
    let origin = row.optional_num(&["origin", "start address"])?;
    if origin.is_some_and(|origin| origin.checked_add(length).is_none()) {
        return Err(MALFORMED);
    }
    Ok(MemoryRegion {
        name: row.text(&["memory"])?.to_owned(),
        origin,
        length,
        used,
        free,
    })
}

pub(super) fn section(row: &Row<'_>, space: Option<&str>) -> Parsed<OutputSection> {
    let address = row.num(&["space addr", "start address", "address"])?;
    let size = row.num(&["size (mau)", "size", "sect. size (hex)", "sect. size (dec)"])?;
    address.checked_add(size).ok_or(MALFORMED)?;
    Ok(OutputSection {
        name: row.text(&["section", "section name"])?.to_owned(),
        address,
        size,
        region: row.owned(&["chip", "chip name", "memory"]),
        address_space: row
            .owned(&["space", "address space"])
            .or_else(|| space.map(str::to_owned)),
    })
}

pub(super) fn symbol(row: &Row<'_>, space: Option<&str>) -> Parsed<Symbol> {
    let address = row.num(&["address", "value"])?;
    let size = row.optional_num(&["size"])?;
    if size.is_some_and(|size| address.checked_add(size).is_none()) {
        return Err(MALFORMED);
    }
    Ok(Symbol {
        name: row.text(&["name", "symbol"])?.to_owned(),
        address,
        size,
        section: row.owned(&["section"]),
        address_space: row
            .owned(&["space", "address space"])
            .or_else(|| space.map(str::to_owned)),
    })
}

pub(super) fn discarded(row: &Row<'_>) -> Parsed<DiscardedSection> {
    Ok(DiscardedSection {
        name: row.text(&["section"])?.to_owned(),
        object_file: row.owned(&["file", "object file"]),
        library: row.owned(&["library"]),
        symbol: row.owned(&["symbol"]),
        reason: row.owned(&["reason"]),
        size: row.optional_num(&["size"])?,
    })
}

pub(super) fn contribution(row: &Row<'_>, file: &str) -> Parsed<Contribution> {
    let input = row
        .table
        .find(&["[in] section", "in section", "section"])
        .ok_or(MALFORMED)?;
    let offset = row
        .table
        .find(&["[out] offset", "out offset", "offset"])
        .ok_or(MALFORMED)?;
    let output = output_column(row, offset)?;
    let input_size = row.num(&["[in] size", "in size", "size"])?;
    let offset_value = number(row.cells[offset])?;
    let output_size = output_size(row, output)?;
    let end = offset_value.checked_add(input_size).ok_or(MALFORMED)?;
    if output_size.is_some_and(|size| end > size)
        || file.is_empty()
        || row.cells[input].is_empty()
        || row.cells[output].is_empty()
    {
        return Err(MALFORMED);
    }
    Ok(Contribution {
        object_file: file.to_owned(),
        section: row.cells[input].to_owned(),
        size: input_size,
        address: None,
        output_section: row.cells[output].to_owned(),
        offset: offset_value,
    })
}

fn output_column(row: &Row<'_>, offset: usize) -> Parsed<usize> {
    row.table
        .find(&["[out] section", "out section"])
        .or_else(|| {
            row.table
                .columns
                .iter()
                .enumerate()
                .skip(offset + 1)
                .find(|(_, name)| *name == "section")
                .map(|(at, _)| at)
        })
        .ok_or(MALFORMED)
}

fn output_size(row: &Row<'_>, output: usize) -> Parsed<Option<u64>> {
    row.table
        .columns
        .iter()
        .enumerate()
        .skip(output + 1)
        .find(|(_, name)| ["size", "[out] size", "out size"].contains(&name.as_str()))
        .map(|(at, _)| number(row.cells[at]))
        .transpose()
}
