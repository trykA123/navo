use std::fmt::Write as _;

use super::{CommandLog, LogEntry, LogError};
use crate::command::lexer::{Lexer, Value};
use crate::command::print::quote;
use crate::{Command, Entry, Event, Listing, Location, PaneId, TabId};

pub(super) fn write(log: &CommandLog) -> String {
    let mut out = format!(
        "ferrite-log {}\nhome {}\n",
        CommandLog::VERSION,
        quote(log.home().as_str())
    );
    if log.dropped() > 0 {
        let _ = writeln!(out, "dropped {}", log.dropped());
    }
    for record in log.records() {
        let _ = write!(out, "+{} ", record.after_ms);
        match &record.entry {
            LogEntry::Command(command) => {
                let _ = writeln!(out, "{command}");
            }
            LogEntry::Event(event) => write_event(&mut out, event),
        }
    }
    out
}

fn write_event(out: &mut String, event: &Event) {
    match event {
        Event::ListingLoaded {
            tab,
            location,
            listing,
        } => {
            let _ = writeln!(
                out,
                "!listing-loaded {{ tab: {}, location: {}, entries: {} }}",
                tab.get(),
                quote(location.as_str()),
                listing.len()
            );
            for entry in listing.entries() {
                let kind = if entry.is_folder { "folder" } else { "file" };
                let _ = writeln!(out, "  {kind} {}", quote(&entry.name));
            }
        }
        Event::PageRowsChanged { pane, rows } => {
            let _ = writeln!(out, "!page-rows {{ pane: {}, rows: {rows} }}", pane.get());
        }
    }
}

pub(super) fn read(text: &str) -> Result<CommandLog, LogError> {
    let lines: Vec<&str> = text.lines().collect();
    check_header(lines.first().copied().unwrap_or_default())?;
    let home = lines
        .get(1)
        .ok_or("the home line is missing".to_owned())
        .and_then(|line| home(line))
        .map_err(at(2))?;
    let mut log = CommandLog::new(home);
    let mut next = 2;
    if let Some(count) = lines.get(next).and_then(|l| l.strip_prefix("dropped ")) {
        let dropped = count
            .parse()
            .map_err(|_| at(3)(format!("bad dropped count {count:?}")))?;
        log.set_dropped(dropped);
        next += 1;
    }
    while let Some(line) = lines.get(next) {
        let number = next + 1;
        next += 1;
        let (after_ms, rest) = record(line).map_err(at(number))?;
        let entry = match rest.strip_prefix('!') {
            Some(event) => event_entry(event, &lines, &mut next).map_err(at(number))?,
            None => LogEntry::Command(Command::parse(rest).map_err(|e| at(number)(e.to_string()))?),
        };
        log.push(after_ms, entry);
    }
    Ok(log)
}

fn at(line: usize) -> impl Fn(String) -> LogError {
    move |message| LogError::Line { line, message }
}

fn check_header(line: &str) -> Result<(), LogError> {
    let version: u32 = line
        .strip_prefix("ferrite-log ")
        .and_then(|v| v.parse().ok())
        .ok_or(LogError::BadHeader)?;
    match version {
        v if v > CommandLog::VERSION => Err(LogError::TooNew {
            found: v,
            supported: CommandLog::VERSION,
        }),
        CommandLog::VERSION => Ok(()),
        _ => Err(LogError::BadHeader),
    }
}

fn home(line: &str) -> Result<Location, String> {
    let mut lexer = Lexer::new(line);
    keyword(&mut lexer, "home")?;
    let text = string_value(&mut lexer)?;
    end(&lexer)?;
    Location::new(text).map_err(|e| e.to_string())
}

fn record(line: &str) -> Result<(u64, &str), String> {
    let rest = line.strip_prefix('+').ok_or("a record starts with +")?;
    let (number, rest) = rest
        .split_once(' ')
        .ok_or("a record needs a space after its time")?;
    let after_ms = number.parse().map_err(|_| format!("bad time {number:?}"))?;
    Ok((after_ms, rest))
}

fn event_entry(text: &str, lines: &[&str], next: &mut usize) -> Result<LogEntry, String> {
    let mut lexer = Lexer::new(text);
    let name = lexer.word().map_err(|e| e.to_string())?;
    lexer.skip_ws();
    let fields = fields(&mut lexer)?;
    let event = match name {
        "listing-loaded" => {
            expect_fields(&fields, &["tab", "location", "entries"])?;
            let count = int(&fields, "entries")?;
            let entries = entry_lines(lines, next, count)?;
            Event::ListingLoaded {
                tab: TabId::new(int(&fields, "tab")?),
                location: Location::new(string(&fields, "location")?).map_err(|e| e.to_string())?,
                listing: Listing::new(entries),
            }
        }
        "page-rows" => {
            expect_fields(&fields, &["pane", "rows"])?;
            Event::PageRowsChanged {
                pane: PaneId::new(int(&fields, "pane")?),
                rows: int(&fields, "rows")?,
            }
        }
        other => return Err(format!("unknown event {other:?}")),
    };
    Ok(LogEntry::Event(event))
}

fn entry_lines(lines: &[&str], next: &mut usize, count: u64) -> Result<Vec<Entry>, String> {
    let missing = || format!("expected {count} entry lines");
    let count = usize::try_from(count).map_err(|_| missing())?;
    let end = next.checked_add(count).ok_or_else(missing)?;
    let slice = lines.get(*next..end).ok_or_else(missing)?;
    let entries = slice
        .iter()
        .map(|line| line.strip_prefix("  ").ok_or_else(missing).and_then(entry))
        .collect::<Result<Vec<_>, _>>()?;
    *next = end;
    Ok(entries)
}

fn entry(line: &str) -> Result<Entry, String> {
    let mut lexer = Lexer::new(line);
    let kind = lexer.word().map_err(|e| e.to_string())?;
    let is_folder = match kind {
        "folder" => true,
        "file" => false,
        other => return Err(format!("unknown entry kind {other:?}")),
    };
    let name = string_value(&mut lexer)?;
    end(&lexer)?;
    Ok(Entry { name, is_folder })
}

type Fields<'a> = Vec<(&'a str, Value<'a>)>;

fn fields<'a>(lexer: &mut Lexer<'a>) -> Result<Fields<'a>, String> {
    let fail = |e: crate::ParseError| e.to_string();
    lexer.eat('{').map_err(fail)?;
    let mut out = Vec::new();
    loop {
        lexer.skip_ws();
        if lexer.peek() == Some('}') {
            break;
        }
        let name = lexer.word().map_err(fail)?;
        lexer.skip_ws();
        lexer.eat(':').map_err(fail)?;
        lexer.skip_ws();
        out.push((name, lexer.value().map_err(fail)?));
        lexer.skip_ws();
        if lexer.peek() == Some(',') {
            lexer.eat(',').map_err(fail)?;
        } else if lexer.peek() != Some('}') {
            return Err(fail(lexer.unexpected()));
        }
    }
    lexer.eat('}').map_err(fail)?;
    end(lexer)?;
    Ok(out)
}

fn expect_fields(fields: &Fields<'_>, names: &[&str]) -> Result<(), String> {
    let mut given: Vec<&str> = fields.iter().map(|(n, _)| *n).collect();
    given.sort_unstable();
    let mut wanted = names.to_vec();
    wanted.sort_unstable();
    if given == wanted {
        Ok(())
    } else {
        Err(format!("expected the fields {names:?}"))
    }
}

fn field<'v, 'a>(fields: &'v Fields<'a>, name: &str) -> Result<&'v Value<'a>, String> {
    fields
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v)
        .ok_or_else(|| format!("missing field {name:?}"))
}

fn int(fields: &Fields<'_>, name: &str) -> Result<u64, String> {
    match field(fields, name)? {
        Value::Int(text) => text.parse().map_err(|_| format!("bad number for {name:?}")),
        _ => Err(format!("{name:?} needs a number")),
    }
}

fn string(fields: &Fields<'_>, name: &str) -> Result<String, String> {
    match field(fields, name)? {
        Value::Str(text) => Ok(text.clone()),
        _ => Err(format!("{name:?} needs a string")),
    }
}

fn keyword(lexer: &mut Lexer<'_>, word: &str) -> Result<(), String> {
    match lexer.word() {
        Ok(found) if found == word => {
            lexer.skip_ws();
            Ok(())
        }
        _ => Err(format!("expected {word:?}")),
    }
}

fn string_value(lexer: &mut Lexer<'_>) -> Result<String, String> {
    lexer.skip_ws();
    match lexer.value().map_err(|e| e.to_string())? {
        Value::Str(text) => Ok(text),
        _ => Err("expected a string".to_owned()),
    }
}

fn end(lexer: &Lexer<'_>) -> Result<(), String> {
    match lexer.peek() {
        None => Ok(()),
        Some(_) => Err(lexer.unexpected().to_string()),
    }
}
