//! Contract tests for NV12e: bash, zsh, fish, Git Bash and WSL prompt hooks, owner-only
//! temp init files with RAII cleanup, and POSIX / Git Bash / WSL folder-sync quoting.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::Recorder;
#[cfg(unix)]
use crate::Size;
use crate::prompt::{
    Nonce, PromptSink, PromptState, PtyWriter, ShellHook, SyncWait, quote_sync_command,
};
use crate::shells::ShellSpec;
use crate::{FakeBackend, FakeHandle, Session, ShellKind, Sink};

const BAD_NAMES: &[&str] = &[
    "a\nb",
    "a\rb",
    "a\0b",
    "a\x1bb",
    "a\tb",
    "a\u{2028}b",
    "a\u{2029}b",
    "",
];

#[test]
fn posix_shells_quote_with_single_quotes_and_reject_controls() {
    for kind in [ShellKind::Bash, ShellKind::Zsh, ShellKind::Fish] {
        assert_eq!(
            quote_sync_command(kind, Path::new("it's here")),
            Ok(String::from("cd -- 'it'\\''s here'\r")),
            "apostrophe quoting for {kind:?}"
        );
        assert_eq!(
            quote_sync_command(kind, Path::new("/home/a b")),
            Ok(String::from("cd -- '/home/a b'\r")),
            "space quoting for {kind:?}"
        );
        for bad in BAD_NAMES {
            assert_eq!(
                quote_sync_command(kind, Path::new(bad)),
                Err(SyncWait::UnusualName),
                "expected UnusualName for {bad:?} in {kind:?}"
            );
        }
    }
}

#[test]
fn git_bash_converts_windows_paths_then_posix_quotes() {
    assert_eq!(
        quote_sync_command(ShellKind::GitBash, Path::new(r"C:\Users\it's here")),
        Ok(String::from("cd -- '/c/Users/it'\\''s here'\r"))
    );
    assert_eq!(
        quote_sync_command(ShellKind::GitBash, Path::new(r"C:\Users\a b")),
        Ok(String::from("cd -- '/c/Users/a b'\r"))
    );
    assert_eq!(
        quote_sync_command(ShellKind::GitBash, Path::new("c:/Users/a b")),
        Ok(String::from("cd -- '/c/Users/a b'\r"))
    );
    for bad in BAD_NAMES {
        assert_eq!(
            quote_sync_command(ShellKind::GitBash, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
    // UNC, extended-length and non-local paths are unsupported.
    for bad in [
        r"\\server\share",
        r"\\?\C:\Users",
        "relative/dir",
        "/unix/path",
    ] {
        assert_eq!(
            quote_sync_command(ShellKind::GitBash, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
}

#[test]
fn wsl_sync_uses_wslpath_with_posix_quoting() {
    assert_eq!(
        quote_sync_command(ShellKind::Wsl, Path::new(r"C:\Users\a b")),
        Ok(String::from("cd -- \"$(wslpath -u 'C:\\Users\\a b')\"\r"))
    );
    assert_eq!(
        quote_sync_command(ShellKind::Wsl, Path::new(r"C:\Users\it's here")),
        Ok(String::from(
            "cd -- \"$(wslpath -u 'C:\\Users\\it'\\''s here')\"\r"
        ))
    );
    for bad in BAD_NAMES {
        assert_eq!(
            quote_sync_command(ShellKind::Wsl, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
}
const TEST_NONCE: &str = "0123456789abcdef0123456789abcdef";
const WRONG_NONCE: &str = "fedcba9876543210fedcba9876543210";
#[cfg(unix)]
const SIZE: Size = Size { cols: 80, rows: 24 };

fn spec(kind: ShellKind, program: &str, args: &[&str]) -> ShellSpec {
    ShellSpec {
        kind,
        program: PathBuf::from(program),
        args: args.iter().map(|arg| String::from(*arg)).collect(),
        label: String::from("test"),
    }
}

fn hook_for(kind: ShellKind, args: &[&str]) -> ShellHook {
    let nonce = Nonce::from_hex(TEST_NONCE);
    ShellHook::for_shell(&spec(kind, "sh", args), &nonce).expect("hook")
}

fn init_path(hook: &ShellHook) -> PathBuf {
    hook.init_path().expect("init path").to_path_buf()
}

fn read_init(hook: &ShellHook) -> String {
    std::fs::read_to_string(init_path(hook)).expect("init file")
}

fn expected_bash_block() -> String {
    format!(
        "__navo_nonce='{TEST_NONCE}'\n\
         __navo_prompt() {{ printf '\\033]133;A;navo=%s\\033\\\\' \"$__navo_nonce\"; }}\n\
         if [ -n \"${{PROMPT_COMMAND:-}}\" ]; then\n\
           PROMPT_COMMAND=\"$PROMPT_COMMAND\"$'\\n'\"__navo_prompt\"\n\
         else\n\
           PROMPT_COMMAND=\"__navo_prompt\"\n\
         fi\n"
    )
}

fn expected_bash_script() -> String {
    format!(
        "# Navo terminal integration (temporary file; removed after the first prompt).\n\
         if [ -f \"$HOME/.bashrc\" ]; then . \"$HOME/.bashrc\"; fi\n\
         unset __NAVO_INIT __NAVO_ZDOTDIR\n\
         {block}",
        block = expected_bash_block()
    )
}

#[test]
fn bash_hook_command_line_and_init_text() {
    let hook = hook_for(ShellKind::Bash, &[]);
    let path = init_path(&hook);
    assert!(!hook.replaces_args());
    assert_eq!(
        hook.args(),
        [String::from("--init-file"), path.display().to_string()]
    );
    assert!(
        hook.env()
            .iter()
            .any(|(key, value)| key == "__NAVO_INIT" && value == &path.display().to_string())
    );
    assert_eq!(read_init(&hook), expected_bash_script());
}

#[test]
fn zsh_hook_uses_private_zdotdir_and_precmd() {
    let hook = hook_for(ShellKind::Zsh, &[]);
    let path = init_path(&hook);
    assert_eq!(path.file_name().unwrap(), ".zshrc");
    assert!(hook.args().is_empty());
    let dir = path.parent().unwrap().display().to_string();
    assert!(
        hook.env()
            .iter()
            .any(|(key, value)| key == "ZDOTDIR" && value == &dir)
    );
    let original = std::env::var("ZDOTDIR").unwrap_or_default();
    assert!(
        hook.env()
            .iter()
            .any(|(key, value)| key == "__NAVO_ZDOTDIR" && value == &original)
    );
    let expected = format!(
        "# Navo terminal integration (temporary ZDOTDIR; removed after the first prompt).\n\
         if [ -n \"${{__NAVO_ZDOTDIR:-}}\" ]; then\n\
           ZDOTDIR=\"$__NAVO_ZDOTDIR\"\n\
         else\n\
           unset ZDOTDIR\n\
         fi\n\
         unset __NAVO_INIT __NAVO_ZDOTDIR\n\
         if [ -f \"${{ZDOTDIR:-$HOME}}/.zshrc\" ]; then . \"${{ZDOTDIR:-$HOME}}/.zshrc\"; fi\n\
         __navo_nonce='{TEST_NONCE}'\n\
         __navo_prompt() {{ printf '\\033]133;A;navo=%s\\033\\\\' \"$__navo_nonce\"; }}\n\
         precmd_functions+=(__navo_prompt)\n"
    );
    assert_eq!(read_init(&hook), expected);

    // Early startup files are forwarded, not hidden by the temporary ZDOTDIR.
    let dir = path.parent().unwrap();
    for name in [".zshenv", ".zprofile"] {
        let forwarded = format!(
            "# Navo terminal integration (temporary ZDOTDIR; removed after the first prompt).\n\
             __navo_zdotdir=\"$ZDOTDIR\"\n\
             if [ -n \"${{__NAVO_ZDOTDIR:-}}\" ]; then\n\
               ZDOTDIR=\"$__NAVO_ZDOTDIR\"\n\
             else\n\
               unset ZDOTDIR\n\
             fi\n\
             if [ -f \"${{ZDOTDIR:-$HOME}}/{name}\" ]; then . \"${{ZDOTDIR:-$HOME}}/{name}\"; fi\n\
             ZDOTDIR=\"$__navo_zdotdir\"\n\
             unset __navo_zdotdir\n"
        );
        assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), forwarded);
    }
}

#[test]
fn fish_hook_sources_temp_file_and_wraps_fish_prompt() {
    let hook = hook_for(ShellKind::Fish, &[]);
    let path = init_path(&hook);
    assert_eq!(
        hook.args(),
        [
            String::from("--init-command"),
            format!("source '{}'", path.display())
        ]
    );
    let expected = format!(
        "# Navo terminal integration (temporary file; removed after the first prompt).\n\
         # fish reads the user's config.fish before --init-command runs, so it is already restored.\n\
         set -e __NAVO_INIT __NAVO_ZDOTDIR\n\
         set -g __navo_nonce '{TEST_NONCE}'\n\
         if functions -q fish_prompt\n\
             functions -c fish_prompt __navo_user_prompt\n\
         else\n\
             function __navo_user_prompt\n\
                 printf '%s> ' (prompt_pwd)\n\
             end\n\
         end\n\
         function fish_prompt\n\
             printf '\\033]133;A;navo=%s\\033\\\\\\\\' \"$__navo_nonce\"\n\
             __navo_user_prompt\n\
         end\n"
    );
    assert_eq!(read_init(&hook), expected);
}

#[test]
fn git_bash_hook_replaces_login_args_and_emulates_login_startup() {
    let hook = hook_for(ShellKind::GitBash, &["--login", "-i"]);
    let path = init_path(&hook);
    assert!(hook.replaces_args());
    assert_eq!(
        hook.args(),
        [
            String::from("--init-file"),
            path.display().to_string(),
            String::from("-i")
        ]
    );
    let expected = format!(
        "# Navo terminal integration for Git Bash (temporary; removed after the first prompt).\n\
         # Replaces --login: performs the login startup itself, then installs the prompt hook.\n\
         if [ -f /etc/profile ]; then . /etc/profile; fi\n\
         __navo_profile=''\n\
         for __navo_f in \"$HOME/.bash_profile\" \"$HOME/.bash_login\" \"$HOME/.profile\"; do\n\
           if [ -f \"$__navo_f\" ]; then . \"$__navo_f\"; __navo_profile=1; break; fi\n\
         done\n\
         if [ -z \"$__navo_profile\" ] && [ -f \"$HOME/.bashrc\" ]; then . \"$HOME/.bashrc\"; fi\n\
         unset __navo_f __navo_profile __NAVO_INIT __NAVO_ZDOTDIR\n\
         {block}",
        block = expected_bash_block()
    );
    assert_eq!(read_init(&hook), expected);
}

#[test]
fn wsl_hook_keeps_distro_args_and_resolves_init_with_wslpath() {
    let hook = hook_for(ShellKind::Wsl, &["-d", "Ubuntu"]);
    let path = init_path(&hook);
    assert!(!hook.replaces_args());
    assert_eq!(
        hook.args(),
        [
            String::from("--"),
            String::from("bash"),
            String::from("-c"),
            format!(
                "exec bash --init-file \"$(wslpath -u '{}')\"",
                path.display()
            )
        ]
    );
    assert_eq!(read_init(&hook), expected_bash_script());
}

#[test]
fn explicit_command_or_rcfile_args_skip_injection() {
    let nonce = Nonce::from_hex(TEST_NONCE);
    for (kind, args) in [
        (ShellKind::Bash, &["-c", "echo hi"][..]),
        (ShellKind::Bash, &["--rcfile", "other"][..]),
        (ShellKind::GitBash, &["-c", "echo hi"][..]),
        (ShellKind::Zsh, &["-c", "echo hi"][..]),
        (ShellKind::Fish, &["-c", "echo hi"][..]),
        (ShellKind::Wsl, &["--exec", "echo"][..]),
    ] {
        let shell = spec(kind, "sh", args);
        assert!(
            ShellHook::for_shell(&shell, &nonce).is_none(),
            "expected no hook for {kind:?} with {args:?}"
        );
    }
}

/// A fake session wired through `PromptSink`, plus the temp init attached to the writer.
fn wired_with_init(kind: ShellKind) -> (Arc<PtyWriter>, FakeHandle, PathBuf) {
    let hook = hook_for(kind, &[]);
    let path = init_path(&hook);
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn Sink> = recorder.clone();
    let (backend, handle) = FakeBackend::new(Arc::clone(&sink));
    let writer = Arc::new(PtyWriter::new(
        kind,
        Some(Nonce::from_hex(TEST_NONCE)),
        Box::new(backend),
    ));
    let prompt_sink = Arc::new(PromptSink::for_writer(&writer, sink));
    let handle = handle.with_sink(prompt_sink);
    writer.attach_init(hook.init().expect("init owner"));
    drop(hook);
    (writer, handle, path)
}

#[test]
fn temp_init_deleted_after_first_trusted_prompt_only() {
    let (writer, handle, path) = wired_with_init(ShellKind::Bash);
    let dir = path.parent().unwrap().to_path_buf();
    assert!(path.exists() && dir.exists());

    // Untrusted markers must not delete the init files.
    handle.emit(format!("\x1b]133;A;navo={WRONG_NONCE}\x1b\\").as_bytes());
    handle.emit(b"\x1b]133;A\x1b\\");
    assert!(path.exists() && dir.exists());
    assert_eq!(writer.prompt_state(), PromptState::Unknown);

    handle.emit(format!("\x1b]133;A;navo={TEST_NONCE}\x1b\\").as_bytes());
    assert_eq!(
        writer.prompt_state(),
        PromptState::AtPrompt { typed: false }
    );
    assert!(
        !path.exists(),
        "init file survived the first trusted prompt"
    );
    assert!(!dir.exists(), "init dir survived the first trusted prompt");
}

#[test]
fn temp_init_deleted_on_session_drop_and_on_hook_drop() {
    let (writer, _handle, path) = wired_with_init(ShellKind::Zsh);
    let dir = path.parent().unwrap().to_path_buf();
    assert!(path.exists());
    drop(writer);
    assert!(!path.exists(), "init file leaked on session drop");
    assert!(!dir.exists(), "init dir leaked on session drop");

    // RAII even without a session: dropping the hook alone cleans up.
    let hook = hook_for(ShellKind::Fish, &[]);
    let orphan = init_path(&hook);
    assert!(orphan.exists());
    drop(hook);
    assert!(!orphan.exists(), "init file leaked on hook drop");
}

#[cfg(unix)]
#[test]
fn temp_init_files_and_dirs_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let hook = hook_for(ShellKind::Zsh, &[]);
    let path = init_path(&hook);
    let file_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    let dir_mode = std::fs::metadata(path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(file_mode, 0o600);
    assert_eq!(dir_mode, 0o700);
}

#[test]
fn bash_session_sync_writes_posix_quoted_cd() {
    let (writer, handle, _path) = wired_with_init(ShellKind::Bash);
    handle.emit(format!("\x1b]133;A;navo={TEST_NONCE}\x1b\\").as_bytes());
    let session = Session::with_writer(Arc::clone(&writer));
    assert_eq!(
        session.sync_cwd(Path::new("/home/it's here")).unwrap(),
        None
    );
    assert_eq!(handle.written(), b"cd -- '/home/it'\\''s here'\r");
}

#[cfg(unix)]
#[test]
fn real_posix_shells_restore_user_config_and_reach_a_trusted_prompt() {
    // An isolated HOME keeps the host's own shell config out of the test; each user
    // config prints a sentinel so restoration is proven at runtime, not just in text.
    let home = std::env::temp_dir().join(format!("navo-home-{}", std::process::id()));
    let fish_config = home.join(".config").join("fish");
    std::fs::create_dir_all(&fish_config).unwrap();
    for file in [".bashrc", ".zshrc", ".zshenv"] {
        std::fs::write(home.join(file), "echo NAVO_USER_CONFIG_RAN\n").unwrap();
    }
    std::fs::write(
        fish_config.join("config.fish"),
        "echo NAVO_USER_CONFIG_RAN\n",
    )
    .unwrap();

    for (kind, program) in [
        (ShellKind::Bash, "bash"),
        (ShellKind::Zsh, "zsh"),
        (ShellKind::Fish, "fish"),
    ] {
        let sink = Arc::new(Recorder::default());
        let mut shell = spec(kind, "env", &[]);
        // No `-u ZDOTDIR` here: the hook itself injects the temporary ZDOTDIR.
        shell.args = vec![
            format!("HOME={}", home.display()),
            format!("XDG_CONFIG_HOME={}/.config", home.display()),
            String::from(program),
        ];
        let session = Session::spawn(&shell, Path::new("/"), SIZE, sink.clone()).expect("spawn");
        let seen = sink
            .wait_for(|seen| seen.printed("133;A;navo=") && seen.printed("NAVO_USER_CONFIG_RAN"));
        assert!(
            matches!(session.prompt(), PromptState::AtPrompt { .. }),
            "{kind:?} never reached a trusted prompt; output {:?}",
            String::from_utf8_lossy(&seen.output)
        );
        drop(session);
    }
    let _ = std::fs::remove_dir_all(&home);
}
