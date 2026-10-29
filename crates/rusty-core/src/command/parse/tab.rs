use super::{Command, CommandId, Fields, ParseError, Problem, Value, flag, pane_number, string};
use crate::{TabCommand, TabId};

pub(super) const fn field_names(command: CommandId) -> &'static [&'static str] {
    match command {
        CommandId::TabPin => &["tab", "pinned"],
        CommandId::TabRename => &["tab", "label"],
        CommandId::TabMoveToWorkspace | CommandId::TabMoveToPane => &["tab", "n"],
        CommandId::TabDuplicate
        | CommandId::TabClose
        | CommandId::TabCloseOthers
        | CommandId::TabCloseRight
        | CommandId::TabMoveToNewWorkspace
        | CommandId::TabMoveToNewPane => &["tab"],
        _ => &[],
    }
}

pub(super) fn build(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    if fields.command == CommandId::TabReopen {
        return Ok(Command::Tab(TabCommand::Reopen));
    }
    let tab = fields.optional("tab", |value| tab_id(&value))?;
    Ok(Command::Tab(match fields.command {
        CommandId::TabPin => TabCommand::Pin {
            tab,
            pinned: fields.required("pinned", |value| flag(&value))?,
        },
        CommandId::TabRename => TabCommand::Rename {
            tab,
            label: fields.required("label", string)?,
        },
        CommandId::TabMoveToWorkspace => TabCommand::MoveToWorkspace {
            tab,
            n: fields.required("n", |value| super::workspace::number(&value))?,
        },
        CommandId::TabMoveToPane => TabCommand::MoveToPane {
            tab,
            n: fields.required("n", |value| pane_number(&value))?,
        },
        CommandId::TabDuplicate => TabCommand::Duplicate { tab },
        CommandId::TabClose => TabCommand::Close { tab },
        CommandId::TabCloseOthers => TabCommand::CloseOthers { tab },
        CommandId::TabCloseRight => TabCommand::CloseRight { tab },
        CommandId::TabMoveToNewWorkspace => TabCommand::MoveToNewWorkspace { tab },
        _ => TabCommand::MoveToNewPane { tab },
    }))
}

fn tab_id(value: &Value<'_>) -> Result<TabId, Problem> {
    super::unsigned::<u64>(value).map(TabId::new)
}
