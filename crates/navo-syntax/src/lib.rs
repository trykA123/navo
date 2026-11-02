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
pub use language::{Delimiter, KeywordGroup, Language};
pub use lexer::{Lexer, lex_line};
pub use registry::{LoadError, Registry};
pub use rules::from_toml;
pub use token::{LineState, Token, TokenKind};
pub use udl::from_udl;
