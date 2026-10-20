use crate::prelude::*;

mod query;
mod source;

#[cfg(test)]
mod tests;

pub use query::{Query, Scope, Term};
pub use source::{ArgKind, Cancel, Group, Hit, HitSink, Request, ResultSource, Scopes, Target};

gpui_kit::actions!(command_center, [Toggle]);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "command_center");
}
