use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::process::ExitCode;

use navo_pdf::{Engine, PdfiumEngine, Unavailable};

pub(crate) const MISSING: &str = "PDF view needs pdfium";

pub(crate) fn run() -> ExitCode {
    let mut engine = load_engine();
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = BufWriter::new(io::stdout().lock());
    match serve(engine.as_mut(), &mut input, &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// Answers Navo's requests; on Windows Open takes only a handle Navo shared, never a path.
#[cfg(windows)]
fn serve(
    engine: &mut dyn Engine,
    input: &mut impl io::Read,
    output: &mut impl io::Write,
) -> Result<(), navo_pdf::protocol::CodecError> {
    navo_pdf::serve_with(engine, input, output, Some(adopt))
}

/// Answers Navo's requests; Open takes a path where no handle can be shared.
#[cfg(not(windows))]
fn serve(
    engine: &mut dyn Engine,
    input: &mut impl io::Read,
    output: &mut impl io::Write,
) -> Result<(), navo_pdf::protocol::CodecError> {
    navo_pdf::serve(engine, input, output)
}

/// Adopts the handle value of one Open request as a `File`; fails for a value that is not a
/// disk file (the value is then left open; Navo kills this helper on any reply but Opened).
#[cfg(windows)]
fn adopt(value: u64) -> io::Result<std::fs::File> {
    // SAFETY: `serve_with` calls this once per Open, with the value Navo duplicated into this
    // process for that request (`share_read_handle`); Navo never closes or resends it, so the
    // new File is its only owner. A reused value comes from a new duplication, so no set of
    // past values is kept. This is the only caller of `adopt_shared_file`.
    unsafe { navo_shell::adopt_shared_file(value) }
}

fn load_engine() -> Box<dyn Engine> {
    let loaded = beside_exe().and_then(|library| load(&library));
    match loaded {
        Ok(engine) => Box::new(engine),
        Err(reason) => Box::new(Unavailable::new(format!("{MISSING}: {reason}"))),
    }
}

fn beside_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let dir = exe
        .parent()
        .ok_or_else(|| "the program has no folder".to_owned())?;
    Ok(navo_pdf::library_beside(dir))
}

#[cfg(windows)]
fn load(library: &std::path::Path) -> Result<PdfiumEngine, String> {
    PdfiumEngine::load(library)
}

#[cfg(not(windows))]
fn load(library: &std::path::Path) -> Result<PdfiumEngine, String> {
    if library.is_file() {
        PdfiumEngine::load(library)
    } else {
        PdfiumEngine::load_system()
    }
}
