//! Unit tests for the compare-screen boundary scanner (N8a).

use super::scan;

#[test]
fn seams_pass_clean_text() {
    let code = "let session = FolderSession::new(run, control, roots);\nsession.step();\n";
    assert_eq!(scan(code), Vec::<usize>::new());
}

#[test]
fn seams_flag_navo_window() {
    let code = "fn poll(&mut self) {\n    let window: &NavoWindow = self;\n}\n";
    assert_eq!(scan(code), vec![2]);
}

#[test]
fn seams_flag_browser_and_its_field() {
    let code = "let browser: &Browser = host;\nlet rows = self.browser.rows();\n";
    assert_eq!(scan(code), vec![1, 2]);
}

#[test]
fn seams_ignore_lowercase_module_paths() {
    let code = "use crate::browser::FolderSession;\n";
    assert_eq!(scan(code), Vec::<usize>::new());
}
