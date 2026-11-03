use super::*;

#[test]
fn each_tool_gives_its_d100h_items() {
    use GitAction::{Commit, Diff, Log, Pull, Push};
    let full = [Commit, Pull, Push, Log, Diff, GitAction::ChooseTool];
    for (setting, kind) in [
        ("tortoisegit", ToolKind::TortoiseGit),
        ("custom", ToolKind::Custom),
    ] {
        assert_eq!(wired(setting, &[kind]).actions(), full, "{setting}");
    }
    let cli = wired("git-cli", &[ToolKind::GitCli]);
    assert_eq!(
        cli.actions(),
        [Commit, Pull, Push, Log, GitAction::ChooseTool]
    );
    assert!(!cli.rig.hooks.can_diff(&cli.repo));
}

#[test]
fn an_open_only_tool_shows_just_open_in_its_name() {
    let fork = wired("fork", &[ToolKind::Fork]);
    let menu = fork.rig.hooks.menu(&fork.repo);
    assert_eq!(menu.items.len(), 1);
    assert_eq!(menu.items[0].action, GitAction::Open);
    assert!(menu.items[0].label.contains("Fork"));
    assert!(!fork.rig.hooks.can_diff(&fork.repo));
}

#[test]
fn no_tool_gives_choose_tool_only() {
    for setting in ["none", "auto", "tortoisegit"] {
        let none = wired(setting, &[]);
        assert_eq!(none.actions(), [GitAction::ChooseTool], "{setting}");
        assert!(!none.rig.hooks.can_diff(&none.repo));
        assert!(none.rig.hooks.tools().iter().all(|row| !row.selected));
    }
}

#[test]
fn tools_lists_the_found_tools_and_marks_the_chosen_one() {
    let wired = wired("auto", &[ToolKind::Fork, ToolKind::GitCli]);
    let rows = wired.rig.hooks.tools();
    let selected: Vec<_> = rows
        .iter()
        .filter(|row| row.selected)
        .map(|row| row.id.as_str())
        .collect();
    assert_eq!(selected, ["fork"]);
    assert_eq!(rows.len(), 2);
}

#[test]
fn a_template_without_a_path_is_refused_and_a_whole_one_passes() {
    let rig = rig(&settings(true));
    assert!(rig.hooks.check_command("tool {path}").is_ok());
    assert!(rig.hooks.check_command("tool {action}").is_err());
    assert!(rig.hooks.check_command("   ").is_err());
}

struct Changing {
    rounds: Mutex<Vec<Vec<DetectedTool>>>,
}

impl ToolPort for Changing {
    fn detect(&self) -> Vec<DetectedTool> {
        self.rounds.lock().unwrap().remove(0)
    }

    fn start(
        &self,
        _: Launch,
        _: &LaunchPorts,
        _: Box<dyn FnOnce() + Send>,
    ) -> Result<(), LaunchError> {
        panic!("a refresh must not start a tool");
    }
}

#[test]
fn refresh_tools_detects_again_off_the_calling_thread_and_notifies() {
    let port = Arc::new(Changing {
        rounds: Mutex::new(vec![vec![], vec![found(ToolKind::Fork)]]),
    });
    let rig = rig_with(
        &settings(true),
        Tools::new(port, FakeSync::replying(Ok(pulled()))),
    );
    drop(rig.hooks.tools.detected());
    assert!(rig.hooks.tools().is_empty());
    rig.hooks.refresh_tools();
    rig.wait_for(|rig| rig.published.load(Ordering::SeqCst) >= 1);
    let rows = rig.hooks.tools();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].id.as_str(), rows[0].path.as_str()),
        ("fork", "tool")
    );
}
