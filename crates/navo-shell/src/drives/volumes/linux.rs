use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::os::fd::OwnedFd;
use std::thread::JoinHandle;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use rustix::pipe::{PipeFlags, pipe_with};

pub struct VolumeWatch {
    stop: Option<OwnedFd>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for VolumeWatch {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("mount watcher panicked");
        }
    }
}

fn acknowledge(mounts: &mut File) -> io::Result<()> {
    mounts.seek(SeekFrom::Start(0))?;
    io::copy(mounts, &mut io::sink())?;
    Ok(())
}

fn changed(mounts: &File, stop: &OwnedFd) -> rustix::io::Result<bool> {
    let mut descriptors = [
        PollFd::new(mounts, PollFlags::PRI | PollFlags::ERR),
        PollFd::new(stop, PollFlags::IN),
    ];
    loop {
        match poll(&mut descriptors, None) {
            Err(Errno::INTR) => continue,
            result => {
                result?;
            }
        }
        if !descriptors[1].revents().is_empty() {
            return Ok(false);
        }
        let events = descriptors[0].revents();
        if events.contains(PollFlags::NVAL) {
            return Err(Errno::BADF);
        }
        return Ok(events.intersects(PollFlags::PRI | PollFlags::ERR));
    }
}

fn follow(mut mounts: File, stop: &OwnedFd, on_change: impl Fn()) {
    loop {
        match changed(&mounts, stop) {
            Ok(false) => return,
            Ok(true) => {}
            Err(error) => {
                tracing::warn!(%error, "waiting for mount changes failed");
                return;
            }
        }
        if let Err(error) = acknowledge(&mut mounts) {
            tracing::warn!(%error, "reading changed mount table failed");
            return;
        }
        on_change();
    }
}

#[must_use]
pub fn watch_volumes(on_change: impl Fn() + Send + 'static) -> Option<VolumeWatch> {
    let mut mounts = File::open("/proc/self/mountinfo")
        .inspect_err(|error| tracing::warn!(%error, "opening mount table failed"))
        .ok()?;
    acknowledge(&mut mounts)
        .inspect_err(|error| tracing::warn!(%error, "reading mount table failed"))
        .ok()?;
    let (read, write) = pipe_with(PipeFlags::CLOEXEC | PipeFlags::NONBLOCK)
        .inspect_err(|error| tracing::warn!(%error, "creating mount stop pipe failed"))
        .ok()?;
    let thread = std::thread::Builder::new()
        .name("mount-watch".to_owned())
        .spawn(move || follow(mounts, &read, on_change))
        .inspect_err(|error| tracing::warn!(%error, "starting mount watcher failed"))
        .ok()?;
    Some(VolumeWatch {
        stop: Some(write),
        thread: Some(thread),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_the_stop_pipe_wakes_the_mount_waiter() {
        let mounts = File::open("/proc/self/mountinfo").expect("mount table");
        let (read, write) = pipe_with(PipeFlags::CLOEXEC).expect("stop pipe");
        drop(write);
        assert!(!changed(&mounts, &read).expect("poll mount table and stop pipe"));
    }

    #[test]
    fn dropping_a_mount_watcher_joins_its_thread() {
        let watch = watch_volumes(|| {}).expect("mount watcher");
        drop(watch);
    }
}
