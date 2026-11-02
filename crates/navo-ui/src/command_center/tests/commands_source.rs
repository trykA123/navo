use std::sync::Arc;

use crate::command_center::commands_source::words;
use crate::command_center::context_name::context_name;
use crate::command_center::{
    ArgKind, Cancel, CommandsSnapshot, CommandsSource, Group, Hit, HitSink, Query, Request,
    ResultSource, Target,
};
use crate::commands::{CommandEntry, Keymap, entries};

const FILES: [&str; 3] = ["Workspace", "NavoPane", "FileList"];

fn snapshot_of(entries: &[CommandEntry], stack: &[&str]) -> Arc<CommandsSnapshot> {
    Arc::new(CommandsSnapshot::build(entries, stack, Keymap::builtin()))
}

fn run(source: &CommandsSource, input: &str, cancel: &Cancel) -> Vec<Hit> {
    let request = Request {
        query: Query::parse(input),
        generation: 1,
        here: None,
        limit: 100,
    };
    let sink = HitSink::new(1);
    source.search(&request, &sink, cancel);
    sink.drain(1)
}

fn find<'a>(hits: &'a [Hit], title: &str) -> Option<&'a Hit> {
    hits.iter().find(|hit| hit.title == title)
}

fn all_visible() -> CommandsSource {
    CommandsSource::new(snapshot_of(&entries(), &FILES))
}

#[test]
fn words_out_of_order_find_a_command() {
    let hits = run(&all_visible(), "tab new", &Cancel::new());
    assert!(find(&hits, "New tab").is_some());
    let hits = run(&all_visible(), "all mark", &Cancel::new());
    let hit = find(&hits, "Mark all").expect("Mark all matches");
    assert!(!hit.indices.is_empty());
    assert!(hit.score > 0);
}

#[test]
fn a_query_that_matches_nothing_gives_no_hits() {
    assert!(run(&all_visible(), "zzqxj", &Cancel::new()).is_empty());
}

#[test]
fn hits_are_commands_with_an_action_target() {
    let hits = run(&all_visible(), "refresh", &Cancel::new());
    let hit = find(&hits, "Refresh").expect("Refresh matches");
    assert_eq!(hit.group, Group::Commands);
    assert_eq!(hit.source, "commands");
    assert_eq!(hit.target, Target::Action("file_list::Refresh"));
}

#[test]
fn key_hints_come_from_the_given_keymap() {
    let hits = run(&all_visible(), "refresh", &Cancel::new());
    let shortcut = find(&hits, "Refresh").and_then(|hit| hit.shortcut.clone());
    assert_eq!(shortcut.as_deref(), Some("ctrl-r"));

    let keymap = Keymap::parse("[[bind]]\ncommand = \"files.refresh\"\nkeys = [\"ctrl-q\"]\n")
        .expect("a valid keymap");
    let snapshot = Arc::new(CommandsSnapshot::build(&entries(), &FILES, &keymap));
    let hits = run(&CommandsSource::new(snapshot), "refresh", &Cancel::new());
    let shortcut = find(&hits, "Refresh").and_then(|hit| hit.shortcut.clone());
    assert_eq!(shortcut.as_deref(), Some("ctrl-q"));
}

#[test]
fn commands_with_an_argument_yield_an_arg_target() {
    let mut list = entries();
    let entry = list
        .iter_mut()
        .find(|entry| entry.spec.name == "files.refresh")
        .expect("files.refresh exists");
    entry.arg = Some(ArgKind::Text);
    let source = CommandsSource::new(snapshot_of(&list, &FILES));
    let hits = run(&source, "refresh", &Cancel::new());
    let hit = find(&hits, "Refresh").expect("Refresh matches");
    assert_eq!(hit.target, Target::Arg("files.refresh", ArgKind::Text));
}

#[test]
fn disabled_and_palette_hidden_commands_are_left_out() {
    let mut list = entries();
    for entry in &mut list {
        if entry.spec.name == "files.refresh" {
            entry.enabled = false;
        }
    }
    let source = CommandsSource::new(snapshot_of(&list, &FILES));
    assert!(find(&run(&source, "refresh", &Cancel::new()), "Refresh").is_none());
    let snapshot = CommandsSnapshot::build(&entries(), &FILES, Keymap::builtin());
    assert!(
        snapshot
            .rows()
            .iter()
            .all(|row| row.name != "files.cursor_up")
    );
}

#[test]
fn viewer_commands_show_only_from_a_viewer_stack() {
    let from_files = run(&all_visible(), "go to line", &Cancel::new());
    assert!(find(&from_files, "Go to line").is_none());
    let viewer = CommandsSource::new(snapshot_of(&entries(), &["Workspace", "Pdf"]));
    assert!(find(&run(&viewer, "go to line", &Cancel::new()), "Go to line").is_some());
}

#[test]
fn file_queries_and_cancel_give_nothing() {
    assert!(run(&all_visible(), "/notes ext:rs", &Cancel::new()).is_empty());
    assert!(run(&all_visible(), "C:\\Users", &Cancel::new()).is_empty());
    let cancel = Cancel::new();
    cancel.cancel();
    assert!(run(&all_visible(), "refresh", &cancel).is_empty());
}

#[test]
fn the_source_describes_itself() {
    let source = all_visible();
    assert_eq!(source.id(), "commands");
    assert_eq!(source.group(), Group::Commands);
}

#[test]
fn a_command_used_often_scores_higher_than_the_same_unused_one() {
    let plain = run(&all_visible(), "refresh", &Cancel::new());
    let base = find(&plain, "Refresh").expect("Refresh matches").score;
    let snapshot = snapshot_of(&entries(), &FILES);
    let weights: Arc<dyn Fn() -> Vec<(String, f64)> + Send + Sync> =
        Arc::new(|| vec![("file_list::Refresh".to_owned(), 8.0)]);
    let source = CommandsSource::with_use(snapshot, weights);
    let used = run(&source, "refresh", &Cancel::new());
    let lifted = find(&used, "Refresh").expect("Refresh matches").score;
    assert!(lifted > base);
    let rivals = run(&source, "mark all", &Cancel::new());
    let unused = run(&all_visible(), "mark all", &Cancel::new());
    assert_eq!(
        find(&rivals, "Mark all").map(|hit| hit.score),
        find(&unused, "Mark all").map(|hit| hit.score)
    );
}

#[test]
fn a_command_row_names_its_context_in_plain_words() {
    let files = run(&all_visible(), "refresh", &Cancel::new());
    assert_eq!(
        find(&files, "Refresh").map(|hit| &*hit.detail),
        Some("File list")
    );
    let viewer = CommandsSource::new(snapshot_of(&entries(), &["Workspace", "Pdf"]));
    let found = run(&viewer, "go to line", &Cancel::new());
    assert_eq!(
        find(&found, "Go to line").map(|hit| &*hit.detail),
        Some("Viewer")
    );
}

#[test]
fn every_palette_command_has_a_plain_context_name() {
    let mut missing: Vec<&str> = entries()
        .iter()
        .filter(|entry| entry.in_palette)
        .map(|entry| entry.spec.context)
        .filter(|context| context_name(context).is_none())
        .collect();
    missing.sort_unstable();
    missing.dedup();
    assert!(missing.is_empty(), "no plain name for {missing:?}");
}

#[test]
fn the_shown_key_is_bound_in_the_commands_own_context() {
    let text = "[[bind]]\ncommand = \"viewer.go_to_line\"\nkeys = [\"ctrl-g\"]\ncontext = \"Paged\"\n\n\
                [[bind]]\ncommand = \"viewer.go_to_line\"\nkeys = [\"ctrl-l\"]\ncontext = \"Tree\"\n";
    let keymap = Keymap::parse(text).expect("a valid keymap");
    let stack = ["Workspace", "Tree"];
    let snapshot = Arc::new(CommandsSnapshot::build(&entries(), &stack, &keymap));
    let hits = run(&CommandsSource::new(snapshot), "go to line", &Cancel::new());
    let shortcut = find(&hits, "Go to line").and_then(|hit| hit.shortcut.clone());
    assert_eq!(shortcut.as_deref(), Some("ctrl-l"));
    let stack = ["Workspace", "Image"];
    let snapshot = Arc::new(CommandsSnapshot::build(&entries(), &stack, &keymap));
    let hits = run(&CommandsSource::new(snapshot), "go to line", &Cancel::new());
    let shortcut = find(&hits, "Go to line").and_then(|hit| hit.shortcut.clone());
    assert_eq!(shortcut, None);
}

#[test]
fn a_slash_in_a_commands_query_is_plain_name_text() {
    let words = |input: &str| {
        words(&Request {
            query: Query::parse(input),
            generation: 1,
            here: None,
            limit: 100,
        })
    };
    assert_eq!(words(">save/as"), Some("save/as".to_owned()));
    assert_eq!(words(">open src\\main"), Some("open src\\main".to_owned()));
    assert_eq!(words("save/as"), None);
    assert_eq!(words(">save ext:rs"), None);
}

#[test]
fn use_weights_load_once_per_source() {
    let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = loads.clone();
    let weights: Arc<dyn Fn() -> Vec<(String, f64)> + Send + Sync> = Arc::new(move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Vec::new()
    });
    let source = CommandsSource::with_use(snapshot_of(&entries(), &FILES), weights);
    run(&source, "r", &Cancel::new());
    run(&source, "re", &Cancel::new());
    run(&source, "ref", &Cancel::new());
    assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 1);
}
