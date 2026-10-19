mod fixture;
mod options;
mod report;
mod runner;
mod workspace;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match options::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("bench-copy: {error}");
            eprintln!(
                "usage: cargo xtask bench-copy <target-folder> [--runs N] [--sets small,medium,large]"
            );
            return ExitCode::from(2);
        }
    };
    match benchmark(&options) {
        Ok(markdown) => {
            print!("{markdown}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("bench-copy: {error}");
            ExitCode::FAILURE
        }
    }
}

fn benchmark(options: &options::Options) -> Result<String, String> {
    let mut workspace = workspace::Workspace::new(&options.target)?;
    let result = workspace
        .prepare()
        .and_then(|()| runner::measure(options, &workspace));
    let cleanup = workspace.clean_up();
    match (result, cleanup) {
        (Ok(markdown), Ok(())) => Ok(markdown),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!("{error}; cleanup failed: {cleanup}")),
    }
}
