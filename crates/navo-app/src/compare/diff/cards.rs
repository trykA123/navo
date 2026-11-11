//! The card detail runners the Stream's detail lane calls, one per heavy pair kind (D26.1, D26.4).

use std::sync::Arc;

use navo_ui::compare_diff::{CardDetail, PairKind};

use super::load::LoadInput;
use super::pair::{EnginePlan, large_input};

#[allow(
    unused_imports,
    reason = "the later runners (A5, A6b) read the spool path"
)]
pub(crate) use super::pair::spool_path;
use super::stream::detail::DetailFn;
use super::stream::detail_state::DetailJob;

mod large;

#[cfg(test)]
pub(crate) use large::{large_card, run};

/// Build the detail function for one engine plan; only large pairs have a runner so far.
#[allow(dead_code, reason = "the hooks wiring (A4b) calls it")]
pub(crate) fn detail_fn(plan: EnginePlan) -> DetailFn {
    Arc::new(
        move |job: &DetailJob, input: &LoadInput, emit: &dyn Fn(CardDetail, bool)| {
            if let PairKind::Large { .. } = job.kind {
                let input = large_input(&job.request, input, &plan);
                large::run(&job.request, input, &job.cancel, emit);
            }
        },
    )
}
