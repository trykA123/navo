#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RulesError {
    #[error("invalid rules TOML: {message}")]
    Toml { message: String },
    #[error("unknown key `{key}` in rules TOML")]
    UnknownKey { key: String },
    #[error("invalid UDL XML: {message}")]
    Xml { message: String },
    #[error("a language has no name")]
    MissingName,
    #[error("cannot read rules file: {message}")]
    Io { message: String },
}
