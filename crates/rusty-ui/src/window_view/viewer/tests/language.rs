use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::component::highlighter::LanguageRegistry;
use rusty_fs::FakeFs;

use super::{Setup, editor_of, redraw, window_on};
use crate::window_view::tests::press;

fn language_after_f3(cx: &mut TestAppContext, name: &str, text: &str) -> String {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file(&format!("home/{name}"), 3);
    fs.add_folder("dest");
    let setup = Setup::new(text.as_bytes().to_vec());
    let (window, view, _) = window_on(cx, Arc::new(fs), setup);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let language = editor.read_with(cx, |state, _| state.language_name().to_string());
    let config = LanguageRegistry::singleton().language(&language);
    assert!(
        config.is_some_and(|config| config.has_grammar()),
        "{language}"
    );
    language
}

#[gpui_kit::test]
fn f3_on_a_powershell_script_uses_the_powershell_grammar(cx: &mut TestAppContext) {
    let language = language_after_f3(cx, "build.ps1", "Write-Host \"hi\"\n");
    assert_eq!(language, "powershell");
}

#[gpui_kit::test]
fn f3_on_an_arxml_file_uses_the_xml_grammar(cx: &mut TestAppContext) {
    let language = language_after_f3(cx, "ecu.arxml", "<AUTOSAR></AUTOSAR>\n");
    assert_eq!(language, "xml");
}
