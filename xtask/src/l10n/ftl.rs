use std::collections::BTreeMap;

use fluent_syntax::ast::Entry;
use fluent_syntax::parser::parse;

pub(super) struct Defined {
    pub ids: BTreeMap<String, String>,
    pub problems: Vec<String>,
}

pub(super) fn defined(files: &[(String, String)]) -> Defined {
    let mut ids = BTreeMap::new();
    let mut problems = Vec::new();
    for (name, text) in files {
        let (resource, errors) = match parse(text.as_str()) {
            Ok(resource) => (resource, Vec::new()),
            Err((resource, errors)) => (resource, errors),
        };
        for error in errors {
            let line = text[..error.pos.start.min(text.len())]
                .matches('\n')
                .count()
                + 1;
            problems.push(format!("{name}:{line}: {:?}", error.kind));
        }
        for entry in resource.body {
            let Entry::Message(message) = entry else {
                continue;
            };
            let id = message.id.name.to_owned();
            if let Some(first) = ids.insert(id.clone(), name.clone()) {
                problems.push(format!("{name}: {id} is already defined in {first}"));
            }
        }
    }
    Defined { ids, problems }
}
