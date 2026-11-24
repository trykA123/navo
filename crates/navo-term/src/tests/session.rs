use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::Recorder;
use crate::{FakeBackend, Session, ShellKind, ShellSpec, Size, TermError};

const SIZE: Size = Size { cols: 80, rows: 24 };

fn shell(program: &str, args: &[&str]) -> ShellSpec {
    ShellSpec {
        kind: ShellKind::Other,
        program: PathBuf::from(program),
        args: args.iter().map(|arg| String::from(*arg)).collect(),
        label: String::from("test shell"),
    }
}

#[test]
fn writes_and_resizes_reach_the_backend_and_drop_ends_it() {
    let (backend, handle) = FakeBackend::new(Arc::new(Recorder::default()));
    let mut session = Session::with_backend(Box::new(backend));
    session.write(b"a\x03").unwrap();
    session
        .resize(Size {
            cols: 100,
            rows: 30,
        })
        .unwrap();
    drop(session);
    assert_eq!(handle.written(), b"a\x03");
    assert_eq!(
        handle.sizes(),
        [Size {
            cols: 100,
            rows: 30
        }]
    );
    assert!(handle.is_dropped());
}

#[test]
fn sizes_outside_one_to_i16_max_are_refused() {
    let (backend, handle) = FakeBackend::new(Arc::new(Recorder::default()));
    let mut session = Session::with_backend(Box::new(backend));
    let max = 32_767;
    assert!(session.resize(Size { cols: 1, rows: max }).is_ok());
    for size in [
        Size { cols: 0, rows: 24 },
        Size {
            cols: 80,
            rows: max + 1,
        },
    ] {
        assert!(matches!(
            session.resize(size),
            Err(TermError::InvalidSize { .. })
        ));
    }
    assert_eq!(handle.sizes().len(), 1);
}

#[test]
fn a_missing_shell_program_is_shell_missing() {
    let ghost = shell("/nonexistent/navo-term/ghost-shell", &[]);
    let result = Session::spawn(&ghost, Path::new("/"), SIZE, Arc::new(Recorder::default()));
    assert!(matches!(result, Err(TermError::ShellMissing(label)) if label == "test shell"));
}

#[cfg(unix)]
#[test]
fn a_real_pty_prints_output_then_reports_the_exit_code() {
    let sink = Arc::new(Recorder::default());
    let script = shell("/bin/sh", &["-c", "printf 'hi from pty'; exit 3"]);
    let _session = Session::spawn(&script, Path::new("/"), SIZE, sink.clone()).unwrap();
    let seen = sink.wait_for(|seen| seen.exit.is_some());
    assert!(seen.printed("hi from pty"), "output: {:?}", seen.output);
    assert_eq!(seen.exit, Some(Some(3)));
    assert_eq!(seen.exits, 1);
}

#[cfg(unix)]
#[test]
fn a_real_shell_runs_typed_input_and_dropping_the_session_ends_it() {
    let sink = Arc::new(Recorder::default());
    let mut session = Session::spawn(
        &shell("/bin/sh", &["-i"]),
        Path::new("/"),
        SIZE,
        sink.clone(),
    )
    .unwrap();
    session.write(b"echo $((40 + 2))x\r").unwrap();
    sink.wait_for(|seen| seen.printed("42x"));
    drop(session);
    let seen = sink.wait_for(|seen| seen.exit.is_some());
    assert_eq!(seen.exits, 1);
}
