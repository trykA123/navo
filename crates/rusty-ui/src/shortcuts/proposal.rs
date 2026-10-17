use gpui_kit::Keystroke;

use crate::commands::{Bind, CommandSpec, GLOBAL, Keymap, is_within, normal_key, shown};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    Same,
    Inner,
    Outer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub key: String,
    pub other: CommandSpec,
    pub context: String,
    pub reach: Reach,
}

impl Conflict {
    #[must_use]
    pub fn message(&self) -> String {
        let key = shown(&self.key).unwrap_or_else(|| self.key.clone());
        let label = self.other.label;
        match self.reach {
            Reach::Same => format!("{key} is used by {label} — replace it?"),
            Reach::Outer => {
                format!("{key} is used by {label} — your key would hide it here. Replace?")
            }
            Reach::Inner => {
                format!("{key} is used by {label} — it would hide your key there. Replace?")
            }
        }
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

fn reach(mine: &str, theirs: &str) -> Option<Reach> {
    if mine == theirs {
        Some(Reach::Same)
    } else if mine == GLOBAL || theirs == GLOBAL || is_within(mine, theirs) {
        Some(Reach::Outer)
    } else if is_within(theirs, mine) {
        Some(Reach::Inner)
    } else {
        None
    }
}

fn contexts(keymap: &Keymap, spec: &CommandSpec) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for bind in keymap
        .binds()
        .iter()
        .filter(|bind| bind.spec.name == spec.name)
    {
        if !found.contains(&bind.context) {
            found.push(bind.context.clone());
        }
    }
    if found.is_empty() {
        found.push(spec.context.to_owned());
    }
    found
}

fn slot(spec: &CommandSpec, context: &str) -> Option<String> {
    (context != spec.context).then(|| context.to_owned())
}

struct Taken<'a> {
    bind: &'a Bind,
    key: String,
    reach: Reach,
}

fn takers<'a>(keymap: &'a Keymap, proposal: &Proposal) -> Vec<Taken<'a>> {
    let mine = contexts(keymap, &proposal.spec);
    let keys: Vec<String> = proposal
        .keys
        .iter()
        .filter_map(|key| normal_key(key))
        .collect();
    keymap
        .binds()
        .iter()
        .filter(|bind| bind.spec.name != proposal.spec.name)
        .filter_map(|bind| taken(bind, &mine, &keys))
        .collect()
}

fn taken<'a>(bind: &'a Bind, mine: &[String], keys: &[String]) -> Option<Taken<'a>> {
    let key = keys.iter().find(|key| holds(bind, key))?;
    let reach = mine
        .iter()
        .filter_map(|context| reach(context, &bind.context))
        .min()?;
    Some(Taken {
        bind,
        key: key.clone(),
        reach,
    })
}

#[must_use]
pub fn conflict(keymap: &Keymap, proposal: &Proposal) -> Option<Conflict> {
    takers(keymap, proposal)
        .into_iter()
        .next()
        .map(|taken| Conflict {
            key: taken.key,
            other: taken.bind.spec,
            context: taken.bind.context.clone(),
            reach: taken.reach,
        })
}

fn is_freed(keymap: &Keymap, taken: &Taken) -> bool {
    match taken.reach {
        Reach::Same => keymap.is_user(taken.bind.spec.name, &taken.bind.context),
        Reach::Inner => true,
        Reach::Outer => false,
    }
}

#[must_use]
pub fn edits(keymap: &Keymap, proposal: &Proposal) -> Vec<Edit> {
    let spec = &proposal.spec;
    let mut edits: Vec<Edit> = contexts(keymap, spec)
        .iter()
        .map(|context| Edit::Set {
            command: spec.name,
            context: slot(spec, context),
            keys: proposal.keys.clone(),
        })
        .collect();
    for taken in takers(keymap, proposal) {
        if !is_freed(keymap, &taken) {
            continue;
        }
        let other = taken.bind;
        let kept = other
            .keys
            .iter()
            .filter(|bound| normal_key(bound).as_deref() != Some(taken.key.as_str()))
            .cloned()
            .collect();
        edits.push(Edit::Set {
            command: other.spec.name,
            context: slot(&other.spec, &other.context),
            keys: kept,
        });
    }
    edits
}
