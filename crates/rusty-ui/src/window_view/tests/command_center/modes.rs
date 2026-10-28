use super::*;

#[gpui_kit::test]
fn alt_f7_opens_content_scoped_to_each_active_pane(cx: &mut TestAppContext) {
    let rig = rig(cx);
    for _ in 0..2 {
        let folder = rig.here(cx);
        rig.press(cx, "alt-f7");
        assert!(rig.is_open(cx));
        assert_eq!(super::pressed_mode(&rig, cx), [1]);
        let label = rig.label(cx, "command-center-scope").expect("scope chip");
        assert!(label.contains(folder.rsplit('/').next().unwrap()));
        assert!(rig.rows(cx).is_empty());
        rig.press(cx, "escape");
        rig.press(cx, "tab");
        rig.view.update(cx, |view, _| {
            let _ = view.browser.command(&rusty_core::Command::PaneNavigate {
                location: location("dest"),
            });
        });
        settle(cx, &rig.view);
    }
}

#[gpui_kit::test]
fn backspace_removes_the_scope_only_after_the_query_is_empty(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "alt-f7");
    rig.type_query(cx, "a");
    rig.press(cx, "backspace");
    assert!(rig.label(cx, "command-center-scope").is_some());
    rig.press(cx, "backspace");
    assert!(rig.label(cx, "command-center-scope").is_none());
    assert_eq!(super::pressed_mode(&rig, cx), [1]);
    assert!(rig.is_open(cx));
}

#[gpui_kit::test]
fn the_last_names_row_switches_to_content_with_the_query_and_folder(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-k");
    let initial = rig.rows(cx);
    rig.type_query(cx, "report");
    let rows = rig.rows(cx);
    assert!(rows.len() > initial.len() || rows.last() != initial.last());
    assert!(
        rows.last()
            .is_some_and(|row| row.contains("report") && row.contains("home"))
    );
    rig.press(cx, "ctrl-end");
    rig.press(cx, "enter");
    assert!(rig.is_open(cx));
    assert_eq!(super::pressed_mode(&rig, cx), [1]);
    assert_eq!(
        rig.view
            .read_with(cx, |view, cx| view.command_center_query(cx))
            .as_deref(),
        Some("report")
    );
    assert!(rig.label(cx, "command-center-scope").is_some());
    assert!(rig.rows(cx).is_empty());
}

struct Observe(Arc<Mutex<Vec<Cancel>>>);

impl ResultSource for Observe {
    fn id(&self) -> &'static str {
        "observe"
    }

    fn group(&self) -> Group {
        Group::Commands
    }

    fn scopes(&self) -> Scopes {
        Scopes::ALL
    }

    fn search(&self, _: &Request, _: &HitSink, cancel: &Cancel) {
        self.0.lock().unwrap().push(cancel.clone());
    }
}

#[gpui_kit::test]
fn switching_modes_cancels_the_search_and_rejects_queued_results(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let cancels = Arc::new(Mutex::new(Vec::new()));
    rig.view.update(cx, |view, _| {
        view.command_center
            .sources
            .push(Arc::new(Observe(cancels.clone())))
    });
    rig.press(cx, "ctrl-k");
    super::late::input_unsettled(cx, &rig, "switch pane");
    cx.run_until_parked();
    assert!(!cancels.lock().unwrap().is_empty());
    let old = rig
        .view
        .read_with(cx, |view, _| view.command_center_generation())
        .unwrap();
    rig.press(cx, "tab");
    assert!(cancels.lock().unwrap().iter().all(Cancel::is_cancelled));
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            view.receive_hits(
                Update {
                    generation: old,
                    scope: Scope::Mixed,
                    hits: vec![hit("Late", Group::Files, Target::GoTo(location("dest")))],
                    notes: Notes::default(),
                    settled: true,
                },
                window,
                cx,
            );
        });
    })
    .expect("window");
    cx.executor().advance_clock(Duration::from_millis(100));
    settle(cx, &rig.view);
    assert!(rig.rows(cx).is_empty());
    assert_eq!(
        rig.view
            .read_with(cx, |view, _| view.command_center_selection()),
        None
    );
}
