#![forbid(unsafe_code)]

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::Duration;

use rusty_pdf::fake::FakeEngine;
use rusty_pdf::protocol::{MAX_REQUEST_FRAME, read_frame};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let watch = flag(&args, "--connect=").and_then(|address| TcpStream::connect(address).ok());
    let pause = flag(&args, "--pause-prepare=")
        .and_then(|text| text.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or_default();
    let gate = prepare_gate(&args);
    let mode = args
        .iter()
        .find(|arg| !arg.contains('='))
        .map(String::as_str);
    let _watch = watch.as_ref().and_then(|watch| watch.try_clone().ok());
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    match mode {
        Some("--garbage") => answer_with_garbage(&mut input, &mut output),
        Some("--silent") => {
            let _ = io::copy(&mut input, &mut io::sink());
            ExitCode::SUCCESS
        }
        Some("--dies-on-second-prepare") => {
            match rusty_pdf::serve(
                &mut FakeEngine::dying_on_second_prepare().watching_prepare(watch, gate),
                &mut input,
                &mut output,
            ) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::FAILURE,
            }
        }
        _ => match rusty_pdf::serve(
            &mut FakeEngine::pausing_prepare(pause).watching_prepare(watch, gate),
            &mut input,
            &mut output,
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().find_map(|arg| arg.strip_prefix(name))
}

fn prepare_gate(args: &[String]) -> Option<u32> {
    if let Some(gate) = flag(args, "--gate-prepare=").and_then(|text| text.parse().ok()) {
        return Some(gate);
    }
    flag(args, "--gate-first-prepare=")
        .filter(|marker| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(marker)
                .is_ok()
        })
        .map(|_| 1)
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
