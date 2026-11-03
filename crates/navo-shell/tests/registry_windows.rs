#![cfg(windows)]

use navo_shell::{Hive, registry_text};

#[test]
fn product_name_is_not_empty() {
    let name = registry_text(
        Hive::LocalMachine,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "ProductName",
    );
    assert!(name.is_some_and(|name| !name.is_empty()));
}
