use super::*;

#[derive(Default)]
pub(super) struct FakeTools {
    pub(super) found: Vec<DetectedTool>,
    pub(super) launches: Mutex<Vec<Launch>>,
}

impl ToolPort for FakeTools {
    fn detect(&self) -> Vec<DetectedTool> {
        self.found.clone()
    }

    fn start(
        &self,
        launch: Launch,
        _: &LaunchPorts,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> Result<(), LaunchError> {
        self.launches.lock().unwrap().push(launch);
        on_exit();
        Ok(())
    }
}

pub(super) struct FakeSync {
    pub(super) reply: Result<SyncSummary, SyncFailure>,
    /// The HEAD the next confirmations name, when set.
    pub(super) head: Mutex<Option<String>>,
    /// Confirmations the next reads return, first one first, before the default one.
    pub(super) queued: Mutex<Vec<Confirmation>>,
    pub(super) confirms: Mutex<Vec<SyncKind>>,
    pub(super) runs: Mutex<Vec<(PathBuf, Confirmation)>>,
    /// When set, a run reports on it and then waits to be cancelled.
    pub(super) hold: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl FakeSync {
    pub(super) fn replying(reply: Result<SyncSummary, SyncFailure>) -> Arc<Self> {
        Arc::new(Self {
            reply,
            head: Mutex::default(),
            queued: Mutex::default(),
            confirms: Mutex::default(),
            runs: Mutex::default(),
            hold: Mutex::default(),
        })
    }
}

impl SyncPort for FakeSync {
    fn confirm(&self, _: &Path, kind: SyncKind) -> Result<Confirmation, SyncFailure> {
        self.confirms.lock().unwrap().push(kind);
        let mut queued = self.queued.lock().unwrap();
        if let Some(at) = queued.iter().position(|held| held.kind == kind) {
            return Ok(queued.remove(at));
        }
        let mut confirmed = confirmation(kind);
        if let Some(head) = self.head.lock().unwrap().clone() {
            confirmed.head = Some(head);
        }
        Ok(confirmed)
    }

    fn run(
        &self,
        repo: &Path,
        confirmed: &Confirmation,
        cancel: &RunCancel,
    ) -> Result<SyncSummary, SyncFailure> {
        self.runs
            .lock()
            .unwrap()
            .push((repo.to_path_buf(), confirmed.clone()));
        if let Some(entered) = self.hold.lock().unwrap().clone() {
            entered.send(()).unwrap();
            while !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            return Err(SyncFailure::Cancelled);
        }
        self.reply.clone()
    }
}

pub(super) fn pulled() -> SyncSummary {
    SyncSummary {
        kind: SyncKind::Pull,
        branch: "main".into(),
        remote: "origin".into(),
        commits: 2,
        tracking_stale: false,
    }
}

pub(super) fn found(kind: ToolKind) -> DetectedTool {
    DetectedTool {
        kind,
        label: kind.label(),
        exe: PathBuf::from("tool"),
    }
}

pub(super) struct Wired {
    pub(super) rig: Rig,
    pub(super) tools: Arc<FakeTools>,
    pub(super) sync: Arc<FakeSync>,
    pub(super) repo: Location,
}

pub(super) fn wired_with(
    setting: &str,
    kinds: &[ToolKind],
    reply: Result<SyncSummary, SyncFailure>,
) -> Wired {
    let tools = Arc::new(FakeTools {
        found: kinds.iter().map(|kind| found(*kind)).collect(),
        ..FakeTools::default()
    });
    let sync = FakeSync::replying(reply);
    let settings = GitSettings {
        tool: setting.to_owned(),
        tool_command: Some("custom {path}".to_owned()),
        ..GitSettings::default()
    };
    let rig = rig_with(&settings, Tools::new(tools.clone(), sync.clone()));
    drop(rig.hooks.tools.detected());
    rig.observe();
    rig.wait_for(|rig| rig.calls() == 1 && rig.fresh_kind().is_some());
    let repo = Location::new("fixture/repo").unwrap();
    Wired {
        rig,
        tools,
        sync,
        repo,
    }
}

pub(super) fn wired(setting: &str, kinds: &[ToolKind]) -> Wired {
    wired_with(setting, kinds, Ok(pulled()))
}

impl Wired {
    pub(super) fn actions(&self) -> Vec<GitAction> {
        let menu = self.rig.hooks.menu(&self.repo);
        menu.items.iter().map(|item| item.action).collect()
    }

    pub(super) fn item(&self, action: GitAction) -> GitMenuItem {
        let menu = self.rig.hooks.menu(&self.repo);
        menu.items
            .into_iter()
            .find(|item| item.action == action)
            .unwrap()
    }

    pub(super) fn previews(&self) -> Previewed {
        let (sender, receiver) = std::sync::mpsc::channel();
        let done = Box::new(move |replies| drop(sender.send(replies)));
        self.rig.hooks.previews(&self.repo, done);
        receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("the previews reply")
    }

    pub(super) fn preview(&self, action: GitAction) -> Option<Shown> {
        let replies = self.previews();
        replies.into_iter().find(|(held, _)| *held == action)?.1
    }

    pub(super) fn preview_text(&self, action: GitAction) -> Option<String> {
        self.preview(action).map(|shown| shown.text)
    }

    /// Runs `action` the way the menu row that showed `shown` does.
    pub(super) fn run_shown(
        &self,
        action: GitAction,
        shown: Option<&Shown>,
    ) -> Result<GitStarted, String> {
        let text = shown.map(|shown| shown.text.as_str());
        let id = shown.map(|shown| shown.id.as_str());
        self.rig.hooks.run_confirmed(&self.repo, action, text, id)
    }

    pub(super) fn settled_rescans(&self, expected: usize) {
        self.rig.wait_for(|rig| rig.calls() == 1 + expected);
        self.rig.stays_at(1 + expected);
    }
}
