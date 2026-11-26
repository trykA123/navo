use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext};

use super::{NavoWindow, fake, open_options, options_for, settle, two_panes};
use crate::window_view::{
    BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem, OpenHooks,
    StartOptions,
};

fn item(name: &str, command: &str, line: usize) -> ButtonItem {
    ButtonItem {
        name: name.to_owned(),
        icon: BarIcon::Missing { dashed: false },
        action: ButtonAction::Navo(command.to_owned()),
        line,
    }
}

fn rig(cx: &mut TestAppContext) -> (Entity<NavoWindow>, ButtonBarSender) {
    let fs = fake();
    let state = two_panes(&fs);
    let (hooks, sender) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(|_| Ok(())),
        Arc::new(|| Ok(())),
        None,
    );
    let options = StartOptions {
        open: OpenHooks {
            button_bar: Some(hooks),
            ..OpenHooks::none()
        },
        ..options_for(fs, state)
    };
    let (_, view) = open_options(cx, options);
    (view, sender)
}

fn send(
    cx: &mut TestAppContext,
    view: &Entity<NavoWindow>,
    tx: &ButtonBarSender,
    news: ButtonBarNews,
) {
    assert!(tx.send(news));
    settle(cx, view);
}

fn notice(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<(String, bool)> {
    view.read_with(cx, |view, _| {
        let shown = view.status_notice()?;
        Some((shown.text.to_string(), shown.is_error))
    })
}

#[gpui_kit::test]
fn button_bar_state_applies_and_keeps_on_invalid(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);

    let first = vec![
        item("Refresh", "pane.refresh", 1),
        item("Up", "pane.parent", 2),
    ];
    send(
        cx,
        &view,
        &tx,
        ButtonBarNews::Applied {
            items: first.clone(),
            warnings: Vec::new(),
        },
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.button_bar.items, first);
        assert_eq!(view.button_bar.focused, None);
        assert_eq!(view.button_bar.error, None);
        assert!(view.button_bar.hooks.is_some());
        assert!(
            view.button_bar
                .hooks
                .as_ref()
                .is_some_and(|hooks| hooks.news.is_none())
        );
    });
    assert_eq!(notice(cx, &view), None);

    let error_msg = "line 3: bad".to_owned();
    send(cx, &view, &tx, ButtonBarNews::Invalid(error_msg.clone()));
    view.read_with(cx, |view, _| {
        assert_eq!(view.button_bar.items, first);
        assert_eq!(view.button_bar.error.as_deref(), Some(error_msg.as_str()));
    });
    assert_eq!(notice(cx, &view), Some((error_msg, true)));

    let second = vec![item("Terminal", "pane.terminal", 4)];
    send(
        cx,
        &view,
        &tx,
        ButtonBarNews::Applied {
            items: second.clone(),
            warnings: vec!["first warning".to_owned(), "second warning".to_owned()],
        },
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.button_bar.items, second);
        assert_eq!(view.button_bar.error, None);
    });
    assert_eq!(notice(cx, &view), Some(("first warning".to_owned(), false)));
}
