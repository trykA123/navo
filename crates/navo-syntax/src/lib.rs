#![forbid(unsafe_code)]

mod builtin;
mod error;
mod language;
mod lexer;
mod registry;
mod rules;
mod token;
mod udl;

pub use builtin::builtin;
pub use error::RulesError;
pub use language::{Delimiter, KeywordGroup, Language, MAX_OPENERS, MAX_PATTERN_BYTES};
pub use lexer::{Lexer, lex_line};
pub use registry::{LoadError, Registry};
pub use rules::from_toml;
pub use token::{ChunkState, Inside, LineState, Token, TokenKind};
pub use udl::from_udl;
