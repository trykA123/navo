//! Test probe for the image worker: cancel a named pair right after its left decode and record
//! whether its right side was decoded (V37).

use std::sync::{Mutex, MutexGuard, PoisonError};

use navo_compare::model::CancelToken;

use super::super::load::Side;

/// Left file names whose pair is cancelled right after the left decode.
static ARMED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Left file names whose pair went on to decode the right side.
static RIGHT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Lock one list, ignoring poison from a failed test.
fn lock(list: &'static Mutex<Vec<String>>) -> MutexGuard<'static, Vec<String>> {
    list.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The text name of a side's entry.
fn name_of((_, entry): &Side) -> String {
    entry.name().to_text().to_string()
}

/// Arm the pair whose left file is `name` to cancel right after its left decode.
pub(crate) fn cancel_after_left_decode(name: &str) {
    lock(&ARMED).push(name.to_owned());
}

/// True when the pair whose left file is `name` decoded its right side.
pub(crate) fn right_decoded(name: &str) -> bool {
    lock(&RIGHT).iter().any(|seen| seen == name)
}

/// Called after the left decode: cancel when this pair is armed.
pub(super) fn left_decoded(left: &Side, cancel: &CancelToken) {
    let name = name_of(left);
    if lock(&ARMED).contains(&name) {
        cancel.cancel();
    }
}

/// Called just before the right decode of the pair with this left side.
pub(super) fn right_decoding(left: &Side) {
    lock(&RIGHT).push(name_of(left));
}
