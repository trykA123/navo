use std::{
    path::Path,
    process::{Command, ExitCode, Stdio},
    time::Instant,
};

use super::{GateOptions, Step, steps};

pub fn run(args: &[String]) -> ExitCode {
    let with_coverage = match args {
        [] => false,
        [option] if option == "--coverage" => true,
        _ => {
            eprintln!("usage: cargo xtask gate [--coverage]");
            return ExitCode::from(2);
        }
    };
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("gate: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let steps = steps(&GateOptions { with_coverage });
    let started = Instant::now();
    for step in &steps {
        let code = run_step(root, step);
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    println!(
        "gate: all {} steps passed ({:.1} s)",
        steps.len(),
        started.elapsed().as_secs_f64()
    );
    ExitCode::SUCCESS
}

fn run_step(root: &Path, step: &Step) -> ExitCode {
    let started = Instant::now();
    let status = Command::new(step.program)
        .args(step.args)
        .current_dir(root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    let (code, result) = match status {
        Ok(status) if status.success() => (ExitCode::SUCCESS, String::from("ok")),
        Ok(status) => {
            let code = status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1);
            (ExitCode::from(code), format!("failed (exit {code})"))
        }
        Err(error) => (ExitCode::FAILURE, format!("failed: {error}")),
    };
    println!(
        "gate: {} {result} ({:.1} s)",
        step.name,
        started.elapsed().as_secs_f64()
    );
    code
}
