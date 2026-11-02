use super::model::{Contribution, DiscardedSection, MemoryRegion, OutputSection, Symbol};
use super::table::{MALFORMED, Parsed, Row, Side};

pub(super) fn memory(row: &Row<'_>) -> Parsed<(MemoryRegion, bool)> {
    let length = row.num(&["total", "length"])?;
    let free = row.num(&["free"])?;
    let used = length.checked_sub(free).ok_or(MALFORMED)?;
    let code = row.num(&["code"])?;
    let data = row.num(&["data"])?;
    let reserved = row.num(&["reserved"])?;
    let consistent = code.checked_add(data).and_then(|n| n.checked_add(reserved)) == Some(used);
    let origin = row.optional_num(&["origin", "start address"])?;
    if origin.is_some_and(|origin| origin.checked_add(length).is_none()) {
        return Err(MALFORMED);
    }
    Ok((
        MemoryRegion {
            name: row.text(&["memory"])?.to_owned(),
            origin,
            length,
            used,
            free,
            reserved,
        },
        consistent,
    ))
}

pub(super) fn section(row: &Row<'_>, space: Option<&str>) -> Parsed<OutputSection> {
    let address = row.num(&["space addr", "start address", "address"])?;
    let size = row.num(&["size", "sect. size"])?;
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
    let address = row.num(&["address", "value", "space addr", "chip addr"])?;
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
        .find_side(&["section"], Some(Side::In))
        .or_else(|| row.table.find(&["section"]))
        .ok_or(MALFORMED)?;
    let offset = row
        .table
        .find_side(&["offset"], Some(Side::Out))
        .or_else(|| row.table.find(&["offset"]))
        .ok_or(MALFORMED)?;
    let output = output_column(row, offset)?;
    let input_size = row.num_at(
        row.table
            .find_side(&["size"], Some(Side::In))
            .or_else(|| row.table.find(&["size"]))
            .ok_or(MALFORMED)?,
    )?;
    let offset_value = row.num_at(offset)?;
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
        region: None,
        output_section: row.cells[output].to_owned(),
        offset: offset_value,
    })
}

fn output_column(row: &Row<'_>, offset: usize) -> Parsed<usize> {
    row.table
        .find_side(&["section"], Some(Side::Out))
        .or_else(|| {
            row.table
                .columns
                .iter()
                .enumerate()
                .skip(offset + 1)
                .find(|(_, name)| name.name == "section" && name.side.is_none())
                .map(|(at, _)| at)
        })
        .ok_or(MALFORMED)
}

fn output_size(row: &Row<'_>, output: usize) -> Parsed<Option<u64>> {
    row.table
        .find_side(&["size"], Some(Side::Out))
        .or_else(|| {
            row.table
                .columns
                .iter()
                .enumerate()
                .skip(output + 1)
                .find(|(_, column)| column.name == "size" && column.side.is_none())
                .map(|(at, _)| at)
        })
        .map(|at| row.num_at(at))
        .transpose()
}
