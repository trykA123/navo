use crate::{
    AppState, Command, CommandLog, CursorMove, Direction, Entry, Event, Listing, Location,
    LogEntry, LogError, PaneId, Record, Recorder, TabId, apply, apply_event,
};

mod props;
mod recorder;
mod text;

fn loc(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn listing(entries: &[(&str, bool)]) -> Listing {
    Listing::new(
        entries
            .iter()
            .map(|(name, is_folder)| Entry {
                name: (*name).to_owned(),
                is_folder: *is_folder,
            })
            .collect(),
    )
}

fn loaded(tab: u64, location: &str, entries: &[(&str, bool)]) -> LogEntry {
    LogEntry::Event(Event::ListingLoaded {
        tab: TabId::new(tab),
        location: loc(location),
        listing: listing(entries),
    })
}

fn page_rows(pane: u64, rows: u64) -> LogEntry {
    LogEntry::Event(Event::PageRowsChanged {
        pane: PaneId::new(pane),
        rows,
    })
}

fn run(command: Command) -> LogEntry {
    LogEntry::Command(command)
}

fn after_ms(log: &CommandLog) -> Vec<u64> {
    log.records().iter().map(|r| r.after_ms).collect()
}
