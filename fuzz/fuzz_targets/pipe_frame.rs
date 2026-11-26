#![no_main]

use libfuzzer_sys::fuzz_target;
use navo_index::pipe::frames::{self, Query};

fuzz_target!(|data: &[u8]| {
    let _: Result<Query, frames::FrameError> = frames::decode(data);
});
