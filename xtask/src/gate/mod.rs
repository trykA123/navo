mod run;
mod steps;

pub use run::run;
pub use steps::steps;

pub struct GateOptions {
    pub with_coverage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: &'static str,
    pub program: &'static str,
    pub args: &'static [&'static str],
}

#[cfg(test)]
mod tests;
