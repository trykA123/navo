mod case;
pub mod check;
mod cleanup;
mod mask;
mod order;
mod parse;
mod rules;
mod search;
mod split;

pub use case::{ExtCase, NameCase};
pub use cleanup::Cleanup;
pub use mask::{
    CaseMode, CounterParts, CounterSpec, FieldLookup, LocalTime, Mask, MaskError, MaskErrorKind,
    Span, Subject, Token,
};
pub use order::{BatchPlan, plan_batch};
pub use rules::{Compiled, Counter, Find, RenameRules, RuleError, RuleField};
pub use split::split_name;

#[cfg(test)]
mod tests;
