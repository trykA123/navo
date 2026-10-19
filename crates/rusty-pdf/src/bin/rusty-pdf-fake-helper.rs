#![forbid(unsafe_code)]

use std::io::{self, Read, Write};
use std::process::ExitCode;

use rusty_pdf::fake::FakeEngine;
use rusty_pdf::protocol::{MAX_REQUEST_FRAME, read_frame};

fn main() -> ExitCode {
    let mode = std::env::args().nth(1);
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    match mode.as_deref() {
        Some("--garbage") => answer_with_garbage(&mut input, &mut output),
        Some("--silent") => {
            let _ = io::copy(&mut input, &mut io::sink());
            ExitCode::SUCCESS
        }
        _ => match rusty_pdf::serve(&mut FakeEngine::default(), &mut input, &mut output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
    }
}

fn answer_with_garbage(input: &mut impl Read, output: &mut impl Write) -> ExitCode {
    while let Ok(Some(_)) = read_frame(input, MAX_REQUEST_FRAME) {
        if output
            .write_all(&[0xff; 8])
            .and_then(|()| output.flush())
            .is_err()
        {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
