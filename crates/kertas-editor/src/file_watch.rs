//! Notifications when a file changes, using kqueue on macOS and inotify
//! on Linux.
//!
//! Editors often save by writing a new file and renaming it over the old
//! one, so this watches both the file and its directory, and starts again
//! after every change to follow the file that's there now. A folder is
//! watched the same way: the folder, its parent, and every file in it.

use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc;

#[cfg(target_os = "linux")]
#[path = "file_watch/inotify.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "file_watch/kqueue.rs"]
mod platform;

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
            platform::stop(queue);
        }
    }
}

/// Reports whenever the file at `path` may have changed: written,
/// replaced, created or deleted. Its directory must exist. Watching stops
/// when the [`Watch`] is dropped.
pub fn watch(path: PathBuf) -> Watch {
    start(move || platform::file_paths(&path))
}

/// Reports whenever a file in `folder` may have changed, or one was added
/// or removed, or the folder itself was created or deleted. Its parent
/// must exist. Watching stops when the [`Watch`] is dropped.
pub fn watch_folder(folder: PathBuf) -> Watch {
    start(move || platform::folder_paths(&folder))
}

/// Starts a thread reporting changes to the paths `watched` gives, with
/// the events to watch each for. It's asked again after every change.
fn start(watched: impl Fn() -> Vec<(PathBuf, u32)> + Send + 'static) -> Watch {
    let (changed, changes) = mpsc::unbounded();
    let queue = platform::stop_queue().map(Arc::new);
    if let Some(queue) = queue.clone() {
        std::thread::Builder::new()
            .name("kertas-file-watch".into())
            .spawn(move || {
                platform::watch_until_stopped(
                    &watched,
                    std::os::fd::AsRawFd::as_raw_fd(&*queue),
                    &changed,
                )
            })
            .ok();
    }
    Watch { changes, queue }
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
        let dir = std::env::temp_dir().join(format!("kertas-file-watch-{}", std::process::id()));
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
    fn reports_changes_in_a_folder() {
        let dir = std::env::temp_dir().join(format!("kertas-folder-watch-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("themes");
        let mut changes = watch_folder(folder.clone());
        std::thread::sleep(Duration::from_millis(100));

        std::fs::create_dir(&folder).unwrap();
        assert!(changed(&mut changes), "folder created");
        let path = folder.join("Mine.toml");
        std::fs::write(&path, "a").unwrap();
        assert!(changed(&mut changes), "file added");
        std::fs::write(&path, "b").unwrap();
        assert!(changed(&mut changes), "file written");
        std::fs::remove_file(&path).unwrap();
        assert!(changed(&mut changes), "file removed");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropping_stops_the_thread() {
        let dir =
            std::env::temp_dir().join(format!("kertas-file-watch-stop-{}", std::process::id()));
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
