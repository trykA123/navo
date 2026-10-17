use gpui_kit::Keystroke;

use crate::commands::{Bind, CommandSpec, Keymap, normal_key, shown};

const MODIFIERS: [&str; 7] = [
    "shift", "control", "ctrl", "alt", "platform", "function", "fn",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Set {
        command: &'static str,
        context: Option<String>,
        keys: Vec<String>,
    },
    Reset {
        command: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub spec: CommandSpec,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub key: String,
    pub other: CommandSpec,
    pub context: String,
}

impl Conflict {
    #[must_use]
    pub fn message(&self) -> String {
        let key = shown(&self.key).unwrap_or_else(|| self.key.clone());
        format!("{key} is used by {} — replace it?", self.other.label)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recorded {
    Key(String),
    Unbind,
    Cancel,
}

#[must_use]
pub fn recorded(stroke: &Keystroke) -> Option<Recorded> {
    if stroke.key.is_empty() || MODIFIERS.contains(&stroke.key.as_str()) {
        return None;
    }
    let bare = !stroke.modifiers.modified();
    match stroke.key.as_str() {
        "escape" if bare => Some(Recorded::Cancel),
        "backspace" if bare => Some(Recorded::Unbind),
        _ => normal_key(&stroke.unparse().to_lowercase()).map(Recorded::Key),
    }
}

fn holds(bind: &Bind, key: &str) -> bool {
    bind.keys
        .iter()
        .any(|bound| normal_key(bound).as_deref() == Some(key))
}

fn taker<'a>(keymap: &'a Keymap, proposal: &Proposal, key: &str) -> Option<&'a Bind> {
    keymap.binds().iter().find(|bind| {
        bind.context == proposal.spec.context
            && bind.spec.name != proposal.spec.name
            && holds(bind, key)
    })
}

#[must_use]
pub fn conflict(keymap: &Keymap, proposal: &Proposal) -> Option<Conflict> {
    proposal.keys.iter().find_map(|key| {
        let key = normal_key(key)?;
        let other = taker(keymap, proposal, &key)?;
        Some(Conflict {
            key,
            other: other.spec,
            context: other.context.clone(),
        })
    })
}

#[must_use]
pub fn edits(keymap: &Keymap, proposal: &Proposal) -> Vec<Edit> {
    let mut edits = vec![Edit::Set {
        command: proposal.spec.name,
        context: None,
        keys: proposal.keys.clone(),
    }];
    for key in proposal.keys.iter().filter_map(|key| normal_key(key)) {
        let Some(other) = taker(keymap, proposal, &key) else {
            continue;
        };
        if !keymap.is_user(other.spec.name, &other.context) {
            continue;
        }
        let kept = other
            .keys
            .iter()
            .filter(|bound| normal_key(bound).as_deref() != Some(key.as_str()))
            .cloned()
            .collect();
        let context = (other.context != other.spec.context).then(|| other.context.clone());
        edits.push(Edit::Set {
            command: other.spec.name,
            context,
            keys: kept,
        });
    }
    edits
}
