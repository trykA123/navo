use super::CommandId;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("at byte {offset}: {kind}")]
pub struct ParseError {
    pub offset: usize,
    pub kind: ParseErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseErrorKind {
    #[error("unexpected end of input")]
    UnexpectedEnd,
    #[error("unexpected character {0:?}")]
    UnexpectedChar(char),
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("`{command}` has no field `{field}`")]
    UnknownField { command: CommandId, field: String },
    #[error("field `{0}` is given more than once")]
    DuplicateField(String),
    #[error("`{command}` needs the field `{field}`")]
    MissingField {
        command: CommandId,
        field: &'static str,
    },
    #[error("field `{field}` expects {expected}")]
    WrongType {
        field: String,
        expected: &'static str,
    },
    #[error("invalid value for `{field}`: {reason}")]
    InvalidValue { field: String, reason: String },
    #[error("invalid escape sequence")]
    BadEscape,
    #[error("string is not closed")]
    UnterminatedString,
}
