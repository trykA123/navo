mod clock;
mod marks;
mod memory;
mod report;
mod run;

pub(crate) use clock::ProcessClock;
pub(crate) use marks::mark;
pub(crate) use memory::Memory;
pub(crate) use run::Run;

#[cfg(test)]
mod tests;

#[cfg(any(unix, test))]
mod unix;
#[cfg(any(windows, test))]
mod windows;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(test)]
pub(crate) use marks::all;
pub(crate) use platform::process_age;
