use super::*;

pub(super) const PUBLISHES_PER_SCAN: usize = 2;

pub(super) struct FakeProbe;

impl Probe for FakeProbe {
    fn has_git(&self, folder: &Path) -> bool {
        folder.ends_with("repo")
    }

    fn drive_class(&self, _: &Location) -> DriveClass {
        DriveClass::Removable
    }
}

#[derive(Default)]
pub(super) struct FakeSource {
    pub(super) calls: AtomicUsize,
    pub(super) reply: Mutex<Option<Result<RepoStatus, ScanError>>>,
}

impl StatusSource for FakeSource {
    fn status(&self, _: &Path, _: &Cancel) -> Result<RepoStatus, ScanError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.reply
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Ok(clean()))
    }
}

pub(super) fn clean() -> RepoStatus {
    parse_porcelain_v2(b"# branch.oid (initial)\0# branch.head main\0", false).unwrap()
}

pub(super) fn dirty() -> RepoStatus {
    parse_porcelain_v2(
        b"# branch.oid (initial)\0# branch.head main\0? file\0",
        false,
    )
    .unwrap()
}

pub(super) struct Rig {
    pub(super) hooks: Hooks,
    pub(super) source: Arc<FakeSource>,
    pub(super) starts: Arc<Mutex<Vec<GitEngine>>>,
    pub(super) now: Arc<Mutex<Instant>>,
    pub(super) published: Arc<AtomicUsize>,
    pub(super) folder: Location,
}

pub(super) fn settings(badges: bool) -> GitSettings {
    GitSettings {
        badges,
        ..GitSettings::default()
    }
}

pub(super) fn rig(initial: &GitSettings) -> Rig {
    rig_with(
        initial,
        Tools::new(
            Arc::new(FakeTools::default()),
            FakeSync::replying(Ok(pulled())),
        ),
    )
}

pub(super) fn rig_with(initial: &GitSettings, tools: Tools) -> Rig {
    let source = Arc::new(FakeSource::default());
    let starts = Arc::new(Mutex::new(Vec::new()));
    let now = Arc::new(Mutex::new(Instant::now()));
    let factory: Factory = {
        let (source, starts) = (source.clone(), starts.clone());
        Box::new(move |settings| {
            starts.lock().unwrap().push(settings.engine);
            Ports {
                source: source.clone(),
                probe: Arc::new(FakeProbe),
                saved: None,
            }
        })
    };
    let clock: Clock = {
        let now = now.clone();
        Box::new(move || *now.lock().unwrap())
    };
    let hooks = Hooks::new(initial, factory, clock, tools);
    let published = Arc::new(AtomicUsize::new(0));
    let counter = published.clone();
    hooks.subscribe(Box::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    Rig {
        hooks,
        source,
        starts,
        now,
        published,
        folder: Location::new("fixture").unwrap(),
    }
}

impl Rig {
    pub(super) fn observe(&self) {
        self.hooks.observe(&self.folder, 1, vec!["repo".to_owned()]);
    }

    pub(super) fn wait_for(&self, ready: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready(self) {
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn wait_published(&self) {
        self.wait_for(|rig| {
            let published = rig.published.load(Ordering::SeqCst);
            rig.calls() == 1 && rig.fresh_kind().is_some() && published >= PUBLISHES_PER_SCAN
        });
    }

    pub(super) fn stays_at(&self, calls: usize) {
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(self.calls(), calls);
    }

    pub(super) fn calls(&self) -> usize {
        self.source.calls.load(Ordering::SeqCst)
    }

    pub(super) fn fresh_kind(&self) -> Option<BadgeKind> {
        let badges = self.hooks.badges(&self.folder)?;
        let badge = badges.by_name.get("repo")?;
        (!badge.stale).then_some(badge.kind)
    }

    pub(super) fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }
}
