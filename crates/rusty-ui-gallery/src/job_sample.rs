use rusty_core::OpKind;
use rusty_ui::job_card::JobInfo;
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
    }
}

pub fn jobs(count: usize) -> Vec<JobInfo> {
    [copy(), move_to_stick()].into_iter().take(count).collect()
}
