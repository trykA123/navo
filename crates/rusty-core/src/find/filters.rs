#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeFilter {
    LessThan(u64),
    GreaterThan(u64),
    Range(u64, u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifiedFilter {
    YoungerThanDays(u32),
    OlderThanDays(u32),
    Before(Date),
    After(Date),
    Today,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribute {
    Hidden,
    Readonly,
    System,
    Archive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttributeFilter {
    pub attribute: Attribute,
    pub excluded: bool,
}

pub(super) fn parse_size(value: &str) -> Option<SizeFilter> {
    if let Some(value) = value.strip_prefix('<') {
        return parse_bytes(value).map(SizeFilter::LessThan);
    }
    if let Some(value) = value.strip_prefix('>') {
        return parse_bytes(value).map(SizeFilter::GreaterThan);
    }
    let (start, end) = value.split_once("..")?;
    let (start, end) = (parse_bytes(start)?, parse_bytes(end)?);
    (start <= end).then_some(SizeFilter::Range(start, end))
}

fn parse_bytes(value: &str) -> Option<u64> {
    let number_end = value
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(value.len());
    let number: u64 = value[..number_end].parse().ok()?;
    let multiplier = match value[number_end..].to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1024,
        "m" | "mb" => 1024 * 1024,
        "g" | "gb" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(multiplier)
}

pub(super) fn parse_modified(value: &str) -> Option<ModifiedFilter> {
    if value.eq_ignore_ascii_case("today") {
        return Some(ModifiedFilter::Today);
    }
    let comparison = value.chars().next()?;
    if !matches!(comparison, '<' | '>') {
        return None;
    }
    let value = &value[1..];
    if let Some(days) = value.strip_suffix('d') {
        let days = days.parse().ok()?;
        return Some(if comparison == '<' {
            ModifiedFilter::YoungerThanDays(days)
        } else {
            ModifiedFilter::OlderThanDays(days)
        });
    }
    let date = parse_date(value)?;
    Some(if comparison == '<' {
        ModifiedFilter::Before(date)
    } else {
        ModifiedFilter::After(date)
    })
}

fn parse_date(value: &str) -> Option<Date> {
    let mut parts = value.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if year.len() != 4 || month.len() != 2 || day.len() != 2 || parts.next().is_some() {
        return None;
    }
    if ![year, month, day]
        .iter()
        .all(|part| part.bytes().all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }
    let date = Date {
        year: year.parse().ok()?,
        month: month.parse().ok()?,
        day: day.parse().ok()?,
    };
    let leap = date.year.is_multiple_of(4)
        && (!date.year.is_multiple_of(100) || date.year.is_multiple_of(400));
    let max_day = match date.month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    (date.year > 0 && date.day > 0 && date.day <= max_day).then_some(date)
}

pub(super) fn parse_attrs(value: &str) -> Option<Vec<AttributeFilter>> {
    value.split(',').map(parse_attribute).collect()
}

fn parse_attribute(value: &str) -> Option<AttributeFilter> {
    let excluded = value.starts_with('-');
    let name = value.strip_prefix('-').unwrap_or(value);
    let attribute = match name.to_ascii_lowercase().as_str() {
        "hidden" => Attribute::Hidden,
        "readonly" => Attribute::Readonly,
        "system" => Attribute::System,
        "archive" => Attribute::Archive,
        _ => return None,
    };
    Some(AttributeFilter {
        attribute,
        excluded,
    })
}
