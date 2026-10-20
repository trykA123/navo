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

impl State {
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
        self.version = match value {
            "1.1" => Version::V11,
            "1.3" => Version::V13,
            "2.0" => Version::V20,
            "2.1" => Version::V21,
            _ => Version::Unknown,
        };
        if self.version == Version::Unknown {
            Shape::Malformed(BAD_VERSION)
        } else {
            Shape::Header(Header::Other)
        }
    }
}
