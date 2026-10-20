mod check;
mod run;
mod steps;

pub use check::run as check;
pub use run::run;
pub use steps::steps;

pub struct GateOptions {
    pub with_coverage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: &'static str,
    pub program: &'static str,
    pub args: Vec<String>,
}

impl Step {
    pub fn new(name: &'static str, program: &'static str, args: &[&str]) -> Self {
        Self {
            name,
            program,
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
        }
    }
}

#[cfg(test)]
mod tests;
