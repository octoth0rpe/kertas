//! Notifications when a file changes, using macOS kqueue.
//!
//! Editors often save by writing a new file and renaming it over the old
//! one, so this watches both the file and its directory, and starts again
//! after every change to follow the file that's there now.

use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc;

/// The user event that stops the watching thread.
const STOP: usize = 1;

/// Watches a file until dropped.
pub struct Watch {
    changes: mpsc::UnboundedReceiver<()>,
    /// The thread's kqueue, shared so dropping this can wake the thread
    /// to stop it, even while the thread is still using it.
    queue: Option<Arc<OwnedFd>>,
}

impl Watch {
    /// Waits until the file may have changed. Resolves to false if
    /// watching has stopped (e.g. kqueue failed), and then never again.
    pub async fn changed(&mut self) -> bool {
        self.changes.next().await.is_some()
    }

    /// Whether a change has been reported and not yet waited for,
    /// consuming it.
    pub fn take_pending(&mut self) -> bool {
        let mut any = false;
        while self.changes.try_recv().is_ok() {
            any = true;
        }
        any
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(queue) = &self.queue {
            let trigger = libc::kevent {
                ident: STOP,
                filter: libc::EVFILT_USER,
                flags: 0,
                fflags: libc::NOTE_TRIGGER,
                data: 0,
                udata: std::ptr::null_mut(),
            };
            // SAFETY: the queue is open while `queue` is held, and
            // `trigger` is valid for the one change given.
            unsafe {
                libc::kevent(
                    queue.as_raw_fd(),
                    &trigger,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            };
        }
    }
}

/// Reports whenever the file at `path` may have changed: written,
/// replaced, created or deleted. Its directory must exist. Watching stops
/// when the [`Watch`] is dropped.
pub fn watch(path: PathBuf) -> Watch {
    let (changed, changes) = mpsc::unbounded();
    let queue = stop_queue().map(Arc::new);
    if let Some(queue) = queue.clone() {
        std::thread::Builder::new()
            .name("kami-file-watch".into())
            .spawn(move || watch_until_stopped(&path, queue.as_raw_fd(), &changed))
            .ok();
    }
    Watch { changes, queue }
}

/// A new kqueue with the [`STOP`] user event added.
fn stop_queue() -> Option<OwnedFd> {
    // SAFETY: kqueue takes no arguments; a negative result is an error.
    // Child processes (e.g. osascript) don't inherit kqueues.
    let fd = unsafe { libc::kqueue() };
    if fd < 0 {
        return None;
    }
    // SAFETY: `fd` is a new descriptor that nothing else owns.
    let queue = unsafe { OwnedFd::from_raw_fd(fd) };
    let stop = libc::kevent {
        ident: STOP,
        filter: libc::EVFILT_USER,
        flags: libc::EV_ADD | libc::EV_CLEAR,
        fflags: 0,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    // SAFETY: `stop` is valid for the one change given.
    let added = unsafe {
        libc::kevent(
            queue.as_raw_fd(),
            &stop,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };
    (added == 0).then_some(queue)
}

/// Sends on `changed` after each change to `path`, until the [`STOP`]
/// event arrives or the receiver is gone.
fn watch_until_stopped(path: &Path, queue: i32, changed: &mpsc::UnboundedSender<()>) {
    let directory = path.parent().unwrap_or(Path::new("/")).to_owned();
    loop {
        let watched = [
            (open_for_events(&directory), libc::NOTE_WRITE),
            (
                open_for_events(path),
                libc::NOTE_WRITE
                    | libc::NOTE_EXTEND
                    | libc::NOTE_ATTRIB
                    | libc::NOTE_DELETE
                    | libc::NOTE_RENAME,
            ),
        ];
        let changes: Vec<libc::kevent> = watched
            .iter()
            .filter_map(|&(fd, flags)| fd.map(|fd| vnode_event(fd, flags)))
            .collect();
        let mut event = vnode_event(0, 0);
        // SAFETY: `changes` and `event` are valid for the lengths given,
        // and the call blocks until an event arrives.
        let received = unsafe {
            libc::kevent(
                queue,
                changes.as_ptr(),
                changes.len() as i32,
                &mut event,
                1,
                std::ptr::null(),
            )
        };
        for fd in watched.iter().filter_map(|&(fd, _)| fd) {
            // SAFETY: `fd` was opened above and is closed once; closing it
            // also removes its kqueue registration.
            unsafe { libc::close(fd) };
        }
        if received < 0 {
            // A signal interrupted the wait: just wait again.
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        if event.filter == libc::EVFILT_USER || changed.unbounded_send(()).is_err() {
            break;
        }
    }
}

/// Opens `path` only to receive events about it, or `None` if it doesn't
/// exist.
fn open_for_events(path: &Path) -> Option<i32> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `path` is a valid C string. O_CLOEXEC keeps the descriptor
    // out of child processes Kami starts.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_EVTONLY | libc::O_CLOEXEC) };
    (fd >= 0).then_some(fd)
}

fn vnode_event(fd: i32, flags: u32) -> libc::kevent {
    libc::kevent {
        ident: fd as usize,
        filter: libc::EVFILT_VNODE,
        flags: libc::EV_ADD | libc::EV_CLEAR,
        fflags: flags,
        data: 0,
        udata: std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// Whether a change arrives within a second.
    fn changed(watch: &mut Watch) -> bool {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if watch.take_pending() {
                // Drain any further events from the same change.
                std::thread::sleep(Duration::from_millis(50));
                watch.take_pending();
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn reports_writes_replacements_and_new_files() {
        let dir = std::env::temp_dir().join(format!("kami-file-watch-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let mut changes = watch(path.clone());
        std::thread::sleep(Duration::from_millis(100));

        std::fs::write(&path, "a").unwrap();
        assert!(changed(&mut changes), "created");
        std::fs::write(&path, "b").unwrap();
        assert!(changed(&mut changes), "written");
        // Editors often save by renaming a new file over the old one.
        let temp = dir.join("config.toml.tmp");
        std::fs::write(&temp, "c").unwrap();
        std::fs::rename(&temp, &path).unwrap();
        assert!(changed(&mut changes), "replaced");
        std::fs::write(&path, "d").unwrap();
        assert!(changed(&mut changes), "written after being replaced");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropping_stops_the_thread() {
        let dir = std::env::temp_dir().join(format!("kami-file-watch-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let watch = watch(dir.join("notes.md"));
        let queue = Arc::downgrade(watch.queue.as_ref().unwrap());
        drop(watch);
        let deadline = Instant::now() + Duration::from_secs(1);
        while queue.upgrade().is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(queue.upgrade().is_none(), "the thread let go of its queue");
        std::fs::remove_dir_all(&dir).ok();
    }
}
