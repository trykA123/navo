use std::process::{Child, Command, Stdio};

use super::*;

const LIMITS: HelperLimits = HelperLimits {
    memory_bytes: 256 << 20,
};

fn waiting_child() -> Child {
    Command::new("cmd")
        .args(["/c", "pause"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn a_confined_child_is_in_the_job_with_its_memory_limit() {
    let mut child = waiting_child();
    let job = confine_helper(&child, LIMITS).unwrap();
    assert!(job.contains(&child).unwrap());
    assert_eq!(job.memory_limit().unwrap(), LIMITS.memory_bytes);
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn closing_the_job_kills_the_child() {
    let mut child = waiting_child();
    let job = confine_helper(&child, LIMITS).unwrap();
    drop(job);
    let status = child.wait().unwrap();
    assert!(!status.success());
}
