use gpui_kit::ClipboardItem;

use super::*;

fn paste(rig: &Rig, cx: &mut TestAppContext, text: &str) {
    cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned())));
    rig.press(cx, &["ctrl-shift-v"]);
}

#[gpui_kit::test]
fn pasted_names_override_masks_and_run_as_a_batch(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]-[C]");
    paste(&rig, cx, "one.txt\r\ntwo.txt\r\nthree.txt\r\n");
    assert_eq!(
        rig.new_names(cx),
        names(&["one.txt", "two.txt", "three.txt"])
    );
    rig.press(cx, &["ctrl-down", "enter"]);
    assert!(!rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "d.txt", "one.txt", "three.txt", "two.txt"])
    );
}

#[gpui_kit::test]
fn mismatched_name_count_blocks_the_previous_valid_preview(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "x[N]");
    paste(&rig, cx, "one.txt\ntwo.txt");
    rig.press(cx, &["ctrl-down", "enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn edited_collisions_block_rename_and_show_the_problem(cx: &mut TestAppContext) {
    let rig = opened(cx);
    paste(&rig, cx, "d.txt\ntwo.txt\nthree.txt");
    rig.press(cx, &["ctrl-down", "enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(rig.preview_cursor(cx), Some(0));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn copy_names_exports_the_preview_and_reset_clears_overrides(cx: &mut TestAppContext) {
    let rig = opened(cx);
    paste(&rig, cx, "one.txt\ntwo.txt\nthree.txt");
    rig.press(cx, &["ctrl-shift-c"]);
    let copied = cx
        .update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap();
    assert_eq!(copied.replace("\r\n", "\n"), "one.txt\ntwo.txt\nthree.txt");
    rig.press(cx, &["ctrl-r"]);
    assert_eq!(rig.new_names(cx), names(&["a.txt", "b.txt", "c.txt"]));
}

#[gpui_kit::test]
fn moving_a_row_changes_counter_order_and_executes_that_order(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]-[C]");
    rig.press(cx, &["ctrl-down", "ctrl-shift-down"]);
    assert_eq!(
        rig.rows(cx),
        vec![
            ("b.txt".into(), "b-1.txt".into()),
            ("a.txt".into(), "a-2.txt".into()),
            ("c.txt".into(), "c-3.txt".into())
        ]
    );
    assert_eq!(rig.preview_cursor(cx), Some(1));
    rig.press(cx, &["enter"]);
    assert!(!rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a-2.txt", "b-1.txt", "c-3.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn moving_an_excluded_edited_row_keeps_both_with_its_source(cx: &mut TestAppContext) {
    let rig = opened(cx);
    paste(&rig, cx, "one.txt\ntwo.txt\nthree.txt");
    rig.press(cx, &["ctrl-down", "delete", "ctrl-shift-down"]);
    assert_eq!(
        rig.rows(cx),
        vec![
            ("b.txt".into(), "two.txt".into()),
            ("a.txt".into(), "a.txt".into()),
            ("c.txt".into(), "three.txt".into())
        ]
    );
    rig.press(cx, &["enter"]);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "d.txt", "three.txt", "two.txt"])
    );
}

#[gpui_kit::test]
fn column_click_sorts_the_preview_and_changes_counter_assignment(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]-[C]");
    for _ in 0..2 {
        cx.update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within("rename-head")
                .click(("column-title", 0usize), cx);
        })
        .expect("window");
        rig.wait(cx);
    }
    assert_eq!(
        rig.rows(cx),
        vec![
            ("c.txt".into(), "c-1.txt".into()),
            ("b.txt".into(), "b-2.txt".into()),
            ("a.txt".into(), "a-3.txt".into())
        ]
    );
}

#[gpui_kit::test]
fn file_import_reads_utf8_names_and_runs_without_changing_other_files(cx: &mut TestAppContext) {
    let rig = opened(cx);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "\u{feff}one.txt\r\ntwo.txt\r\nthree.txt\r\n").unwrap();
    rig.press(cx, &["ctrl-shift-o"]);
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![file.path().to_owned()])
    });
    rig.wait(cx);
    assert_eq!(
        rig.new_names(cx),
        names(&["one.txt", "two.txt", "three.txt"])
    );
}

#[gpui_kit::test]
fn the_name_editor_accepts_multiple_lines_and_can_be_hidden(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.press(cx, &["ctrl-e", "ctrl-a"]);
    rig.type_text(cx, "one.txt\ntwo.txt\nthree.txt");
    assert_eq!(
        rig.new_names(cx),
        names(&["one.txt", "two.txt", "three.txt"])
    );
    rig.press(cx, &["ctrl-e"]);
    assert_eq!(
        rig.new_names(cx),
        names(&["one.txt", "two.txt", "three.txt"])
    );
    rig.press(cx, &["enter"]);
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn typing_and_immediately_running_names_uses_the_latest_text(cx: &mut TestAppContext) {
    let rig = opened(cx);
    paste(&rig, cx, "one.txt\ntwo.txt\nthree.txt");
    cx.update_window(rig.window, |_, window, cx| {
        window.press("ctrl-a", cx);
        window.input("latest.txt\ntwo.txt\nthree.txt", cx);
        window.press("ctrl-down", cx);
        window.press("enter", cx);
    })
    .expect("window");
    rig.wait(cx);
    assert!(!rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "d.txt", "latest.txt", "three.txt", "two.txt"])
    );
}

#[gpui_kit::test]
fn invalid_counter_typed_then_enter_cannot_run_the_previous_preview(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "x[N]");
    cx.update_window(rig.window, |_, window, cx| {
        let input = rig.view.read(cx).multi_rename_inputs().unwrap().digits;
        window.focus(&input.read(cx).focus_handle(cx), cx);
        input.update(cx, |input, cx| input.select_all(window, cx));
        window.input("bad", cx);
        window.press("enter", cx);
    })
    .unwrap();
    rig.wait(cx);
    assert!(rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn hiding_the_editor_immediately_after_typing_keeps_the_latest_names(cx: &mut TestAppContext) {
    let rig = opened(cx);
    paste(&rig, cx, "one.txt\ntwo.txt\nthree.txt");
    cx.update_window(rig.window, |_, window, cx| {
        window.press("ctrl-a", cx);
        window.input("latest.txt\ntwo.txt\nthree.txt", cx);
        window.press("ctrl-e", cx);
        window.press("enter", cx);
    })
    .unwrap();
    rig.wait(cx);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "d.txt", "latest.txt", "three.txt", "two.txt"])
    );
}

#[gpui_kit::test]
fn moving_a_row_immediately_after_typing_keeps_the_latest_names(cx: &mut TestAppContext) {
    let rig = opened(cx);
    paste(&rig, cx, "one.txt\ntwo.txt\nthree.txt");
    cx.update_window(rig.window, |_, window, cx| {
        window.press("ctrl-a", cx);
        window.input("latest.txt\ntwo.txt\nthree.txt", cx);
        window.press("ctrl-down", cx);
        window.press("ctrl-shift-down", cx);
    })
    .unwrap();
    rig.wait(cx);
    assert_eq!(
        rig.rows(cx),
        vec![
            ("b.txt".into(), "two.txt".into()),
            ("a.txt".into(), "latest.txt".into()),
            ("c.txt".into(), "three.txt".into())
        ]
    );
}

#[gpui_kit::test]
fn an_invalid_name_list_can_be_reopened_and_corrected(cx: &mut TestAppContext) {
    let rig = opened(cx);
    paste(&rig, cx, "one.txt\ntwo.txt");
    rig.press(cx, &["ctrl-e", "ctrl-e", "ctrl-a"]);
    rig.type_text(cx, "one.txt\ntwo.txt\nthree.txt");
    assert_eq!(
        rig.new_names(cx),
        names(&["one.txt", "two.txt", "three.txt"])
    );
    rig.press(cx, &["ctrl-down", "enter"]);
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn removing_a_source_clamps_the_cursor_before_reordering(cx: &mut TestAppContext) {
    let (rig, fs) = watched_rig(cx);
    rig.press(cx, &["ctrl-down", "pagedown"]);
    assert_eq!(rig.preview_cursor(cx), Some(2));
    fs.remove("home/c.txt");
    let deadline = Instant::now() + Duration::from_secs(5);
    while rig.listing(cx).iter().any(|name| name == "c.txt") && Instant::now() < deadline {
        rig.wait(cx);
    }
    rig.press(cx, &["enter"]);
    assert_eq!(rig.rows(cx).len(), 2);
    rig.press(cx, &["ctrl-shift-up"]);
    assert!(rig.preview_cursor(cx).is_none_or(|row| row < 2));
}

#[gpui_kit::test]
fn clicking_a_displayed_toggle_during_sort_keeps_the_original_source_identity(
    cx: &mut TestAppContext,
) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]-[C]");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("rename-head")
            .click(("column-title", 0usize), cx);
    })
    .unwrap();
    rig.wait(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("rename-head")
            .click(("column-title", 0usize), cx);
        window
            .within("rename-preview")
            .click(("rename-toggle", 0usize), cx);
    })
    .unwrap();
    rig.wait(cx);
    let a = rig
        .rows(cx)
        .into_iter()
        .find(|(old, _)| old == "a.txt")
        .unwrap();
    assert_eq!(a.1, "a.txt");
}

#[gpui_kit::test]
fn an_import_from_a_closed_session_cannot_replace_a_new_sessions_names(cx: &mut TestAppContext) {
    let rig = opened(cx);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "one.txt\ntwo.txt\nthree.txt").unwrap();
    rig.press(cx, &["ctrl-shift-o", "escape", "ctrl-m"]);
    cx.simulate_path_prompt_response(|_| Some(vec![file.path().to_owned()]));
    rig.wait(cx);
    assert_eq!(rig.new_names(cx), names(&["a.txt", "b.txt", "c.txt"]));
}

#[gpui_kit::test]
fn resetting_while_a_file_prompt_is_open_discards_its_late_result(cx: &mut TestAppContext) {
    let rig = opened(cx);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "one.txt\ntwo.txt\nthree.txt").unwrap();
    rig.press(cx, &["ctrl-shift-o", "ctrl-r"]);
    cx.simulate_path_prompt_response(|_| Some(vec![file.path().to_owned()]));
    rig.wait(cx);
    assert_eq!(rig.new_names(cx), names(&["a.txt", "b.txt", "c.txt"]));
}
