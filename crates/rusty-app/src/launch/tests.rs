use rusty_core::Location;

use super::command_for;

#[test]
fn a_local_file_opens_with_the_system_handler() {
    let file = Location::new("/home/admin/notes.txt").unwrap();
    let command = command_for(&file).unwrap();
    let program = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    assert_eq!(command.get_program(), program);
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, ["/home/admin/notes.txt"]);
}

#[test]
fn a_location_from_another_provider_is_not_launched() {
    let remote = Location::new("fake://home/a.txt").unwrap();
    assert!(command_for(&remote).is_none());
}
