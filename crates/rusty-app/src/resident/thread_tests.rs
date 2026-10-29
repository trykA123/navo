use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use rusty_ui::settings::parse_summon_key;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_HOTKEY};

use super::super::events::{ResidentConfig, ResidentEvent, Sink};
use super::super::keys::HOTKEY_ID;
use super::Resident;

const WAIT: Duration = Duration::from_secs(5);

fn config(key: &str) -> ResidentConfig {
    ResidentConfig {
        keep_in_tray: false,
        summon_key: parse_summon_key(key).unwrap(),
    }
}

fn sink() -> (Sink, mpsc::Receiver<ResidentEvent>) {
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    let sink: Sink = Arc::new(move |event| {
        let _ = sender.lock().unwrap().send(event);
    });
    (sink, receiver)
}

#[test]
fn an_idle_config_starts_no_thread() {
    let (sink, _) = sink();
    let resident = Resident::start(ResidentConfig::default(), sink);
    assert_eq!(resident.thread_id(), None);
    assert!(!resident.key_taken());
}

#[test]
fn start_then_stop_joins_the_thread() {
    let (sink, _) = sink();
    let mut resident = Resident::start(config("Ctrl+Alt+Shift+Win+7"), sink);
    assert!(resident.thread_id().is_some());
    assert!(resident.stop());
    assert_eq!(resident.thread_id(), None);
}

#[test]
fn a_hotkey_message_summons() {
    let (sink, events) = sink();
    let resident = Resident::start(config("Ctrl+Alt+Shift+Win+8"), sink);
    let thread = resident.thread_id().unwrap();
    // SAFETY: posting to a live thread id passes no pointers.
    let posted = unsafe { PostThreadMessageW(thread, WM_HOTKEY, HOTKEY_ID as usize, 0) };
    assert_ne!(posted, 0);
    assert!(matches!(
        events.recv_timeout(WAIT),
        Ok(ResidentEvent::Summon(_))
    ));
}

#[test]
fn a_key_held_by_someone_else_is_reported_taken() {
    let (first_sink, _) = sink();
    let (second_sink, _) = sink();
    let first = Resident::start(config("Ctrl+Alt+Shift+Win+9"), first_sink);
    let second = Resident::start(config("Ctrl+Alt+Shift+Win+9"), second_sink);
    assert!(!first.key_taken());
    assert!(second.key_taken());
}
