use super::*;

pub(super) fn gated(browser: &mut Browser) -> Arc<AtomicBool> {
    let gate = Arc::new(AtomicBool::new(false));
    let open = Arc::clone(&gate);
    browser.set_operations(Operations {
        log: Arc::new(move |_: &OperationPlan| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !open.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Box::new(SharedLog(Arc::default())) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
    });
    gate
}

fn paused_copy(fs: &Arc<FakeFs>) -> (Browser, u64) {
    let mut browser = browser(fs, home_beside_dest(fs));
    let gate = gated(&mut browser);
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    let id = browser.jobs()[0].id;
    assert!(browser.pause_job(id));
    gate.store(true, Ordering::Release);
    assert!(engine_paused(&mut browser), "the engine reports the pause");
    (browser, id)
}

fn engine_paused(browser: &mut Browser) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        browser.poll();
        if browser.jobs()[0].progress.paused {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    false
}

#[test]
fn pause_job_holds_only_the_named_job() {
    let fs = fake();
    let (mut browser, id) = paused_copy(&fs);
    assert!(!browser.pause_job(id + 1));
    assert!(!exists(&fs, "dest/docs"));
    assert!(browser.running());
    browser.cancel_job(id);
    settle(&mut browser);
}

#[test]
fn resume_job_finishes_the_copy() {
    let fs = fake();
    let (mut browser, id) = paused_copy(&fs);
    assert!(browser.resume_job(id));
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert!(!browser.jobs()[0].progress.paused);
    assert!(exists(&fs, "dest/docs/report.txt"));
}

#[test]
fn cancel_while_paused_ends_the_job_cancelled() {
    let fs = fake();
    let (mut browser, id) = paused_copy(&fs);
    assert!(browser.cancel_job(id));
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Cancelled);
    assert!(!exists(&fs, "dest/docs"));
}

#[test]
fn shutdown_ends_a_paused_job() {
    let fs = fake();
    let (mut browser, _) = paused_copy(&fs);
    assert!(browser.shutdown(Duration::from_secs(5)));
    assert_eq!(browser.jobs()[0].status, JobStatus::Cancelled);
}

#[test]
fn an_ended_job_cannot_be_paused_or_resumed() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    let id = browser.jobs()[0].id;
    assert_eq!(
        (browser.pause_job(id), browser.resume_job(id)),
        (false, false)
    );
}
