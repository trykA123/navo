use super::lexer::{Lexer, Value};
use super::{Command, CommandId, CursorMove, ParseError, ParseErrorKind};
use crate::{Direction, Location};

struct Field<'a> {
    name: &'static str,
    value: Value<'a>,
    offset: usize,
}

struct Fields<'a> {
    command: CommandId,
    list: Vec<Field<'a>>,
    end: usize,
}

pub(super) fn parse(text: &str) -> Result<Command, ParseError> {
    let mut lexer = Lexer::new(text);
    lexer.skip_ws();
    let command = command_id(&mut lexer)?;
    lexer.skip_ws();
    let fields = match lexer.peek() {
        None => Fields {
            command,
            list: Vec::new(),
            end: lexer.pos(),
        },
        Some('{') => braced_fields(&mut lexer, command)?,
        Some(_) => return Err(lexer.unexpected()),
    };
    if lexer.peek().is_some() {
        return Err(lexer.unexpected());
    }
    build(fields)
}

fn command_id(lexer: &mut Lexer<'_>) -> Result<CommandId, ParseError> {
    let start = lexer.pos();
    let group = lexer.word()?;
    lexer.eat('.')?;
    let name = lexer.word()?;
    let id = format!("{group}.{name}");
    CommandId::from_name(&id).ok_or(ParseError {
        offset: start,
        kind: ParseErrorKind::UnknownCommand(id),
    })
}

fn braced_fields<'a>(lexer: &mut Lexer<'a>, command: CommandId) -> Result<Fields<'a>, ParseError> {
    lexer.eat('{')?;
    let mut list: Vec<Field<'a>> = Vec::new();
    loop {
        lexer.skip_ws();
        if lexer.peek() == Some('}') {
            break;
        }
        list.push(field(lexer, command, &list)?);
        lexer.skip_ws();
        match lexer.peek() {
            Some(',') => lexer.eat(',')?,
            Some('}') => break,
            _ => return Err(lexer.unexpected()),
        }
    }
    let end = lexer.pos();
    lexer.eat('}')?;
    lexer.skip_ws();
    Ok(Fields { command, list, end })
}

fn field<'a>(
    lexer: &mut Lexer<'a>,
    command: CommandId,
    seen: &[Field<'a>],
) -> Result<Field<'a>, ParseError> {
    let start = lexer.pos();
    let word = lexer.word()?;
    let at_name = |kind| ParseError {
        offset: start,
        kind,
    };
    let name = field_names(command)
        .iter()
        .find(|n| **n == word)
        .ok_or_else(|| {
            at_name(ParseErrorKind::UnknownField {
                command,
                field: word.to_owned(),
            })
        })?;
    if seen.iter().any(|f| f.name == *name) {
        return Err(at_name(ParseErrorKind::DuplicateField(word.to_owned())));
    }
    lexer.skip_ws();
    lexer.eat(':')?;
    lexer.skip_ws();
    let offset = lexer.pos();
    let value = lexer.value()?;
    Ok(Field {
        name,
        value,
        offset,
    })
}

pub(super) const fn field_names(command: CommandId) -> &'static [&'static str] {
    match command {
        CommandId::PaneOpenTab | CommandId::PaneNavigate | CommandId::WindowOpen => &["location"],
        CommandId::PaneGoDrive => &["root"],
        CommandId::PaneCloseTab | CommandId::PaneActivateTab => &["tab"],
        CommandId::PaneCursorMove | CommandId::SelectionRange | CommandId::PaneScroll => &["to"],
        CommandId::WindowClose | CommandId::WindowFocus => &["window"],
        CommandId::SelectionAddPattern | CommandId::SelectionRemovePattern => {
            &["pattern", "folders"]
        }
        CommandId::LayoutSplit | CommandId::LayoutFocus => &["direction"],
        _ => &[],
    }
}

fn build(mut fields: Fields<'_>) -> Result<Command, ParseError> {
    use CommandId as Id;
    Ok(match fields.command {
        Id::PaneOpenTab => Command::PaneOpenTab {
            location: fields.optional("location", location)?,
        },
        Id::PaneCloseTab => Command::PaneCloseTab {
            tab: fields.optional("tab", unsigned::<u32>)?,
        },
        Id::PaneNavigate => Command::PaneNavigate {
            location: fields.required("location", location)?,
        },
        Id::PaneGoDrive => Command::PaneGoDrive {
            root: fields.required("root", location)?,
        },
        Id::PaneBack => Command::PaneBack,
        Id::PaneForward => Command::PaneForward,
        Id::PaneRefresh => Command::PaneRefresh,
        Id::PaneCursorMove => Command::PaneCursorMove {
            to: fields.required("to", cursor_move)?,
        },
        Id::PaneActivateTab => Command::PaneActivateTab {
            tab: fields.required("tab", unsigned::<u32>)?,
        },
        Id::PaneScroll => Command::PaneScroll {
            to: fields.required("to", unsigned::<u64>)?,
        },
        Id::SelectionToggle => Command::SelectionToggle,
        Id::SelectionToggleDown => Command::SelectionToggleDown,
        Id::SelectionRange => Command::SelectionRange {
            to: fields.required("to", unsigned::<u64>)?,
        },
        Id::SelectionAddPattern | Id::SelectionRemovePattern => pattern_command(&mut fields)?,
        Id::SelectionInvert => Command::SelectionInvert,
        Id::SelectionAll => Command::SelectionAll,
        Id::SelectionClear => Command::SelectionClear,
        Id::LayoutSplit => Command::LayoutSplit {
            direction: fields.required("direction", direction)?,
        },
        Id::LayoutClose => Command::LayoutClose,
        Id::LayoutFocus => Command::LayoutFocus {
            direction: fields.required("direction", direction)?,
        },
        Id::WindowOpen | Id::WindowClose | Id::WindowFocus => window_command(&mut fields)?,
        Id::AppSaveSession => Command::AppSaveSession,
    })
}

fn pattern_command(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    let pattern = fields.required("pattern", string)?;
    let folders = fields.optional("folders", flag)?.unwrap_or(false);
    Ok(match fields.command {
        CommandId::SelectionAddPattern => Command::SelectionAddPattern { pattern, folders },
        _ => Command::SelectionRemovePattern { pattern, folders },
    })
}

fn window_command(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    Ok(match fields.command {
        CommandId::WindowOpen => Command::WindowOpen {
            location: fields.optional("location", location)?,
        },
        CommandId::WindowClose => Command::WindowClose {
            window: fields.optional("window", unsigned::<u64>)?,
        },
        _ => Command::WindowFocus {
            window: fields.required("window", unsigned::<u64>)?,
        },
    })
}

type Convert<T> = fn(Value<'_>) -> Result<T, Problem>;

enum Problem {
    WrongType(&'static str),
    Invalid(String),
}

impl Fields<'_> {
    fn optional<T>(
        &mut self,
        name: &'static str,
        convert: Convert<T>,
    ) -> Result<Option<T>, ParseError> {
        let Some(index) = self.list.iter().position(|f| f.name == name) else {
            return Ok(None);
        };
        let field = self.list.swap_remove(index);
        convert(field.value)
            .map(Some)
            .map_err(|problem| ParseError {
                offset: field.offset,
                kind: match problem {
                    Problem::WrongType(expected) => ParseErrorKind::WrongType {
                        field: name.to_owned(),
                        expected,
                    },
                    Problem::Invalid(reason) => ParseErrorKind::InvalidValue {
                        field: name.to_owned(),
                        reason,
                    },
                },
            })
    }

    fn required<T>(&mut self, name: &'static str, convert: Convert<T>) -> Result<T, ParseError> {
        self.optional(name, convert)?.ok_or(ParseError {
            offset: self.end,
            kind: ParseErrorKind::MissingField {
                command: self.command,
                field: name,
            },
        })
    }
}

fn string(value: Value<'_>) -> Result<String, Problem> {
    match value {
        Value::Str(text) => Ok(text),
        _ => Err(Problem::WrongType("string")),
    }
}

fn flag(value: Value<'_>) -> Result<bool, Problem> {
    match value {
        Value::Word("true") => Ok(true),
        Value::Word("false") => Ok(false),
        Value::Word(_) => Err(Problem::Invalid("expected true or false".to_owned())),
        _ => Err(Problem::WrongType("keyword")),
    }
}

fn location(value: Value<'_>) -> Result<Location, Problem> {
    Location::new(string(value)?).map_err(|e| Problem::Invalid(e.to_string()))
}

fn unsigned<T: std::str::FromStr + Bounded>(value: Value<'_>) -> Result<T, Problem> {
    match value {
        Value::Int(text) => text.parse().map_err(|_| {
            Problem::Invalid(format!("expected a whole number from 0 to {}", T::MAX_TEXT))
        }),
        _ => Err(Problem::WrongType("integer")),
    }
}

trait Bounded {
    const MAX_TEXT: &'static str;
}

impl Bounded for u32 {
    const MAX_TEXT: &'static str = "4294967295";
}

impl Bounded for u64 {
    const MAX_TEXT: &'static str = "18446744073709551615";
}

fn direction(value: Value<'_>) -> Result<Direction, Problem> {
    match value {
        Value::Word(word) => Direction::from_keyword(word)
            .ok_or_else(|| Problem::Invalid("expected left, right, up or down".to_owned())),
        _ => Err(Problem::WrongType("keyword")),
    }
}

fn cursor_move(value: Value<'_>) -> Result<CursorMove, Problem> {
    match value {
        Value::Word(word) => CursorMove::from_keyword(word).ok_or_else(|| {
            Problem::Invalid(
                "expected up, down, page-up, page-down, first, last or a row".to_owned(),
            )
        }),
        Value::Int(_) => unsigned::<u64>(value).map(CursorMove::Row),
        Value::Str(_) => Err(Problem::WrongType("keyword or integer")),
    }
}
