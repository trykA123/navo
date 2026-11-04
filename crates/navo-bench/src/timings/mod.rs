//! Folder timings from outside (D99): the headless `navo bench --folder` numbers, first usable
//! with the folder, and key-to-frame of plain letters posted to Navo's window, on the owner's
//! folder and on a generated 100,000-file folder.
//!
//! Every number is typed in `privacy::Report`, or `not measured(reason)`. The owner's folder
//! reaches Navo as a read-only argument; every write goes through the guard. Navo's files are
//! parsed for numbers only, and the strings in them are dropped unread.

mod bench_json;
mod error;
mod headless;
mod host;
mod keys;
mod layout;
mod real;
mod result;
mod run;
mod trace;
mod window;

pub use bench_json::{BenchNumbers, parse_bench};
pub use error::TimingError;
pub use host::{Capture, Host, Launch, Subject, Window};
pub use real::RealHost;
pub use result::{
    GeneratedReport, HeadlessReport, KeysReport, SortKey, SortStat, TimingsReport, UsableReport,
};
pub use run::{GENERATED_FILES, Plan, measure_folder_timings};
pub use trace::{TraceNumbers, parse_trace};

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod real_tests;
#[cfg(test)]
mod tests;
