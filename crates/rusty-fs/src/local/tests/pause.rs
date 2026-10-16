use std::sync::mpsc::{Sender, channel};
use std::thread::scope;

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, plan};

use super::*;
use crate::job::{JobContext, JobLog, JobOutcome, Progress, StepOutcome, run_job};
use crate::{TEMP_SUFFIX, scan};

const MIB: u64 = 1 << 20;

struct Quiet;

impl JobLog for Quiet {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }
}

struct Stop(CancelToken);

impl Drop for Stop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

fn copy_into_dst(ops: &LocalFs, root: &Location, name: &str) -> OperationPlan {
    let source = root.join(&ProviderPath::from_text(name)).unwrap();
    let items = scan(ops, &[source], &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(root.join(&ProviderPath::from_text("dst")).unwrap()),
        existing: Vec::new(),
        policy: ConflictPolicy::Skip,
        same_volume: true,
        protected: Vec::new(),
    })
}

fn run_pausing_at(
    ops: &LocalFs,
    plan: &OperationPlan,
    cancel: &CancelToken,
    sender: &Sender<Progress>,
) -> JobOutcome {
    let mut is_asked = false;
    let mut on_progress = |progress: &Progress| {
        if progress.bytes_done == MIB && !is_asked {
            is_asked = true;
            cancel.pause();
        }
        let _ = sender.send(progress.clone());
    };
    run_job(
        plan,
        &[],
        &mut JobContext {
            ops,
            log: &mut Quiet,
            cancel,
            progress: &mut on_progress,
        },
    )
}

#[test]
fn a_real_copy_paused_mid_file_resumes_to_identical_bytes() {
    let (dir, root) = temp();
    let data: Vec<u8> = (0..3 * MIB + 77).map(|i| (i % 251) as u8).collect();
    fs::create_dir(dir.path().join("dst")).unwrap();
    fs::write(dir.path().join("big.bin"), &data).unwrap();
    let ops = LocalFs::new();
    let plan = copy_into_dst(&ops, &root, "big.bin");
    let part = dir.path().join(format!("dst/big.bin{TEMP_SUFFIX}"));
    let token = CancelToken::new();
    let (sender, seen) = channel();
    let outcome = scope(|s| {
        let _stop = Stop(token.clone());
        let (ops, plan, cancel) = (&ops, &plan, token.clone());
        let job = s.spawn(move || run_pausing_at(ops, plan, &cancel, &sender));
        let paused = seen.iter().find(|progress| progress.paused);
        let held = (
            paused.map(|p| p.bytes_done),
            fs::metadata(&part).map(|m| m.len()).ok(),
            dir.path().join("dst/big.bin").exists(),
        );
        assert_eq!(held, (Some(MIB), Some(MIB), false));
        token.resume();
        job.join().expect("job thread")
    });
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(fs::read(dir.path().join("dst/big.bin")).unwrap(), data);
    assert!(!part.exists());
}
