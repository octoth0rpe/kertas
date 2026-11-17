//! Watching with inotify, on Linux.
//!
//! The "queue" the shared code holds is an eventfd: writing to it wakes
//! the watching thread to stop it. Each round, the thread makes a new
//! inotify instance and watches the paths it's given, so a file replaced
//! by a rename is watched again where it is now.

use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use futures::channel::mpsc;

/// The events that mean a file may have changed.
const FILE_EVENTS: u32 = libc::IN_MODIFY
    | libc::IN_ATTRIB
    | libc::IN_CLOSE_WRITE
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF;

/// The events that mean a directory's entries changed.
const DIRECTORY_EVENTS: u32 =
    libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO;

/// Wakes the thread watching with `queue`, to stop it.
pub(super) fn stop(queue: &OwnedFd) {
    let one: u64 = 1;
    // SAFETY: the eventfd is open while `queue` is held, and `one` is the
    // eight bytes an eventfd takes.
    unsafe {
        libc::write(
            queue.as_raw_fd(),
            (&one as *const u64).cast(),
            size_of::<u64>(),
        )
    };
}

/// The paths to watch for changes to the file at `path`: the file, and
/// its directory for its being created, replaced or deleted.
pub(super) fn file_paths(path: &Path) -> Vec<(PathBuf, u32)> {
    let directory = path.parent().unwrap_or(Path::new("/")).to_owned();
    vec![
        (directory, DIRECTORY_EVENTS),
        (path.to_owned(), FILE_EVENTS),
    ]
}

/// The paths to watch for changes in `folder`: its parent, for the folder
/// being created or deleted, and the folder. inotify reports changes to
/// the files in a watched folder, so they needn't be watched one by one.
pub(super) fn folder_paths(folder: &Path) -> Vec<(PathBuf, u32)> {
    let parent = folder.parent().unwrap_or(Path::new("/")).to_owned();
    vec![
        (parent, DIRECTORY_EVENTS),
        (folder.to_owned(), DIRECTORY_EVENTS | FILE_EVENTS),
    ]
}

/// A new eventfd, to stop the watching thread with.
pub(super) fn stop_queue() -> Option<OwnedFd> {
    // SAFETY: eventfd takes an initial value and flags; a negative result
    // is an error.
    let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
    // SAFETY: `fd` is a new descriptor that nothing else owns.
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Sends on `changed` after each change to the paths `watched` gives,
/// until `stop` is written to or the receiver is gone.
///
/// A change is sent only once the paths are watched again, so a change
/// made while they weren't is seen by whoever reads the file on hearing
/// of it.
pub(super) fn watch_until_stopped(
    watched: &dyn Fn() -> Vec<(PathBuf, u32)>,
    stop: i32,
    changed: &mpsc::UnboundedSender<()>,
) {
    let mut pending = false;
    loop {
        // SAFETY: inotify_init1 takes flags; a negative result is an error.
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
        if fd < 0 {
            break;
        }
        // SAFETY: `fd` is a new descriptor that nothing else owns. Closing
        // it removes its watches.
        let inotify = unsafe { OwnedFd::from_raw_fd(fd) };
        for (path, mask) in watched() {
            let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
                continue;
            };
            // SAFETY: `path` is a valid C string. A path that doesn't
            // exist fails, and isn't watched.
            unsafe { libc::inotify_add_watch(inotify.as_raw_fd(), path.as_ptr(), mask) };
        }
        if std::mem::take(&mut pending) && changed.unbounded_send(()).is_err() {
            break;
        }
        let mut fds = [
            libc::pollfd {
                fd: inotify.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: stop,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `fds` is valid for its length, and with no timeout the
        // call blocks until one is readable.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if ready < 0 {
            // A signal interrupted the wait: just wait again.
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        if fds[1].revents != 0 {
            break;
        }
        // The events themselves don't matter, only that there were some;
        // the instance is closed and a new one made.
        let mut buffer = [0u8; 4096];
        // SAFETY: `buffer` has room for the length given. The descriptor
        // doesn't block, so this stops once the events are read.
        while unsafe {
            libc::read(
                inotify.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        } > 0
        {}
        pending = true;
    }
}
