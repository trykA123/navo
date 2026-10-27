use super::{Command, CommandId, Fields, ParseError, Problem, Value, string};

pub(super) fn build(fields: &mut Fields<'_>) -> Result<Command, ParseError> {
    Ok(match fields.command {
        CommandId::WorkspaceSwitch => Command::WorkspaceSwitch {
            n: fields.required("n", number)?,
        },
        CommandId::WorkspaceNew => Command::WorkspaceNew {
            name: fields.optional("name", string)?,
        },
        CommandId::WorkspaceRename => Command::WorkspaceRename {
            n: fields.required("n", number)?,
            name: fields.required("name", string)?,
        },
        CommandId::WorkspaceMove => Command::WorkspaceMove {
            n: fields.required("n", number)?,
            by: fields.required("by", movement)?,
        },
        CommandId::WorkspaceDelete => Command::WorkspaceDelete {
            n: fields.required("n", number)?,
        },
        CommandId::WorkspaceRestore => Command::WorkspaceRestore,
        CommandId::WorkspaceSave => Command::WorkspaceSave,
        _ => Command::WorkspaceDiscard {
            n: fields.required("n", number)?,
        },
    })
}

fn number(value: Value<'_>) -> Result<usize, Problem> {
    let Value::Int(text) = value else {
        return Err(Problem::WrongType("integer"));
    };
    text.parse::<usize>()
        .ok()
        .filter(|n| (1..=9).contains(n))
        .ok_or_else(|| Problem::Invalid("expected a whole number from 1 to 9".into()))
}

fn movement(value: Value<'_>) -> Result<i8, Problem> {
    let Value::Int(text) = value else {
        return Err(Problem::WrongType("integer"));
    };
    text.parse::<i8>()
        .ok()
        .filter(|by| matches!(by, -1 | 1))
        .ok_or_else(|| Problem::Invalid("expected -1 or 1".into()))
}
