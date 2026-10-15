use std::fmt;

use serde_json::{Number, Value};
use tracing::field::{Field, Visit};

use super::is_redacted_path;
use crate::Redactor;

pub(super) struct FieldVisitor<'a> {
    redactor: &'a Redactor,
    pub(super) message: String,
    pub(super) fields: Vec<(String, Value)>,
    pub(super) misses: u64,
}

impl<'a> FieldVisitor<'a> {
    pub(super) fn new(redactor: &'a Redactor) -> Self {
        Self {
            redactor,
            message: String::new(),
            fields: Vec::new(),
            misses: 0,
        }
    }

    fn push(&mut self, field: &Field, value: Value) {
        if is_path_field(field.name()) {
            self.misses += 1;
        } else {
            self.fields.push((field.name().to_owned(), value));
        }
    }

    fn push_text(&mut self, field: &Field, text: &str) {
        if field.name() == "message" {
            self.message = self.redactor.text(text);
        } else if is_query_field(field.name()) {
            let hidden = format!("<text:{} chars>", text.chars().count());
            self.push(field, Value::String(hidden));
        } else {
            let redacted = self.redactor.text(text);
            self.push(field, Value::String(redacted));
        }
    }
}

fn is_path_field(name: &str) -> bool {
    name == "path" || name.ends_with("_path")
}

fn is_query_field(name: &str) -> bool {
    matches!(name, "query" | "search" | "filter" | "pattern") || name.ends_with("_query")
}

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let text = format!("{value:?}");
        if is_path_field(field.name()) && is_redacted_path(&text) {
            self.fields
                .push((field.name().to_owned(), Value::String(text)));
        } else {
            self.push_text(field, &text);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push_text(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push(field, Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        let number = Number::from_f64(value).map_or(Value::Null, Value::Number);
        self.push(field, number);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push(field, Value::from(value));
    }
}
