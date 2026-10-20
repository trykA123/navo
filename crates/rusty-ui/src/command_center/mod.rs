mod query;
pub mod rank;
mod results;
mod selection;
mod source;

#[cfg(test)]
mod tests;

pub use query::{Query, Scope, Term};
pub use results::{Heading, Results, Row};
pub use selection::Selection;
pub use source::{ArgKind, Cancel, Group, Hit, HitSink, Request, ResultSource, Scopes, Target};
