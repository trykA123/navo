//! Windows ConPTY checks; they only run on Windows (windows-quick).

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::Recorder;
use crate::shells::{Probe, detect};
use crate::{Session, ShellKind, ShellSpec, Size};

const SIZE: Size = Size {
    cols: 120,
    rows: 30,
};
/// Each check ends in about 2 s when correct; well below the recorder's 20 s.
const LIMIT: Duration = Duration::from_secs(8);

/// `cmd /D <args>`: `/D` skips the AutoRun commands a machine may have registered.
fn cmd(args: &[&str]) -> ShellSpec {
    let mut cmd = detect(&Probe::system(Vec::new()))
        .into_iter()
        .find(|shell| shell.kind == ShellKind::Cmd)
        .unwrap();
    cmd.args = std::iter::once("/D")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    cmd
}

fn spawn(args: &[&str], sink: &Arc<Recorder>) -> Session {
    Session::spawn(&cmd(args), Path::new("C:\\"), SIZE, sink.clone()).unwrap()
}

#[test]
fn cmd_echo_prints_through_conpty_and_exits() {
    let sink = Arc::new(Recorder::default());
    let _session = spawn(&["/c", "echo", "hi"], &sink);
    let seen = sink.wait_within(LIMIT, |seen| seen.exit.is_some());
    assert!(
        seen.printed("hi"),
        "output: {:?}",
        String::from_utf8_lossy(&seen.output)
    );
    assert_eq!(seen.exit, Some(Some(0)));
}

/// R1 probe: logs whether ConPTY passes a complete OSC 133 mark through. When it does not,
/// the mark must be stripped whole: no fragment may reach the grid as text.
#[test]
fn osc133_passthrough_is_logged() {
    let sink = Arc::new(Recorder::default());
    let mark = "\x1b]133;A;navo=test";
    let complete = |seen: &super::Recorded| {
        seen.printed(&format!("{mark}\x1b\\")) || seen.printed(&format!("{mark}\x07"))
    };
    let _session = spawn(&["/c", "echo", &format!("{mark}\x1b\\")], &sink);
    let seen = sink.wait_within(LIMIT, |seen| complete(seen) || seen.exit.is_some());
    let passthrough = complete(&seen);
    if !passthrough {
        assert!(
            !seen.printed("133;A"),
            "ConPTY passed a partial OSC 133 mark: {:?}",
            String::from_utf8_lossy(&seen.output)
        );
    }
    let mut stderr = std::io::stderr();
    writeln!(stderr, "osc133 passthrough={passthrough}").unwrap();
}

#[test]
fn dropping_a_live_conpty_session_ends_cmd_within_the_limit() {
    let sink = Arc::new(Recorder::default());
    let session = spawn(&["/K", "echo", "ready"], &sink);
    sink.wait_for(|seen| seen.printed("ready"));
    let start = Instant::now();
    drop(session);
    assert!(start.elapsed() < Duration::from_millis(500), "drop blocked");
    let seen = sink.wait_within(Duration::from_secs(5), |seen| seen.exit.is_some());
    assert_eq!(seen.exits, 1);
}
