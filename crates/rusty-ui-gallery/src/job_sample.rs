use rusty_core::OpKind;
use rusty_ui::job_card::{JobInfo, Pause, RenameNote};
use rusty_ui::job_row::JobRowState;

const MB: u64 = 1024 * 1024;

fn copy() -> JobInfo {
    JobInfo {
        id: 1,
        kind: OpKind::Copy,
        items: 4,
        from: Some("commander".into()),
        target: Some(r"Z:\archive\2026-10".into()),
        current: Some("Sinaia-2026-10.7z".into()),
        bytes_done: 1150 * MB,
        bytes_total: 2806 * MB,
        files_done: 14,
        files_total: 37,
        state: JobRowState::Running,
        speed: Some(112 * MB),
        pause: Pause::Off,
        rename: RenameNote::default(),
    }
}

fn move_to_stick() -> JobInfo {
    JobInfo {
        id: 2,
        kind: OpKind::Move,
        items: 1,
        from: Some("Downloads".into()),
        target: Some(r"E:\".into()),
        current: Some("Windows11_25H2_x64.iso".into()),
        bytes_done: 997 * MB,
        bytes_total: 5540 * MB,
        files_done: 0,
        files_total: 1,
        state: JobRowState::Running,
        speed: Some(38 * MB),
        pause: Pause::Off,
        rename: RenameNote::default(),
    }
}

fn failed_copy() -> JobInfo {
    JobInfo {
        id: 3,
        files_done: 35,
        state: JobRowState::Failed("Access denied".into()),
        speed: None,
        ..copy()
    }
}

fn done_move() -> JobInfo {
    JobInfo {
        id: 4,
        items: 3,
        target: Some(r"C:\work\Projects".into()),
        bytes_done: 2806 * MB,
        files_done: 37,
        state: JobRowState::Done,
        ..move_to_stick()
    }
}

fn paused_copy() -> JobInfo {
    JobInfo {
        id: 5,
        target: Some(r"D:\photos".into()),
        bytes_done: 310 * MB,
        files_done: 120,
        files_total: 480,
        speed: None,
        pause: Pause::On,
        ..copy()
    }
}

fn recycle() -> JobInfo {
    JobInfo {
        id: 6,
        kind: OpKind::Recycle,
        items: 9,
        target: None,
        bytes_done: 40 * MB,
        bytes_total: 90 * MB,
        files_done: 4,
        files_total: 9,
        speed: Some(20 * MB),
        ..copy()
    }
}

pub fn jobs(count: usize) -> Vec<JobInfo> {
    [
        copy(),
        move_to_stick(),
        failed_copy(),
        done_move(),
        paused_copy(),
        recycle(),
    ]
    .into_iter()
    .take(count)
    .collect()
}
