mod row;

use super::asc::{Header, Shape};

const BAD_HEADER: &str = "the TRC header value is missing or invalid";
const BAD_VERSION: &str = "the TRC version must be 1.1, 1.3, 2.0 or 2.1";
const BAD_COLUMNS: &str = "the TRC columns are missing or invalid";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Version {
    #[default]
    Unknown,
    V11,
    V13,
    V20,
    V21,
}

impl Version {
    fn of(value: &str) -> Self {
        match value {
            "1.1" => Self::V11,
            "1.3" => Self::V13,
            "2.0" => Self::V20,
            "2.1" => Self::V21,
            _ => Self::Unknown,
        }
    }

    fn label(self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::V11 => Some("1.1"),
            Self::V13 => Some("1.3"),
            Self::V20 => Some("2.0"),
            Self::V21 => Some("2.1"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Columns {
    number: bool,
    bus: bool,
    reserved: bool,
    dlc: bool,
}

impl Columns {
    fn parse(value: &str) -> Option<Self> {
        let mut words = value.split(',').peekable();
        let number = words.next_if_eq(&"N").is_some();
        if words.next() != Some("O") || words.next() != Some("T") {
            return None;
        }
        let bus = words.next_if_eq(&"B").is_some();
        if words.next() != Some("I") || words.next() != Some("d") {
            return None;
        }
        let reserved = words.next_if_eq(&"R").is_some();
        let dlc = match words.next()? {
            "l" => false,
            "L" => true,
            _ => return None,
        };
        if words.next() != Some("D") || words.next().is_some() {
            return None;
        }
        Some(Self {
            number,
            bus,
            reserved,
            dlc,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct State {
    version: Version,
    columns: Option<Columns>,
}

pub(crate) fn is_header(line: &[u8]) -> bool {
    matches!(
        line.split(|byte| *byte == b'=').next(),
        Some(b";$FILEVERSION" | b";$STARTTIME" | b";$COLUMNS")
    )
}

pub(crate) fn has_unsupported_version(line: &[u8]) -> bool {
    line.strip_prefix(b";$FILEVERSION=")
        .and_then(|value| std::str::from_utf8(value).ok())
        .is_some_and(|value| Version::of(value.trim()) == Version::Unknown)
}

impl State {
    pub(crate) fn version_label(&self) -> Option<&'static str> {
        self.version.label()
    }

    pub(crate) fn parse(&mut self, line: &[u8], data: &mut Vec<u8>) -> (Option<u64>, Shape) {
        data.clear();
        if line.is_empty() {
            return (None, Shape::Blank);
        }
        if line.starts_with(b";") && !is_header(line) {
            return (None, Shape::Header(Header::Other));
        }
        let Ok(text) = std::str::from_utf8(line) else {
            return (None, Shape::Malformed("the TRC line is not valid UTF-8"));
        };
        if text.starts_with(';') {
            return (None, self.header(text));
        }
        row::parse(self, text, data)
    }

    fn header(&mut self, text: &str) -> Shape {
        let Some((key, value)) = text.split_once('=') else {
            return if is_header(text.as_bytes()) {
                Shape::Malformed(BAD_HEADER)
            } else {
                Shape::Header(Header::Other)
            };
        };
        let value = value.trim();
        match key {
            ";$FILEVERSION" => self.version(value),
            ";$STARTTIME" => match value.parse::<f64>() {
                Ok(days) if days.is_finite() && days >= 0.0 => {
                    Shape::Header(Header::Date(value.to_owned()))
                }
                _ => Shape::Malformed(BAD_HEADER),
            },
            ";$COLUMNS" => {
                self.columns = Columns::parse(value).filter(|columns| {
                    self.version != Version::V20 || !(columns.bus || columns.reserved)
                });
                if self.columns.is_some() {
                    Shape::Header(Header::Other)
                } else {
                    Shape::Malformed(BAD_COLUMNS)
                }
            }
            _ => Shape::Header(Header::Other),
        }
    }

    fn version(&mut self, value: &str) -> Shape {
        self.columns = None;
        self.version = Version::of(value);
        if self.version == Version::Unknown {
            Shape::Malformed(BAD_VERSION)
        } else {
            Shape::Header(Header::Other)
        }
    }
}
