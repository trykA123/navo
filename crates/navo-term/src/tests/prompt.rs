//! Contract tests for NV12c: trusted OSC 133 nonce prompt state, one serialized writer,
//! manual sync queueing/auto-retry, safe shell quoting, split-chunk parsing and fault injection.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, Mutex, PoisonError};
use std::thread;

use super::Recorder;
use crate::grid::Grid;
use crate::prompt::{
    MarkKind, Nonce, PromptSink, PromptState, PtyWriter, ShellHook, SyncWait, quote_sync_command,
};
use crate::{
    Backend, FakeBackend, FakeHandle, Session, ShellKind, ShellSpec, Sink, Size, TermError,
};

const TEST_NONCE: &str = "0123456789abcdef0123456789abcdef";
const WRONG_NONCE: &str = "fedcba9876543210fedcba9876543210";

fn wired(shell: ShellKind, nonce: Option<&str>) -> (Session, FakeHandle, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let sink_inner: Arc<dyn Sink> = recorder.clone();
    let (backend, handle) = FakeBackend::new(Arc::clone(&sink_inner));
    let writer = Arc::new(PtyWriter::new(
        shell,
        nonce.map(Nonce::from_hex),
        Box::new(backend),
    ));
    let prompt_sink = Arc::new(PromptSink::for_writer(&writer, sink_inner));
    let handle = handle.with_sink(prompt_sink);
    (Session::with_writer(writer), handle, recorder)
}

fn osc_a_st(nonce: &str) -> Vec<u8> {
    format!("\x1b]133;A;navo={nonce}\x1b\\").into_bytes()
}

fn osc_a_bel(nonce: &str) -> Vec<u8> {
    format!("\x1b]133;A;navo={nonce}\x07").into_bytes()
}

/// Recorded write chunks and an optional one-shot write failure shared with the test.
type Chunks = Arc<Mutex<Vec<Vec<u8>>>>;
type FailNext = Arc<Mutex<Option<fn() -> TermError>>>;

/// A fake backend that records each `write` call as a separate chunk and yields inside `write`
/// to expose any missing serialization between concurrent callers.
struct ChunkedBackend {
    chunks: Chunks,
    fail_next: FailNext,
}

impl ChunkedBackend {
    fn new() -> (Self, Chunks, FailNext) {
        let chunks = Arc::new(Mutex::new(Vec::new()));
        let fail_next = Arc::new(Mutex::new(None));
        (
            Self {
                chunks: Arc::clone(&chunks),
                fail_next: Arc::clone(&fail_next),
            },
            chunks,
            fail_next,
        )
    }
}

impl Backend for ChunkedBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        if let Some(err_fn) = self
            .fail_next
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            return Err(err_fn());
        }
        // Yield mid-write to maximize interleaving if callers were not serialized.
        let mut piece = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            piece.push(byte);
            thread::yield_now();
        }
        self.chunks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(piece);
        Ok(())
    }

    fn resize(&mut self, size: Size) -> Result<(), TermError> {
        size.check()
    }
}

fn shell_spec(kind: ShellKind, program: &str, args: &[&str]) -> ShellSpec {
    ShellSpec {
        kind,
        program: PathBuf::from(program),
        args: args.iter().map(|arg| String::from(*arg)).collect(),
        label: String::from("test"),
    }
}

#[test]
fn prompt_unknown_blocks_sync_and_keeps_nothing() {
    let (session, handle, _) = wired(ShellKind::Pwsh, Some(TEST_NONCE));
    assert_eq!(session.prompt(), PromptState::Unknown);
    assert_eq!(
        session.sync_reason(Path::new(r"C:\Projects")),
        Some(SyncWait::NeedsIntegration)
    );
    let wait = session.sync_cwd(Path::new(r"C:\Projects")).unwrap();
    assert_eq!(wait, Some(SyncWait::NeedsIntegration));
    assert!(handle.written().is_empty());

    // Even if a trusted 133;A arrives later, nothing was queued while Unknown.
    handle.emit(&osc_a_st(TEST_NONCE));
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: false });
    assert!(handle.written().is_empty());
}

#[test]
fn prompt_ignores_untrusted_133_a_without_nonce_or_with_wrong_nonce() {
    let (session, handle, _) = wired(ShellKind::Pwsh, Some(TEST_NONCE));

    // Plain 133;A without nonce stays Unknown.
    handle.emit(b"\x1b]133;A\x1b\\");
    assert_eq!(session.prompt(), PromptState::Unknown);
    assert!(session.marks().is_empty());

    // Wrong nonce stays Unknown (both ST and BEL).
    handle.emit(&osc_a_st(WRONG_NONCE));
    handle.emit(&osc_a_bel(WRONG_NONCE));
    assert_eq!(session.prompt(), PromptState::Unknown);

    // Duplicate navo= param with one bad and one good is rejected fail-closed.
    handle.emit(format!("\x1b]133;A;navo={WRONG_NONCE};navo={TEST_NONCE}\x1b\\").as_bytes());
    assert_eq!(session.prompt(), PromptState::Unknown);

    // Aborted OSC terminated by CSI or CAN instead of BEL/ST is rejected.
    handle.emit(format!("\x1b]133;A;navo={TEST_NONCE}\x1b[31m").as_bytes());
    handle.emit(format!("\x1b]133;A;navo={TEST_NONCE}\x18").as_bytes());
    assert_eq!(session.prompt(), PromptState::Unknown);

    // Unintegrated shell ignores even matching-format navo params.
    let (unintegrated, unint_handle, _) = wired(ShellKind::Bash, None);
    unint_handle.emit(&osc_a_st(TEST_NONCE));
    assert_eq!(unintegrated.prompt(), PromptState::Unknown);
}

#[test]
fn prompt_running_blocks_sync_and_flushes_only_newest_folder_on_next_trusted_prompt() {
    let (mut session, handle, _) = wired(ShellKind::Pwsh, Some(TEST_NONCE));
    handle.emit(&osc_a_st(TEST_NONCE));
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: false });

    // User types a command and presses Enter -> Running.
    session.write(b"cargo build\r").unwrap();
    assert!(matches!(
        session.prompt(),
        PromptState::Running { ref command, .. } if command == "cargo build"
    ));
    assert_eq!(
        session.sync_reason(Path::new(r"C:\first")),
        Some(SyncWait::Running(String::from("cargo build")))
    );

    // Queue two folders while Running; only the newest is kept, nothing written yet.
    assert_eq!(
        session.sync_cwd(Path::new(r"C:\first")).unwrap(),
        Some(SyncWait::Running(String::from("cargo build")))
    );
    assert_eq!(
        session.sync_cwd(Path::new(r"C:\second")).unwrap(),
        Some(SyncWait::Running(String::from("cargo build")))
    );
    assert_eq!(handle.written(), b"cargo build\r");

    // Untrusted 133;A and 133;D must NOT flush the pending folder.
    handle.emit(b"\x1b]133;A\x1b\\");
    handle.emit(&osc_a_st(WRONG_NONCE));
    handle.emit(b"\x1b]133;D;0\x1b\\");
    assert_eq!(handle.written(), b"cargo build\r");
    assert_eq!(session.last_exit_code(), Some(0));
    assert!(matches!(session.prompt(), PromptState::Running { .. }));

    // Trusted 133;A flushes ONLY the newest folder and transitions straight to Running.
    handle.emit(&osc_a_st(TEST_NONCE));
    let written = String::from_utf8(handle.written()).unwrap();
    assert_eq!(
        written,
        "cargo build\rSet-Location -LiteralPath 'C:\\second'\r"
    );
    assert!(matches!(
        session.prompt(),
        PromptState::Running { ref command, .. } if command == "Set-Location -LiteralPath 'C:\\second'"
    ));

    // Next trusted 133;A returns to idle with nothing more written.
    handle.emit(&osc_a_st(TEST_NONCE));
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: false });
    assert_eq!(
        String::from_utf8(handle.written()).unwrap(),
        "cargo build\rSet-Location -LiteralPath 'C:\\second'\r"
    );
}

#[test]
fn prompt_typed_char_blocks_sync_and_backspace_does_not_clear_typed() {
    let (mut session, handle, _) = wired(ShellKind::Pwsh, Some(TEST_NONCE));
    handle.emit(&osc_a_st(TEST_NONCE));
    assert_eq!(session.sync_reason(Path::new(r"C:\work")), None);

    // Typing one character blocks sync with SyncWait::Typed.
    session.write(b"x").unwrap();
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: true });
    assert_eq!(
        session.sync_reason(Path::new(r"C:\work")),
        Some(SyncWait::Typed)
    );

    // Backspace is fail-closed: typed stays true until the next trusted 133;A.
    session.write(b"\x7f").unwrap();
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: true });

    let wait = session.sync_cwd(Path::new(r"C:\work")).unwrap();
    assert_eq!(wait, Some(SyncWait::Typed));
    assert_eq!(handle.written(), b"x\x7f");

    // Next trusted 133;A flushes the pending folder.
    handle.emit(&osc_a_st(TEST_NONCE));
    assert_eq!(
        handle.written(),
        b"x\x7fSet-Location -LiteralPath 'C:\\work'\r"
    );
}

#[test]
fn prompt_concurrent_keystroke_and_sync_never_interleave_bytes() {
    for _ in 0..40 {
        let (backend, chunks, _) = ChunkedBackend::new();
        let writer = Arc::new(PtyWriter::new(
            ShellKind::Pwsh,
            Some(Nonce::from_hex(TEST_NONCE)),
            Box::new(backend),
        ));
        let sink = Arc::new(PromptSink::for_writer(
            &writer,
            Arc::new(Recorder::default()),
        ));
        sink.output(&osc_a_st(TEST_NONCE));

        let barrier = Arc::new(Barrier::new(2));
        let w_user = Arc::clone(&writer);
        let b_user = Arc::clone(&barrier);
        let user_thread = thread::spawn(move || {
            b_user.wait();
            w_user.write_user(b"echo hello").unwrap();
        });

        let w_sync = Arc::clone(&writer);
        let b_sync = Arc::clone(&barrier);
        let sync_thread = thread::spawn(move || {
            b_sync.wait();
            w_sync.sync_cwd(Path::new(r"C:\Projects\navo")).unwrap()
        });

        user_thread.join().unwrap();
        let sync_result = sync_thread.join().unwrap();

        let recorded = chunks.lock().unwrap().clone();
        let expected_sync = b"Set-Location -LiteralPath 'C:\\Projects\\navo'\r".to_vec();
        let expected_user = b"echo hello".to_vec();
        match sync_result {
            None => {
                // Sync won the lock first: complete sync command first, then complete user input.
                assert_eq!(recorded, vec![expected_sync, expected_user]);
            }
            Some(SyncWait::Typed) => {
                // User keystroke won first: only user input was written; sync waited.
                assert_eq!(recorded, vec![expected_user]);
            }
            other => panic!("unexpected sync outcome: {other:?}"),
        }
    }
}

#[test]
fn prompt_powershell_quoting_doubles_apostrophes_and_rejects_controls_and_smart_quotes() {
    let quoted = quote_sync_command(ShellKind::Pwsh, Path::new(r"C:\Users\it's\dir")).unwrap();
    assert_eq!(
        quoted,
        "Set-Location -LiteralPath 'C:\\Users\\it''s\\dir'\r"
    );

    let win_ps =
        quote_sync_command(ShellKind::WindowsPowerShell, Path::new(r"C:\a [1]\$env:X")).unwrap();
    assert_eq!(win_ps, "Set-Location -LiteralPath 'C:\\a [1]\\$env:X'\r");

    for bad in [
        "C:\\a\nb",
        "C:\\a\rb",
        "C:\\a\x00b",
        "C:\\a\x1bb",
        "C:\\a\u{2019};calc",
        "",
    ] {
        assert_eq!(
            quote_sync_command(ShellKind::Pwsh, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
}

#[test]
fn prompt_cmd_quoting_refuses_metacharacters_and_controls() {
    let ok = quote_sync_command(ShellKind::Cmd, Path::new(r"C:\Users\it's\a b")).unwrap();
    assert_eq!(ok, "cd /d \"C:\\Users\\it's\\a b\"\r");

    for bad in [
        r"C:\100%",
        "C:\\has\"quote",
        r"C:\caret^here",
        r"C:\bang!here",
        "C:\\line\nbreak",
        "C:\\tab\there",
        "C:\\esc\x1bseq",
        "",
    ] {
        assert_eq!(
            quote_sync_command(ShellKind::Cmd, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }

    // Via Session: sync_reason and sync_cwd both report UnusualName and write nothing.
    let (session, handle, _) = wired(ShellKind::Cmd, Some(TEST_NONCE));
    handle.emit(&osc_a_st(TEST_NONCE));
    assert_eq!(
        session.sync_reason(Path::new(r"C:\100%")),
        Some(SyncWait::UnusualName)
    );
    assert_eq!(
        session.sync_cwd(Path::new(r"C:\100%")).unwrap(),
        Some(SyncWait::UnusualName)
    );
    assert!(handle.written().is_empty());
}

#[test]
fn prompt_osc133_split_across_reads_handles_st_and_bel() {
    let (mut session, handle, _) = wired(ShellKind::Pwsh, Some(TEST_NONCE));

    // Split ST-terminated 133;A across two chunks (including right between ESC and \).
    let st = format!("\x1b]133;A;navo={TEST_NONCE}\x1b\\");
    let (first, second) = st.as_bytes().split_at(st.len() - 1);
    handle.emit(first);
    assert_eq!(session.prompt(), PromptState::Unknown);
    handle.emit(second);
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: false });

    // Move to Running via 133;C split across two chunks with BEL terminator.
    let c_bel = b"\x1b]133;C;cmdline=cargo test\x07";
    handle.emit(&c_bel[..12]);
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: false });
    handle.emit(&c_bel[12..]);
    assert!(matches!(
        session.prompt(),
        PromptState::Running { ref command, .. } if command == "cargo test"
    ));

    // 133;D split byte-by-byte with exit code 101 stays Running and records exit code.
    for byte in b"\x1b]133;D;101\x1b\\" {
        handle.emit(&[*byte]);
    }
    assert_eq!(session.last_exit_code(), Some(101));
    assert!(matches!(session.prompt(), PromptState::Running { .. }));

    // BEL-terminated 133;A split across two chunks transitions back to AtPrompt.
    let bel = osc_a_bel(TEST_NONCE);
    let mid = bel.len() / 2;
    handle.emit(&bel[..mid]);
    assert!(matches!(session.prompt(), PromptState::Running { .. }));
    handle.emit(&bel[mid..]);
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: false });
    session.write(b"a").unwrap();
    assert_eq!(session.prompt(), PromptState::AtPrompt { typed: true });
}

#[test]
fn prompt_records_accurate_grid_lines_for_marks_in_single_chunk() {
    let grid = Arc::new(Grid::new(Size { cols: 80, rows: 24 }, 1_000, || {}));
    let grid_sink: Arc<dyn Sink> = grid.clone();
    let (backend, handle) = FakeBackend::new(Arc::clone(&grid_sink));
    let writer = Arc::new(PtyWriter::new(
        ShellKind::Pwsh,
        Some(Nonce::from_hex(TEST_NONCE)),
        Box::new(backend),
    ));
    let sink = Arc::new(PromptSink::for_writer(&writer, grid_sink));
    let handle = handle.with_sink(sink);
    let session = Session::with_writer(writer);

    // One read chunk contains multiple lines and A, C, D, A marks.
    let stream = format!(
        "\x1b]133;A;navo={TEST_NONCE}\x1b\\PS> ls\r\n\x1b]133;C;cmdline=ls\x1b\\file1\r\nfile2\r\n\x1b]133;D;0\x1b\\\x1b]133;A;navo={TEST_NONCE}\x1b\\PS> "
    );
    handle.emit(stream.as_bytes());

    let marks = session.marks();
    let summary: Vec<(MarkKind, Option<usize>)> = marks
        .into_iter()
        .map(|mark| (mark.kind, mark.line))
        .collect();
    assert_eq!(
        summary,
        vec![
            (MarkKind::PromptStart, Some(0)),
            (
                MarkKind::CommandExecuted {
                    command: Some(String::from("ls")),
                },
                Some(1),
            ),
            (MarkKind::CommandFinished { exit_code: Some(0) }, Some(3)),
            (MarkKind::PromptStart, Some(3)),
        ]
    );
}

#[test]
fn prompt_nonce_is_128_bit_hex_random_and_redacted_in_debug() {
    let first = Nonce::generate().unwrap();
    let second = Nonce::generate().unwrap();
    assert_eq!(first.as_str().len(), 32);
    assert!(
        first
            .as_str()
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_ne!(first, second);
    let debug = format!("{first:?}");
    assert!(!debug.contains(first.as_str()));
}

#[test]
fn prompt_shell_hooks_cover_pwsh_windows_powershell_and_cmd() {
    let nonce = Nonce::from_hex(TEST_NONCE);

    for kind in [ShellKind::Pwsh, ShellKind::WindowsPowerShell] {
        let spec = shell_spec(kind, "pwsh.exe", &["-NoLogo"]);
        let hook = ShellHook::for_shell(&spec, &nonce).expect("PowerShell hook");
        assert!(hook.env().is_empty());
        assert_eq!(hook.args().len(), 3);
        assert_eq!(hook.args()[0], "-NoExit");
        assert_eq!(hook.args()[1], "-Command");
        assert!(hook.args()[2].contains(&format!("133;A;navo={TEST_NONCE}")));
        assert!(!hook.args()[2].contains('"'));
        assert!(!hook.args()[2].contains('`'));

        // Explicit -Command in args skips injection.
        let one_shot = shell_spec(kind, "pwsh.exe", &["-Command", "echo hi"]);
        assert!(ShellHook::for_shell(&one_shot, &nonce).is_none());
    }

    let cmd = shell_spec(ShellKind::Cmd, "cmd.exe", &[]);
    let hook = ShellHook::for_shell(&cmd, &nonce).expect("cmd hook");
    assert!(hook.args().is_empty());
    assert_eq!(
        hook.env(),
        &[(
            String::from("PROMPT"),
            format!("$E]133;A;navo={TEST_NONCE}$E\\$P$G")
        )]
    );

    // NV12e integrates bash, zsh, fish, Git Bash and WSL (see tests::hooks); only
    // unrecognized shells stay unintegrated.
    let spec = shell_spec(ShellKind::Other, "sh", &[]);
    assert!(ShellHook::for_shell(&spec, &nonce).is_none());
}

#[test]
fn powershell_prompt_hook_captures_script_block_before_redefining_prompt() {
    let nonce = Nonce::from_hex(TEST_NONCE);

    for kind in [ShellKind::Pwsh, ShellKind::WindowsPowerShell] {
        let spec = shell_spec(kind, "powershell.exe", &["-NoLogo"]);
        let hook = ShellHook::for_shell(&spec, &nonce).expect("PowerShell hook");
        let script = &hook.args()[2];
        let (setup, body) = script
            .split_once("function global:prompt")
            .expect("script defines function global:prompt");

        assert!(
            setup.contains(".ScriptBlock"),
            "{kind:?} must capture immutable .ScriptBlock before redefining Function:prompt"
        );
        assert!(
            !body.contains("$global:__navo_p.ScriptBlock"),
            "{kind:?} prompt body must not dereference mutable FunctionInfo.ScriptBlock"
        );
        assert!(
            body.contains("& $global:__navo_p"),
            "{kind:?} prompt body must invoke captured ScriptBlock directly"
        );
        assert!(!script.contains('"'), "script must stay single-quote-only");
        assert!(!script.contains('`'), "script must stay backtick-free");
    }
}

#[test]
fn prompt_writer_fault_injection_fails_closed() {
    let (backend, _chunks, fail_next) = ChunkedBackend::new();
    let writer = Arc::new(PtyWriter::new(
        ShellKind::Pwsh,
        Some(Nonce::from_hex(TEST_NONCE)),
        Box::new(backend),
    ));
    let sink = Arc::new(PromptSink::for_writer(
        &writer,
        Arc::new(Recorder::default()),
    ));
    sink.output(&osc_a_st(TEST_NONCE));

    // InputFull sends zero bytes -> sync returns Err and leaves prompt clean.
    *fail_next.lock().unwrap() = Some(|| TermError::InputFull);
    assert!(matches!(
        writer.sync_cwd(Path::new(r"C:\work")),
        Err(TermError::InputFull)
    ));
    assert_eq!(
        writer.prompt_state(),
        PromptState::AtPrompt { typed: false }
    );

    // Io error may have sent partial bytes -> fail closed (no longer AtPrompt { typed: false }).
    *fail_next.lock().unwrap() = Some(|| TermError::Io(io::Error::other("broken pipe")));
    assert!(matches!(
        writer.sync_cwd(Path::new(r"C:\work")),
        Err(TermError::Io(_))
    ));
    assert_ne!(
        writer.prompt_state(),
        PromptState::AtPrompt { typed: false }
    );
}
