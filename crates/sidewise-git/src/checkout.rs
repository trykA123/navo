//! The checkout converter of one Git side: turns a stored blob into the bytes `git checkout`
//! would write, with every filter driver removed, so no program ever runs.

use std::panic::{AssertUnwindSafe, catch_unwind};

use gix::attrs::StateRef;
use gix::bstr::BStr;
use gix::filter::plumbing::driver::apply::Delay;
use gix::filter::plumbing::eol::AutoCrlf;
use gix::filter::plumbing::pipeline::convert::{ToWorktreeOutcome, to_worktree};
use sidewise_engine::model::{EntryKind, Reason};

use crate::checkout_config;
use crate::index::Child;
use crate::repo::{GitProblem, MAX_BLOB};

/// Largest stored blob that an `ident` or `working-tree-encoding` file is converted from (1 MiB).
pub const MAX_EXPAND_INPUT: u64 = 1024 * 1024;
/// The attributes that decide a checkout conversion; `filter` is left out: no driver runs.
const SELECTED: [&str; 5] = ["crlf", "ident", "eol", "text", "working-tree-encoding"];

/// The conversion state of one side: the filter pipeline, its attribute stack and a reused
/// attribute outcome. Lives in a `Mutex`, so it must stay `Send`.
pub struct Checkout {
    pipeline: gix::filter::plumbing::Pipeline,
    stack: gix::worktree::Stack,
    attrs: gix::attrs::search::Outcome,
    /// `core.autocrlf` resolved to `Enabled`: any text file may change.
    auto_crlf: bool,
}

/// What a file needs to know its checkout size.
enum Plan {
    /// No conversion applies: the stored size is the checkout size.
    Plain,
    /// Line endings may change: read and convert.
    Eol,
    /// `ident` or `working-tree-encoding` applies: read and convert, under a lower input cap.
    Expand,
}

/// The plan for one file from its matched attributes: `Expand` for a set `ident` or a
/// `working-tree-encoding` value, `Eol` for any other specified attribute or `autocrlf`.
fn plan_from(attrs: &gix::attrs::search::Outcome, auto_crlf: bool) -> Plan {
    let mut any = false;
    for found in attrs.iter_selected() {
        let name = found.assignment.name.as_str();
        match found.assignment.state {
            StateRef::Set if name == "ident" => return Plan::Expand,
            StateRef::Value(_) if name == "working-tree-encoding" => return Plan::Expand,
            StateRef::Unspecified => {}
            _ => any = true,
        }
    }
    if any || auto_crlf {
        Plan::Eol
    } else {
        Plan::Plain
    }
}

impl Checkout {
    /// Builds the converter for `tree` with drivers cleared and the eol settings of `repo`, else
    /// of `system`. A network `core.attributesFile` is `Unsupported`; a gix error or panic is
    /// `Broken`. Attributes come from `tree` only (an index made from it), never the working tree.
    pub fn open(
        repo: &gix::Repository,
        tree: gix::ObjectId,
        system: Option<&gix::config::File>,
    ) -> Result<Checkout, GitProblem> {
        use gix::worktree::stack::state::attributes::Source;
        checkout_config::check_attributes_file(repo)?;
        catch_unwind(AssertUnwindSafe(|| {
            let index = repo
                .index_from_tree(&tree)
                .map_err(|_| GitProblem::Broken)?;
            let stack = repo
                .attributes_only(&index, Source::IdMapping)
                .map_err(|_| GitProblem::Broken)?
                .detach();
            let pipeline =
                gix::filter::Pipeline::new(repo, stack).map_err(|_| GitProblem::Broken)?;
            let (mut pipeline, stack) = pipeline.into_parts();
            let options = pipeline.options_mut();
            options.drivers.clear();
            options.eol_config = checkout_config::eol_settings(repo, system)?;
            let auto_crlf = options.eol_config.auto_crlf == AutoCrlf::Enabled;
            let mut attrs = gix::attrs::search::Outcome::default();
            attrs.initialize_with_selection(stack.attributes_collection(), SELECTED);
            Ok(Checkout {
                pipeline,
                stack,
                attrs,
                auto_crlf,
            })
        }))
        .unwrap_or(Err(GitProblem::Broken))
    }

    /// Decides from the attributes at `rel` whether the file must be read. A stack error or
    /// a panic while reading `.gitattributes` is `ReadError`.
    fn plan(&mut self, repo: &gix::Repository, rel: &str) -> Result<Plan, Reason> {
        let Self {
            stack,
            attrs,
            auto_crlf,
            ..
        } = self;
        catch_unwind(AssertUnwindSafe(|| {
            let entry = stack
                .at_entry(BStr::new(rel), None, &repo.objects)
                .map_err(|_| Reason::ReadError)?;
            attrs.reset();
            entry.matching_attributes(attrs);
            Ok(plan_from(attrs, *auto_crlf))
        }))
        .unwrap_or(Err(Reason::ReadError))
    }

    /// The checkout bytes of the blob `data` stored at `rel`. A stack error is `ReadError`, as
    /// is a driver outcome (never expected: drivers are cleared); a filter error or a panic is
    /// `Unsupported`.
    pub fn convert(
        &mut self,
        repo: &gix::Repository,
        rel: &str,
        data: &[u8],
    ) -> Result<Vec<u8>, Reason> {
        let Self {
            pipeline, stack, ..
        } = self;
        catch_unwind(AssertUnwindSafe(|| {
            let entry = stack
                .at_entry(BStr::new(rel), None, &repo.objects)
                .map_err(|_| Reason::ReadError)?;
            let options = to_worktree::Options {
                can_delay: Delay::Forbid,
                unknown_encoding: to_worktree::UnknownEncoding::Fail,
            };
            let mut attributes = |_: &BStr, out: &mut gix::attrs::search::Outcome| {
                entry.matching_attributes(out);
            };
            match pipeline.convert_to_worktree(data, BStr::new(rel), &mut attributes, options) {
                Ok(ToWorktreeOutcome::Unchanged(bytes) | ToWorktreeOutcome::Buffer(bytes)) => {
                    Ok(bytes.to_vec())
                }
                Ok(ToWorktreeOutcome::Process(_)) => Err(Reason::ReadError),
                Err(_) => Err(Reason::Unsupported),
            }
        }))
        .unwrap_or(Err(Reason::Unsupported))
    }

    /// Sets `child.size` to its checkout size, or `child.readable` to false when it cannot be
    /// known within the caps. Reads the blob through `read` only when a conversion applies; a
    /// read error leaves the child as stored, so opening it reports the error later.
    #[expect(
        clippy::too_many_arguments,
        reason = "the G17 plan fixes this signature; the index calls it once per file"
    )]
    pub fn fix_child(
        &mut self,
        repo: &gix::Repository,
        rel: &str,
        child: &mut Child,
        read: &mut dyn FnMut(gix::ObjectId) -> Result<Vec<u8>, Reason>,
    ) {
        if child.kind != EntryKind::File || !child.readable || child.size > MAX_BLOB {
            return;
        }
        match self.plan(repo, rel) {
            Err(_) => child.readable = false,
            Ok(Plan::Plain) => {}
            Ok(Plan::Expand) if child.size > MAX_EXPAND_INPUT => child.readable = false,
            Ok(Plan::Eol | Plan::Expand) => {
                let Ok(data) = read(child.id) else { return };
                match self.convert(repo, rel, &data) {
                    Ok(out) if out.len() as u64 <= MAX_BLOB => child.size = out.len() as u64,
                    _ => child.readable = false,
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "checkout_tests.rs"]
mod tests;
