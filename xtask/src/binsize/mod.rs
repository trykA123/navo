mod check;
mod run;

pub use check::check_size;
pub use run::run;

pub const RUSTY_EXE_LIMIT: u64 = 60 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct TooLarge {
    pub bytes: u64,
    pub limit: u64,
}

#[cfg(test)]
mod tests;
