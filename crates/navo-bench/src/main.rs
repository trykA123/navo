use navo_bench::privacy::{LeakScanner, SafeWriter, install_panic_hook};

fn main() -> std::process::ExitCode {
    let scanner = bootstrap_scanner();
    install_panic_hook(SafeWriter::new(std::io::stderr(), scanner.clone()));
    run(scanner)
}

fn bootstrap_scanner() -> LeakScanner {
    match LeakScanner::new(&[]) {
        Ok(scanner) => scanner,
        Err(_) => std::process::exit(2),
    }
}

#[cfg(windows)]
fn run(_scanner: LeakScanner) -> std::process::ExitCode {
    let mut platform = navo_bench::run::RealPlatform::default();
    let args = std::env::args_os().skip(1).collect();
    let exit = navo_bench::run::run(args, &mut platform, &mut std::io::stderr());
    std::process::ExitCode::from(exit.code())
}

#[cfg(not(windows))]
fn run(scanner: LeakScanner) -> std::process::ExitCode {
    use navo_bench::privacy::{Label, Output, Report};
    let report = Report::Message {
        label: Label::WindowsOnly.into(),
    };
    let mut writer = SafeWriter::new(std::io::stderr(), scanner);
    match writer.write(Output::Console, &report) {
        Ok(()) => std::process::ExitCode::from(2),
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
