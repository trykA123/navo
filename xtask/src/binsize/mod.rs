pub const RUSTY_EXE_LIMIT: u64 = 30 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct TooLarge {
    pub bytes: u64,
    pub limit: u64,
}

pub fn check_size(bytes: u64, limit: u64) -> Result<u64, TooLarge> {
    let _ = (bytes, limit);
    todo!()
}

pub fn run(args: &[String]) -> std::process::ExitCode {
    let _ = args;
    todo!()
}

#[cfg(test)]
mod tests;
