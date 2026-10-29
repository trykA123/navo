use std::collections::HashMap;
use std::ops::ControlFlow;

use rusty_core::retention::{MAX_NODES, RetainedNode, RetentionRecord};
use rusty_core::{Location, ProviderPath};
use rusty_fs::{EntryBatch, EntryKind, ProviderOps};

use crate::{Error, RetentionRun, ancestry::Ancestors, proof};

pub(super) fn read(
    ops: &dyn ProviderOps,
    (root, scope): (&Location, proof::Scope),
    run: &mut RetentionRun<'_>,
) -> Result<Vec<RetainedNode>, Error> {
    let mut pending = vec![(Vec::<String>::new(), root.clone())];
    let mut nodes = Vec::new();
    let mut ancestors = Ancestors::new(root, scope);
    while let Some((path, location)) = pending.pop() {
        run.visit(&location)?;
        if !ancestors.verify(ops, &path)? {
            return Err(Error::Changed(location));
        }
        if nodes.len() >= MAX_NODES {
            return Err(Error::Unsupported(root.clone()));
        }
        let found = ops.metadata(&location)?;
        if found.kind == EntryKind::Folder && reserved(&found.name.to_text()) {
            return Err(Error::Unsupported(location));
        }
        let identity = proof::identity(ops, &location)?;
        proof::on_scope(ops, &location, scope)?;
        if found.kind == EntryKind::Folder {
            ancestors.remember(&path, identity);
            children(ops, (&path, &location), &mut pending, run)?;
        }
        nodes.push(retained_node(path, &found, identity));
    }
    Ok(nodes)
}

type Pending = Vec<(Vec<String>, Location)>;

fn children(
    ops: &dyn ProviderOps,
    (path, at): (&[String], &Location),
    pending: &mut Pending,
    run: &mut RetentionRun<'_>,
) -> Result<(), Error> {
    let mut failure = None;
    let mut sink = |batch: EntryBatch| {
        for child in batch.entries {
            if pending.len() >= MAX_NODES || run.cancel.is_cancelled() {
                failure = Some(Error::Unsupported(at.clone()));
                return ControlFlow::Break(());
            }
            let name = child.name.to_text();
            if child.kind == EntryKind::Folder && reserved(&name) {
                failure = Some(Error::Unsupported(at.clone()));
                return ControlFlow::Break(());
            }
            let Ok(location) = at.join(&ProviderPath::from_text(&name)) else {
                failure = Some(Error::Changed(at.clone()));
                return ControlFlow::Break(());
            };
            let mut relative = path.to_vec();
            relative.push(name);
            pending.push((relative, location));
        }
        ControlFlow::Continue(())
    };
    ops.list(at, &mut sink, run.cancel)?;
    failure.map_or(Ok(()), Err)
}

pub(super) fn verify(
    ops: &dyn ProviderOps,
    record: &RetentionRecord,
    (root, missing): (&Location, bool),
    run: &mut RetentionRun<'_>,
) -> Result<(), Error> {
    if missing && proof::present(ops, root)?.is_none() {
        return Ok(());
    }
    let found = read(
        ops,
        (root, proof::scope(ops, &proof::anchor(record)?)?),
        run,
    )?;
    let expected: HashMap<_, _> = record.nodes.iter().map(|node| (&node.path, node)).collect();
    for node in &found {
        if expected.get(&node.path).copied() != Some(node) {
            return Err(Error::Changed(node.location(root)?));
        }
    }
    if !missing && found.len() != record.nodes.len() {
        return Err(Error::Changed(root.clone()));
    }
    Ok(())
}

fn retained_node(
    path: Vec<String>,
    found: &rusty_fs::FsEntry,
    identity: rusty_core::FileIdentity,
) -> RetainedNode {
    RetainedNode {
        path,
        kind: proof::kind(found.kind),
        identity,
        size: if found.kind == EntryKind::Folder {
            0
        } else {
            found.size
        },
        modified_ms: if found.kind == EntryKind::Folder {
            None
        } else {
            found.modified_ms
        },
    }
}

fn reserved(name: &str) -> bool {
    rusty_core::brand::is_retained_container(name)
}
