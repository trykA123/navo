use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::process::ExitCode;

use rusty_pdf::{Engine, PdfiumEngine, Unavailable};

pub(crate) const MISSING: &str = "PDF view needs pdfium";

pub(crate) fn run() -> ExitCode {
    let mut engine = load_engine();
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = BufWriter::new(io::stdout().lock());
    match rusty_pdf::serve(engine.as_mut(), &mut input, &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
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
    Ok(rusty_pdf::library_beside(dir))
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
