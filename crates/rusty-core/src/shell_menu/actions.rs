use super::{FerriteAction, Group, ItemId, MenuItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionContext {
    pub count: usize,
    pub one_per_pane: bool,
}

#[must_use]
pub fn ferrite_actions(context: ActionContext) -> Vec<MenuItem> {
    let multi = context.count > 1;
    let rename = if multi {
        format!("Multi-rename {} files…", context.count)
    } else {
        "Rename\tF2".to_owned()
    };
    let can_compare = context.count == 2 || context.one_per_pane;
    [
        (FerriteAction::CopyPath, "Copy path".to_owned(), 'P'),
        (FerriteAction::CopyName, "Copy name".to_owned(), 'N'),
        (FerriteAction::Rename, rename, 'R'),
        (
            FerriteAction::Compare,
            "Compare with other pane".to_owned(),
            'M',
        ),
        (
            FerriteAction::OpenInOtherPane,
            "Open in other pane".to_owned(),
            'O',
        ),
        (FerriteAction::Properties, "Properties".to_owned(), 'I'),
        (
            FerriteAction::ClassicMenu,
            "Classic menu\tShift+F10".to_owned(),
            'C',
        ),
    ]
    .into_iter()
    .map(|(action, text, key)| MenuItem {
        access_key: Some(key),
        enabled: action != FerriteAction::Compare || can_compare,
        ..MenuItem::new(ItemId::Ferrite(action), &text, Group::Ferrite)
    })
    .collect()
}
