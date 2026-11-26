//! Comment-preserving structural edits for `buttonbar.toml`.

use navo_icons::spec::{FieldValue, IconFields, IconSpec, to_fields};
use navo_ui::window_view::{BarEdit, DraftAction, ItemDraft, ItemPath};
use toml_edit::{ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

const EMPTY_HEADER: &str = "# Navo button bar. Each [[item]] is one button, separator or menu.\n\n";
const NON_MENU_ACTION_KEYS: [&str; 4] = ["navo", "user", "program", "separator"];
const ALL_ACTION_KEYS: [&str; 7] = [
    "navo",
    "user",
    "program",
    "args",
    "start-in",
    "console",
    "separator",
];

/// Errors returned when applying a structural edit to `buttonbar.toml`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum EditError {
    /// The existing file text could not be parsed as TOML.
    #[error("failed to parse buttonbar.toml: {0}")]
    Parse(String),
    /// The addressed item or submenu child does not exist.
    #[error("no button bar item at {0:?}")]
    NoItem(ItemPath),
    /// The requested edit target is invalid.
    #[error("invalid button bar edit target")]
    BadTarget,
}

/// Applies a comment-preserving structural edit to `buttonbar.toml` text.
pub(crate) fn apply(text: &str, edit: &BarEdit) -> Result<String, EditError> {
    let is_blank = text.trim().is_empty();
    let source = if is_blank { "" } else { text };
    let mut doc: DocumentMut = source
        .parse()
        .map_err(|err: toml_edit::TomlError| EditError::Parse(err.to_string()))?;

    match edit {
        BarEdit::Insert { at, draft } => insert_item(&mut doc, *at, draft, is_blank)?,
        BarEdit::Update { at, draft } => update_item(&mut doc, *at, draft)?,
        BarEdit::Remove { at } => remove_item(&mut doc, *at)?,
        BarEdit::Move { from, to } => move_item(&mut doc, *from, *to)?,
        BarEdit::Duplicate { at } => duplicate_item(&mut doc, *at)?,
    }

    Ok(doc.to_string())
}

fn insert_item(
    doc: &mut DocumentMut,
    at: ItemPath,
    draft: &ItemDraft,
    is_blank: bool,
) -> Result<(), EditError> {
    if let Some(child_idx) = at.child {
        if matches!(draft.action, DraftAction::Menu) {
            return Err(EditError::BadTarget);
        }
        let items = top_items_mut(doc).ok_or(EditError::NoItem(at))?;
        let parent = items.get_mut(at.index).ok_or(EditError::NoItem(at))?;
        let table = new_table_from_draft(draft, "\n");
        return insert_into_menu(parent, child_idx, at, table);
    }

    let len = top_items_len(doc)?;
    if at.index > len {
        return Err(EditError::NoItem(at));
    }
    let items = ensure_top_items_mut(doc)?;
    let prefix = if is_blank && items.is_empty() {
        EMPTY_HEADER
    } else {
        "\n"
    };
    let table = new_table_from_draft(draft, prefix);
    insert_at(items, at.index, table);
    Ok(())
}

fn update_item(doc: &mut DocumentMut, at: ItemPath, draft: &ItemDraft) -> Result<(), EditError> {
    if at.child.is_some() && matches!(draft.action, DraftAction::Menu) {
        return Err(EditError::BadTarget);
    }
    let table = item_table_mut(doc, at)?;
    apply_draft(table, draft);
    Ok(())
}

fn remove_item(doc: &mut DocumentMut, at: ItemPath) -> Result<(), EditError> {
    let items = top_items_mut(doc).ok_or(EditError::NoItem(at))?;
    if let Some(child_idx) = at.child {
        let parent = items.get_mut(at.index).ok_or(EditError::NoItem(at))?;
        let _ = remove_from_menu(parent, child_idx, at)?;
        return Ok(());
    }
    if at.index >= items.len() {
        return Err(EditError::NoItem(at));
    }
    let _ = items.remove(at.index);
    if items.is_empty() {
        doc.as_table_mut().remove("item");
    }
    Ok(())
}

fn move_item(doc: &mut DocumentMut, from: ItemPath, to: ItemPath) -> Result<(), EditError> {
    let items = top_items_mut(doc).ok_or(EditError::NoItem(from))?;
    validate_move(items, from, to)?;
    if from == to {
        return Ok(());
    }

    let mut moved = match from.child {
        None => take_at(items, from.index).ok_or(EditError::NoItem(from))?,
        Some(fc) => {
            let parent = items.get_mut(from.index).ok_or(EditError::NoItem(from))?;
            remove_from_menu(parent, fc, from)?
        }
    };
    moved.set_position(None);
    if moved.decor().prefix().is_none() {
        moved.decor_mut().set_prefix("\n");
    }

    match to.child {
        None => {
            insert_at(items, to.index, moved);
            Ok(())
        }
        Some(tc) => {
            let parent_idx = if from.child.is_none() && from.index < to.index {
                to.index - 1
            } else {
                to.index
            };
            let parent = items.get_mut(parent_idx).ok_or(EditError::NoItem(to))?;
            insert_into_menu(parent, tc, to, moved)
        }
    }
}

/// Validates source existence, destination bounds, and nesting rules before mutating `items`.
fn validate_move(items: &ArrayOfTables, from: ItemPath, to: ItemPath) -> Result<(), EditError> {
    let src_parent = items.get(from.index).ok_or(EditError::NoItem(from))?;
    if let Some(fc) = from.child {
        let src_menu = menu_array(src_parent).ok_or(EditError::NoItem(from))?;
        if fc >= src_menu.len() {
            return Err(EditError::NoItem(from));
        }
    }

    match (from.child, to.child) {
        (None, None) => {
            if to.index >= items.len() {
                return Err(EditError::NoItem(to));
            }
        }
        (Some(_), None) => {
            if to.index > items.len() {
                return Err(EditError::NoItem(to));
            }
        }
        (None, Some(tc)) => {
            if from.index == to.index || is_menu_table(src_parent) {
                return Err(EditError::BadTarget);
            }
            validate_menu_target(items, to, tc, false)?;
        }
        (Some(_), Some(tc)) => {
            let same_menu = from.index == to.index;
            validate_menu_target(items, to, tc, same_menu)?;
        }
    }
    Ok(())
}

fn validate_menu_target(
    items: &ArrayOfTables,
    to: ItemPath,
    child_idx: usize,
    same_menu: bool,
) -> Result<(), EditError> {
    let dst_parent = items.get(to.index).ok_or(EditError::NoItem(to))?;
    if has_non_menu_action(dst_parent) {
        return Err(EditError::BadTarget);
    }
    let menu_len = match dst_parent.get("menu") {
        None => 0,
        Some(Item::ArrayOfTables(arr)) => arr.len(),
        Some(_) => return Err(EditError::BadTarget),
    };
    let out_of_range = if same_menu {
        child_idx >= menu_len
    } else {
        child_idx > menu_len
    };
    if out_of_range {
        return Err(EditError::NoItem(to));
    }
    Ok(())
}

fn duplicate_item(doc: &mut DocumentMut, at: ItemPath) -> Result<(), EditError> {
    let items = top_items_mut(doc).ok_or(EditError::NoItem(at))?;
    if let Some(child_idx) = at.child {
        let parent = items.get_mut(at.index).ok_or(EditError::NoItem(at))?;
        let menu = menu_array_mut(parent).ok_or(EditError::NoItem(at))?;
        let copy = clone_with_copy_name(menu.get(child_idx).ok_or(EditError::NoItem(at))?);
        insert_at(menu, child_idx + 1, copy);
        return Ok(());
    }
    let copy = clone_with_copy_name(items.get(at.index).ok_or(EditError::NoItem(at))?);
    insert_at(items, at.index + 1, copy);
    Ok(())
}

fn clone_with_copy_name(source: &Table) -> Table {
    let mut copy = source.clone();
    copy.set_position(None);
    copy.decor_mut().set_prefix("\n");
    if let Some(name) = copy.get("name").and_then(Item::as_str).map(str::to_owned) {
        set_value(&mut copy, "name", Value::from(format!("{name} copy")));
    }
    copy
}

fn new_table_from_draft(draft: &ItemDraft, prefix: &str) -> Table {
    let mut table = Table::new();
    table.decor_mut().set_prefix(prefix);
    apply_draft(&mut table, draft);
    table
}

fn apply_draft(table: &mut Table, draft: &ItemDraft) {
    if matches!(draft.action, DraftAction::Separator) {
        remove_keys(table, &ALL_ACTION_KEYS);
        table.remove("name");
        table.remove("icon");
        table.remove("menu");
        set_value(table, "separator", Value::from(true));
        return;
    }

    set_value(table, "name", Value::from(draft.name.as_str()));
    match &draft.icon {
        Some(spec) => set_value(table, "icon", icon_value(spec)),
        None => {
            table.remove("icon");
        }
    }

    match &draft.action {
        DraftAction::Navo(id) => {
            set_value(table, "navo", Value::from(id.as_str()));
            remove_keys(
                table,
                &[
                    "user",
                    "program",
                    "args",
                    "start-in",
                    "console",
                    "separator",
                    "menu",
                ],
            );
        }
        DraftAction::User(name) => {
            set_value(table, "user", Value::from(name.as_str()));
            remove_keys(
                table,
                &[
                    "navo",
                    "program",
                    "args",
                    "start-in",
                    "console",
                    "separator",
                    "menu",
                ],
            );
        }
        DraftAction::Program {
            program,
            args,
            start_in,
            hidden_console,
        } => {
            set_value(table, "program", Value::from(program.as_str()));
            set_optional_text(table, "args", args);
            set_optional_text(table, "start-in", start_in);
            if *hidden_console {
                set_value(table, "console", Value::from("hide"));
            } else {
                table.remove("console");
            }
            remove_keys(table, &["navo", "user", "separator", "menu"]);
        }
        DraftAction::Menu => {
            remove_keys(table, &ALL_ACTION_KEYS);
        }
        DraftAction::Separator => {}
    }
}

fn icon_value(spec: &IconSpec) -> Value {
    match to_fields(spec) {
        IconFields::Text(text) => Value::from(text),
        IconFields::Table(entries) => {
            let mut inline = InlineTable::new();
            for (key, field_val) in entries {
                let val = match field_val {
                    FieldValue::Text(s) => Value::from(s),
                    FieldValue::Flag(flag) => Value::from(flag),
                };
                inline.insert(key, val);
            }
            inline.fmt();
            Value::InlineTable(inline)
        }
    }
}

fn set_optional_text(table: &mut Table, key: &str, text: &str) {
    if text.is_empty() {
        table.remove(key);
    } else {
        set_value(table, key, Value::from(text));
    }
}

fn set_value(table: &mut Table, key: &str, mut val: Value) {
    match table.get_mut(key) {
        Some(Item::Value(old)) => {
            *val.decor_mut() = old.decor().clone();
            *old = val;
        }
        _ => {
            table.insert(key, Item::Value(val));
        }
    }
}

fn remove_keys(table: &mut Table, keys: &[&str]) {
    for key in keys {
        table.remove(key);
    }
}

fn top_items_len(doc: &DocumentMut) -> Result<usize, EditError> {
    match doc.get("item") {
        None => Ok(0),
        Some(Item::ArrayOfTables(arr)) => Ok(arr.len()),
        Some(_) => Err(EditError::BadTarget),
    }
}

fn top_items_mut(doc: &mut DocumentMut) -> Option<&mut ArrayOfTables> {
    doc.get_mut("item").and_then(Item::as_array_of_tables_mut)
}

fn ensure_top_items_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, EditError> {
    doc.entry("item")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or(EditError::BadTarget)
}

fn item_table_mut(doc: &mut DocumentMut, at: ItemPath) -> Result<&mut Table, EditError> {
    let items = top_items_mut(doc).ok_or(EditError::NoItem(at))?;
    let parent = items.get_mut(at.index).ok_or(EditError::NoItem(at))?;
    match at.child {
        None => Ok(parent),
        Some(child_idx) => menu_array_mut(parent)
            .and_then(|menu| menu.get_mut(child_idx))
            .ok_or(EditError::NoItem(at)),
    }
}

fn menu_array(table: &Table) -> Option<&ArrayOfTables> {
    table.get("menu").and_then(Item::as_array_of_tables)
}

fn menu_array_mut(table: &mut Table) -> Option<&mut ArrayOfTables> {
    table.get_mut("menu").and_then(Item::as_array_of_tables_mut)
}

fn insert_into_menu(
    parent: &mut Table,
    child_idx: usize,
    at: ItemPath,
    child: Table,
) -> Result<(), EditError> {
    if has_non_menu_action(parent) {
        return Err(EditError::BadTarget);
    }
    let menu_len = match parent.get("menu") {
        None => 0,
        Some(Item::ArrayOfTables(arr)) => arr.len(),
        Some(_) => return Err(EditError::BadTarget),
    };
    if child_idx > menu_len {
        return Err(EditError::NoItem(at));
    }
    let menu = parent
        .entry("menu")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or(EditError::BadTarget)?;
    insert_at(menu, child_idx, child);
    Ok(())
}

fn remove_from_menu(
    parent: &mut Table,
    child_idx: usize,
    at: ItemPath,
) -> Result<Table, EditError> {
    let menu = menu_array_mut(parent).ok_or(EditError::NoItem(at))?;
    let removed = take_at(menu, child_idx).ok_or(EditError::NoItem(at))?;
    if menu.is_empty() {
        parent.remove("menu");
    }
    Ok(removed)
}

/// Inserts `table` at `index` in `array` by draining and re-pushing the tail, then resets
/// table positions so `toml_edit` serializes tables in array order.
fn insert_at(array: &mut ArrayOfTables, index: usize, table: Table) {
    let mut tail = Vec::with_capacity(array.len().saturating_sub(index));
    while array.len() > index {
        if let Some(existing) = take_at(array, index) {
            tail.push(existing);
        }
    }
    array.push(table);
    for existing in tail {
        array.push(existing);
    }
    normalize_positions(array);
}

fn take_at(array: &mut ArrayOfTables, index: usize) -> Option<Table> {
    let table = array.get(index)?.clone();
    array.remove(index);
    Some(table)
}

fn normalize_positions(array: &mut ArrayOfTables) {
    for table in array.iter_mut() {
        table.set_position(None);
        if let Some(menu) = menu_array_mut(table) {
            for child in menu.iter_mut() {
                child.set_position(None);
            }
        }
    }
}

fn has_non_menu_action(table: &Table) -> bool {
    NON_MENU_ACTION_KEYS
        .iter()
        .any(|key| table.contains_key(key))
}

fn is_menu_table(table: &Table) -> bool {
    table.contains_key("menu") || !has_non_menu_action(table)
}

#[cfg(test)]
mod tests;
