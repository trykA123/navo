use gpui_kit::{AppContext as _, Keystroke, Modifiers, TestAppContext};
use navo_term::Size;
use navo_term::grid::Modes;

use super::input::{encode_key, encode_paste};
use super::session::TerminalSession;

fn key(name: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: name.to_owned(),
        key_char: key_char.map(str::to_owned),
    }
}

fn plain(name: &str, ch: Option<&str>) -> Keystroke {
    key(name, ch, Modifiers::none())
}

#[test]
fn terminal_input_encodes_printable_utf8_and_basic_control_keys() {
    let modes = Modes::default();
    assert_eq!(
        encode_key(&plain("a", Some("a")), modes),
        Some(b"a".to_vec())
    );
    assert_eq!(
        encode_key(&key("a", Some("A"), Modifiers::shift()), modes),
        Some(b"A".to_vec())
    );
    assert_eq!(
        encode_key(&plain("e", Some("€")), modes),
        Some("€".as_bytes().to_vec())
    );
    assert_eq!(
        encode_key(&plain("enter", None), modes),
        Some(b"\r".to_vec())
    );
    assert_eq!(encode_key(&plain("tab", None), modes), Some(b"\t".to_vec()));
    assert_eq!(
        encode_key(&key("tab", None, Modifiers::shift()), modes),
        Some(b"\x1b[Z".to_vec())
    );
    assert_eq!(
        encode_key(&plain("backspace", None), modes),
        Some(b"\x7f".to_vec())
    );
    assert_eq!(
        encode_key(&plain("escape", None), modes),
        Some(b"\x1b".to_vec())
    );
}

#[test]
fn terminal_input_encodes_arrows_and_nav_keys_for_normal_and_app_cursor_modes() {
    let normal = Modes::default();
    let app = Modes {
        app_cursor: true,
        bracketed_paste: false,
    };

    assert_eq!(
        encode_key(&plain("up", None), normal),
        Some(b"\x1b[A".to_vec())
    );
    assert_eq!(
        encode_key(&plain("down", None), normal),
        Some(b"\x1b[B".to_vec())
    );
    assert_eq!(
        encode_key(&plain("right", None), normal),
        Some(b"\x1b[C".to_vec())
    );
    assert_eq!(
        encode_key(&plain("left", None), normal),
        Some(b"\x1b[D".to_vec())
    );
    assert_eq!(
        encode_key(&plain("home", None), normal),
        Some(b"\x1b[H".to_vec())
    );
    assert_eq!(
        encode_key(&plain("end", None), normal),
        Some(b"\x1b[F".to_vec())
    );

    assert_eq!(
        encode_key(&plain("up", None), app),
        Some(b"\x1bOA".to_vec())
    );
    assert_eq!(
        encode_key(&plain("down", None), app),
        Some(b"\x1bOB".to_vec())
    );
    assert_eq!(
        encode_key(&plain("right", None), app),
        Some(b"\x1bOC".to_vec())
    );
    assert_eq!(
        encode_key(&plain("left", None), app),
        Some(b"\x1bOD".to_vec())
    );
    assert_eq!(
        encode_key(&plain("home", None), app),
        Some(b"\x1bOH".to_vec())
    );
    assert_eq!(
        encode_key(&plain("end", None), app),
        Some(b"\x1bOF".to_vec())
    );

    assert_eq!(
        encode_key(&plain("pageup", None), normal),
        Some(b"\x1b[5~".to_vec())
    );
    assert_eq!(
        encode_key(&plain("pagedown", None), app),
        Some(b"\x1b[6~".to_vec())
    );
    assert_eq!(
        encode_key(&key("up", None, Modifiers::control()), app),
        Some(b"\x1b[1;5A".to_vec())
    );
}

#[test]
fn terminal_input_encodes_ctrl_and_alt_combos() {
    let modes = Modes::default();
    assert_eq!(
        encode_key(&key("a", None, Modifiers::control()), modes),
        Some(vec![0x01])
    );
    assert_eq!(
        encode_key(&key("c", None, Modifiers::control()), modes),
        Some(vec![0x03])
    );
    assert_eq!(
        encode_key(&key("z", None, Modifiers::control()), modes),
        Some(vec![0x1a])
    );
    assert_eq!(
        encode_key(&key("b", Some("b"), Modifiers::alt()), modes),
        Some(b"\x1bb".to_vec())
    );
    assert_eq!(
        encode_key(&key("f", None, Modifiers::alt()), modes),
        Some(b"\x1bf".to_vec())
    );
    assert_eq!(
        encode_key(
            &key(
                "b",
                Some("B"),
                Modifiers {
                    alt: true,
                    shift: true,
                    ..Modifiers::none()
                },
            ),
            modes,
        ),
        Some(b"\x1bB".to_vec())
    );
}

#[test]
fn terminal_input_bracketed_paste_wraps_and_strips_embedded_end_sequences() {
    let off = Modes::default();
    assert_eq!(encode_paste("echo hi\x1b[201~", off), b"echo hi\x1b[201~");

    let on = Modes {
        bracketed_paste: true,
        app_cursor: false,
    };
    assert_eq!(
        encode_paste("echo hi", on),
        b"\x1b[200~echo hi\x1b[201~".to_vec()
    );
    assert_eq!(
        encode_paste("safe\x1b[201~evil", on),
        b"\x1b[200~safeevil\x1b[201~".to_vec()
    );
    assert_eq!(
        encode_paste("a\x1b[20\x1b[201~1~b", on),
        b"\x1b[200~ab\x1b[201~".to_vec()
    );
}

#[gpui_kit::test]
fn terminal_session_wakes_on_output_writes_bytes_resizes_and_shuts_down(cx: &mut TestAppContext) {
    let mut handle_out = None;
    let session = cx.new(|cx| {
        let (session, handle) = TerminalSession::with_fake(Size { cols: 80, rows: 24 }, cx);
        handle_out = Some(handle);
        session
    });
    let handle = handle_out.expect("fake handle");

    handle.emit(b"hello");
    cx.run_until_parked();

    let first_row: String = session.read_with(cx, |session, _| {
        session.snapshot().rows[0]
            .iter()
            .take(5)
            .map(|cell| cell.c)
            .collect()
    });
    assert_eq!(first_row, "hello");

    session.update(cx, |session, _| {
        session.send_bytes(b"ls\r").expect("send");
        session.resize(100, 30).expect("resize");
        session.resize(100, 30).expect("duplicate resize skipped");
    });
    assert_eq!(handle.written(), b"ls\r");
    assert_eq!(
        handle.sizes(),
        vec![Size {
            cols: 100,
            rows: 30
        }]
    );

    session.update(cx, |session, _| session.shutdown());
    assert!(handle.is_dropped());
}
