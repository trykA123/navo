use std::collections::BTreeMap;

use super::{Settings, SettingsError};

pub const UNUSED_NOTE: &str = "# no longer used by Ferrite";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultChange {
    pub key: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    pub text: String,
    pub written: Vec<(String, String)>,
    pub changes: Vec<DefaultChange>,
}

pub type Ledger = BTreeMap<String, String>;

pub fn complete_text(text: &str, defaults: &Settings) -> Result<Reconciled, SettingsError> {
    let _ = (text, defaults);
    todo!()
}

pub fn upgrade_text(
    text: &str,
    ledger: &Ledger,
    defaults: &Settings,
) -> Result<Reconciled, SettingsError> {
    let _ = (text, ledger, defaults);
    todo!()
}

pub fn mark_unused(text: &str) -> Result<String, SettingsError> {
    let _ = text;
    todo!()
}

pub fn reconcile(
    text: &str,
    ledger: &Ledger,
    defaults: &Settings,
) -> Result<Reconciled, SettingsError> {
    let _ = (text, ledger, defaults);
    todo!()
}
