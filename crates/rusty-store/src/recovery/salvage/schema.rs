use rusqlite::{
    Connection, Row,
    types::{Type, Value},
};

use super::copy::{CopyError, read_error};

pub(super) struct Column {
    pub name: String,
    kind: Type,
    nullable: bool,
}

pub(super) fn shared_columns(
    source: &Connection,
    target: &Connection,
    table: &str,
) -> Result<Vec<Column>, CopyError> {
    let available = source
        .prepare(&format!("SELECT * FROM {table} LIMIT 0"))
        .map_err(read_error)?
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut statement = target
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(CopyError::Destination)?;
    let columns = statement
        .query_map([], column)
        .map_err(CopyError::Destination)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(CopyError::Destination)?;
    Ok(columns
        .into_iter()
        .filter(|column| available.contains(&column.name))
        .collect())
}

fn column(row: &Row<'_>) -> Result<Column, rusqlite::Error> {
    let declared: String = row.get(2)?;
    let kind = match declared.as_str() {
        "INTEGER" => Type::Integer,
        "REAL" => Type::Real,
        "TEXT" => Type::Text,
        "BLOB" => Type::Blob,
        _ => return Err(rusqlite::Error::InvalidColumnType(2, declared, Type::Text)),
    };
    let required: bool = row.get(3)?;
    let primary_key: bool = row.get(5)?;
    Ok(Column {
        name: row.get(1)?,
        kind,
        nullable: !required && !primary_key,
    })
}

pub(super) fn read_row(row: &Row<'_>, columns: &[Column]) -> Result<Vec<Value>, CopyError> {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let value: Value = row.get(index).map_err(read_error)?;
            let actual = value.data_type();
            let valid = if actual == Type::Null {
                column.nullable
            } else {
                actual == column.kind || (column.kind == Type::Real && actual == Type::Integer)
            };
            if !valid {
                return Err(CopyError::Untrusted(format!(
                    "invalid type for {}: expected {:?}, found {actual:?}",
                    column.name, column.kind
                )));
            }
            Ok(value)
        })
        .collect()
}
