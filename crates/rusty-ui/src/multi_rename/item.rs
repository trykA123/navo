use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use rusty_core::Location;
use rusty_core::rename::{ItemFields, LocalTime, Subject, split_name};
use rusty_fs::FsEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub parent: Location,
    pub name: String,
    pub excluded: bool,
    pub fields: ItemFields,
    parent_name: String,
    grand_name: String,
    time: Option<LocalTime>,
}

impl Item {
    #[must_use]
    pub fn from_entry(parent: &Location, entry: &FsEntry) -> Self {
        Self {
            parent: parent.clone(),
            name: entry.name.to_text(),
            excluded: false,
            fields: ItemFields::default(),
            parent_name: folder_name(Some(parent)),
            grand_name: folder_name(parent.parent().as_ref()),
            time: entry.modified_ms.and_then(local_time),
        }
    }

    pub(super) fn subject(&self) -> Subject<'_> {
        let (stem, ext) = split_name(&self.name);
        Subject {
            stem,
            ext,
            parent: &self.parent_name,
            grand: &self.grand_name,
            time: self.time,
            fields: None,
        }
    }
}

fn folder_name(location: Option<&Location>) -> String {
    location
        .and_then(Location::name)
        .map(|name| name.to_text())
        .unwrap_or_default()
}

fn local_time(modified_ms: i64) -> Option<LocalTime> {
    let moment: DateTime<Local> = Local.timestamp_millis_opt(modified_ms).single()?;
    Some(LocalTime {
        year: u16::try_from(moment.year()).ok()?,
        month: u8::try_from(moment.month()).ok()?,
        day: u8::try_from(moment.day()).ok()?,
        hour: u8::try_from(moment.hour()).ok()?,
        minute: u8::try_from(moment.minute()).ok()?,
        second: u8::try_from(moment.second()).ok()?,
    })
}
