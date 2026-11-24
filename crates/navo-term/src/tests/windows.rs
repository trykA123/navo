//! Windows ConPTY checks; they only run on Windows (windows-quick).

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use super::Recorder;
use crate::shells::{Probe, detect};
use crate::{Session, ShellKind, ShellSpec, Size};

const SIZE: Size = Size {
    cols: 120,
    rows: 30,
};

fn cmd(args: &[&str]) -> ShellSpec {
    let mut cmd = detect(&Probe::system(Vec::new()))
        .into_iter()
        .find(|shell| shell.kind == ShellKind::Cmd)
        .unwrap();
    cmd.args = args.iter().map(|arg| String::from(*arg)).collect();
    cmd
}

#[test]
fn cmd_echo_prints_through_conpty_and_exits() {
    let sink = Arc::new(Recorder::default());
    let _session = Session::spawn(
        &cmd(&["/c", "echo", "hi"]),
        Path::new("C:\\"),
        SIZE,
        sink.clone(),
    )
    .unwrap();
    let seen = sink.wait_for(|seen| seen.exit.is_some());
    assert!(
        seen.printed("hi"),
        "output: {:?}",
        String::from_utf8_lossy(&seen.output)
    );
    assert_eq!(seen.exit, Some(Some(0)));
}

/// R1 probe: logs whether ConPTY passes an OSC 133 mark through; never fails on the answer.
#[test]
fn osc133_passthrough_is_logged() {
    let sink = Arc::new(Recorder::default());
    let mark = "\x1b]133;A;navo=test\x1b\\";
    let _session = Session::spawn(
        &cmd(&["/c", "echo", mark]),
        Path::new("C:\\"),
        SIZE,
        sink.clone(),
    )
    .unwrap();
    let seen = sink.wait_for(|seen| seen.exit.is_some());
    let passthrough = seen.printed("\x1b]133;A;navo=test");
    let mut stderr = std::io::stderr();
    writeln!(stderr, "osc133 passthrough={passthrough}").unwrap();
}
