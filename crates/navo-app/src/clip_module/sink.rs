use std::path::PathBuf;

use navo_clipboard::{ClipSink, Sequence, SinkError};
use navo_shell::{ClipboardError, Effect, FileClipboard};

/// Writes rows back through the `navo-shell` clipboard calls. Windows only; elsewhere every
/// call reports `Unsupported`.
pub(super) struct ShellSink;

fn fault(error: ClipboardError) -> SinkError {
    match error {
        ClipboardError::Unsupported => SinkError::Unsupported,
        _ => SinkError::Failed,
    }
}

impl ClipSink for ShellSink {
    fn write_text(&self, text: &str) -> Result<(), SinkError> {
        navo_shell::write_text(text).map_err(fault)
    }

    fn write_files(&self, paths: &[PathBuf], cut: bool) -> Result<(), SinkError> {
        let effect = if cut { Effect::Move } else { Effect::Copy };
        let files = FileClipboard {
            paths: paths.to_vec(),
            effect,
        };
        navo_shell::write_files(&files).map_err(fault)
    }

    fn write_private(&self, text: &str) -> Result<Sequence, SinkError> {
        navo_shell::write_private_text(text)
            .map(Sequence)
            .map_err(fault)
    }

    fn clear_if_sequence(&self, sequence: Sequence) -> Result<bool, SinkError> {
        navo_shell::clear_if_sequence(sequence.0).map_err(fault)
    }
}
