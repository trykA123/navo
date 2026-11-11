//! The card detail runners the Stream's detail lane calls, one per heavy pair kind (D26.1, D26.4).

use std::sync::Arc;

use navo_compare::bytes::Align;
use navo_ui::compare_diff::{CardDetail, PairKind};

use super::bytes::ByteInput;
use super::load::LoadInput;
pub(crate) use super::pair::spool_path;
use super::pair::{EnginePlan, large_input};
use super::sources::Sources;
use super::stream::detail::DetailFn;
use super::stream::detail_state::DetailJob;

mod bytes;
mod large;

#[cfg(test)]
pub(crate) use bytes::{BytesJob, run as run_bytes, window_start};
#[cfg(test)]
pub(crate) use large::{large_card, run};

/// Build the detail function for one engine plan; large and binary pairs have a runner so far.
pub(crate) fn detail_fn(plan: EnginePlan) -> DetailFn {
    Arc::new(
        move |job: &DetailJob, input: &LoadInput, emit: &dyn Fn(CardDetail, bool)| match job.kind {
            PairKind::Large { .. } => {
                let input = large_input(&job.request, input, &plan);
                large::run(&job.request, input, &job.cancel, emit);
            }
            PairKind::Binary => {
                let (Some(left), Some(right)) = (input.left.clone(), input.right.clone()) else {
                    return;
                };
                let align = if left.1.size == right.1.size {
                    Align::Offset
                } else {
                    Align::Content
                };
                bytes::run(
                    bytes::BytesJob {
                        request: job.request.clone(),
                        input: ByteInput {
                            left,
                            right,
                            align,
                            run: 0,
                            budget: plan.budget,
                        },
                        sources: Sources::new(spool_path(&plan)),
                        settings: plan.settings.clone(),
                    },
                    &job.cancel,
                    emit,
                );
            }
            _ => {}
        },
    )
}
