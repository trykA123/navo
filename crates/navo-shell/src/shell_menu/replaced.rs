use std::thread::JoinHandle;

use super::WorkerHealth;

/// Shell threads replaced because they hung; reading ahead waits until every one of them has ended.
#[derive(Default)]
pub(super) struct Replaced {
    running: Vec<JoinHandle<()>>,
    count: u64,
}

impl Replaced {
    pub(super) fn add(&mut self, thread: JoinHandle<()>) {
        self.running.push(thread);
        self.count += 1;
    }

    pub(super) fn health(&mut self) -> WorkerHealth {
        self.running.retain(|thread| !thread.is_finished());
        WorkerHealth {
            replaced: self.count,
            can_read_ahead: self.running.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::Replaced;
    use crate::shell_menu::WorkerHealth;

    #[test]
    fn a_new_service_can_read_ahead() {
        let health = Replaced::default().health();
        assert_eq!(
            health,
            WorkerHealth {
                replaced: 0,
                can_read_ahead: true
            }
        );
    }

    #[test]
    fn reading_ahead_waits_until_a_replaced_thread_ends() {
        let (release, wait) = mpsc::channel::<()>();
        let mut replaced = Replaced::default();
        let thread = std::thread::spawn(move || {
            let _ = wait.recv();
        });
        replaced.add(thread);
        assert!(!replaced.health().can_read_ahead);
        drop(release);
        while !replaced.health().can_read_ahead {
            std::thread::yield_now();
        }
        assert_eq!(replaced.health().replaced, 1);
    }
}
