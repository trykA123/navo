mod tab;
mod workspace;

use super::lexer::{Lexer, Value};
use super::{Command, CommandId, CursorMove, ParseError, ParseErrorKind};
use crate::{Direction, Location, SpanMode};

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
        CommandId::SelectionExtend => &["to", "mode"],
        CommandId::SelectionExtension => &["mark"],
        CommandId::WindowClose | CommandId::WindowFocus => &["window"],
        CommandId::SelectionAddPattern | CommandId::SelectionRemovePattern => {
            &["pattern", "folders"]
        }
        CommandId::LayoutSplit
        | CommandId::LayoutFocus
        | CommandId::LayoutSwapToward
        | CommandId::LayoutResize => &["direction"],
        CommandId::LayoutPreset => &["tiles"],
        CommandId::LayoutSwap => &["a", "b"],
        CommandId::LayoutRatio => &["split", "ratio"],
        CommandId::LayoutFocusPane => &["n"],
        CommandId::WorkspaceSwitch | CommandId::WorkspaceDelete | CommandId::WorkspaceDiscard => {
            &["n"]
        }
        CommandId::WorkspaceNew => &["name"],
        CommandId::WorkspaceRename => &["n", "name"],
        CommandId::WorkspaceMove => &["n", "by"],
        _ => tab::field_names(command),
    }
}

fn build(mut fields: Fields<'_>) -> Result<Command, ParseError> {
    use CommandId as Id;
    Ok(match fields.command {
        Id::PaneOpenTab
        | Id::PaneCloseTab
        | Id::PaneNavigate
        | Id::PaneGoDrive
        | Id::PaneBack
        | Id::PaneForward
        | Id::PaneRefresh
        | Id::PaneCursorMove
        | Id::PaneActivateTab
        | Id::PaneScroll => pane_command(&mut fields)?,
        Id::SelectionToggle => Command::SelectionToggle,
        Id::SelectionToggleDown => Command::SelectionToggleDown,
        Id::SelectionRange => Command::SelectionRange {
            to: fields.required("to", |value| unsigned::<u64>(&value))?,
        },
        Id::SelectionExtend | Id::SelectionExtension | Id::SelectionRestore => {
            selection_command(&mut fields)?
        }
        Id::SelectionAddPattern | Id::SelectionRemovePattern => pattern_command(&mut fields)?,
        Id::SelectionInvert => Command::SelectionInvert,
        Id::SelectionAll => Command::SelectionAll,
        Id::SelectionClear => Command::SelectionClear,
        Id::LayoutSplit => Command::LayoutSplit {
            direction: fields.required("direction", |value| direction(&value))?,
        },
        Id::LayoutPreset
        | Id::LayoutSwap
        | Id::LayoutSwapToward
        | Id::LayoutResize
        | Id::LayoutRatio
        | Id::LayoutEven
        | Id::LayoutFocusPane => layout_command(&mut fields)?,
        Id::LayoutClose => Command::LayoutClose,
        Id::LayoutFocus => Command::LayoutFocus {
            direction: fields.required("direction", |value| direction(&value))?,
        },
        Id::WindowOpen | Id::WindowClose | Id::WindowFocus => window_command(&mut fields)?,
        Id::WorkspaceSwitch
        | Id::WorkspaceNew
        | Id::WorkspaceRename
        | Id::WorkspaceMove
        | Id::WorkspaceDelete
        | Id::WorkspaceRestore
        | Id::WorkspaceSave
        | Id::WorkspaceDiscard => workspace::build(&mut fields)?,
        Id::AppSaveSession => Command::AppSaveSession,
        _ => tab::build(&mut fields)?,
    })
}

fn layout_command(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    use CommandId as Id;
    Ok(match fields.command {
        Id::LayoutPreset => Command::LayoutPreset {
            tiles: fields.required("tiles", |v| {
                crate::Tiles::parse(&string(v)?).map_err(|e| Problem::Invalid(e.to_string()))
            })?,
        },
        Id::LayoutSwap => Command::LayoutSwap {
            a: fields.required("a", |value| pane_number(&value))?,
            b: fields.required("b", |value| pane_number(&value))?,
        },
        Id::LayoutSwapToward => Command::LayoutSwapToward {
            direction: fields.required("direction", |value| direction(&value))?,
        },
        Id::LayoutResize => Command::LayoutResize {
            direction: fields.required("direction", |value| direction(&value))?,
        },
        Id::LayoutRatio => Command::LayoutRatio {
            split: fields.required("split", |value| split_path(&value))?,
            ratio: fields.required("ratio", |v| bounded_number(&v, 100, 900).map(|n| n as u16))?,
        },
        Id::LayoutEven => Command::LayoutEven,
        _ => Command::LayoutFocusPane {
            n: fields.required("n", |value| pane_number(&value))?,
        },
    })
}

fn pane_command(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    Ok(match fields.command {
        CommandId::PaneOpenTab => Command::PaneOpenTab {
            location: fields.optional("location", location)?,
        },
        CommandId::PaneCloseTab => Command::PaneCloseTab {
            tab: fields.optional("tab", |value| unsigned::<u32>(&value))?,
        },
        CommandId::PaneNavigate => Command::PaneNavigate {
            location: fields.required("location", location)?,
        },
        CommandId::PaneGoDrive => Command::PaneGoDrive {
            root: fields.required("root", location)?,
        },
        CommandId::PaneBack => Command::PaneBack,
        CommandId::PaneForward => Command::PaneForward,
        CommandId::PaneRefresh => Command::PaneRefresh,
        CommandId::PaneCursorMove => Command::PaneCursorMove {
            to: fields.required("to", |value| cursor_move(&value))?,
        },
        CommandId::PaneActivateTab => Command::PaneActivateTab {
            tab: fields.required("tab", |value| unsigned::<u32>(&value))?,
        },
        _ => Command::PaneScroll {
            to: fields.required("to", |value| unsigned::<u64>(&value))?,
        },
    })
}

fn selection_command(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    Ok(match fields.command {
        CommandId::SelectionExtend => Command::SelectionExtend {
            to: fields.required("to", |value| cursor_move(&value))?,
            mode: fields.required("mode", |value| span_mode(&value))?,
        },
        CommandId::SelectionExtension => Command::SelectionExtension {
            mark: fields.required("mark", |value| flag(&value))?,
        },
        _ => Command::SelectionRestore,
    })
}

fn pattern_command(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    let pattern = fields.required("pattern", string)?;
    let folders = fields
        .optional("folders", |value| flag(&value))?
        .unwrap_or(false);
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
            window: fields.optional("window", |value| unsigned::<u64>(&value))?,
        },
        _ => Command::WindowFocus {
            window: fields.required("window", |value| unsigned::<u64>(&value))?,
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

fn flag(value: &Value<'_>) -> Result<bool, Problem> {
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

fn unsigned<T: std::str::FromStr + Bounded>(value: &Value<'_>) -> Result<T, Problem> {
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

fn direction(value: &Value<'_>) -> Result<Direction, Problem> {
    match value {
        Value::Word(word) => Direction::from_keyword(word)
            .ok_or_else(|| Problem::Invalid("expected left, right, up or down".to_owned())),
        _ => Err(Problem::WrongType("keyword")),
    }
}

fn span_mode(value: &Value<'_>) -> Result<SpanMode, Problem> {
    match value {
        Value::Word(word) => SpanMode::from_keyword(word)
            .ok_or_else(|| Problem::Invalid("expected mark, unmark or toggle".to_owned())),
        _ => Err(Problem::WrongType("keyword")),
    }
}

fn cursor_move(value: &Value<'_>) -> Result<CursorMove, Problem> {
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

fn bounded_number(value: &Value<'_>, min: usize, max: usize) -> Result<usize, Problem> {
    let Value::Int(text) = value else {
        return Err(Problem::WrongType("integer"));
    };
    text.parse()
        .ok()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| Problem::Invalid(format!("expected a whole number from {min} to {max}")))
}

fn pane_number(value: &Value<'_>) -> Result<usize, Problem> {
    bounded_number(value, 1, crate::MAX_PANES)
}

fn split_path(value: &Value<'_>) -> Result<crate::SplitPath, Problem> {
    let text = match value {
        Value::Str(text) => text.as_str(),
        Value::Int(text) | Value::Word(text) => text,
    };
    text.parse()
        .map_err(|reason: &str| Problem::Invalid(reason.to_owned()))
}
