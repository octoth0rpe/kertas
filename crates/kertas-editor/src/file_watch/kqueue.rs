//! Watching with kqueue, on macOS.

use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use futures::channel::mpsc;

/// The user event that stops the watching thread.
const STOP: usize = 1;

/// Wakes the thread watching with `queue`, to stop it.
pub(super) fn stop(queue: &OwnedFd) {
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

/// The events that mean a file may have changed.
const FILE_EVENTS: u32 = libc::NOTE_WRITE
    | libc::NOTE_EXTEND
    | libc::NOTE_ATTRIB
    | libc::NOTE_DELETE
    | libc::NOTE_RENAME;

/// The paths to watch for changes to the file at `path`.
pub(super) fn file_paths(path: &Path) -> Vec<(PathBuf, u32)> {
    let directory = path.parent().unwrap_or(Path::new("/")).to_owned();
    vec![
        (directory, libc::NOTE_WRITE),
        (path.to_owned(), FILE_EVENTS),
    ]
}

/// The paths to watch for changes in `folder`: the folder, its parent,
/// and every file in it.
pub(super) fn folder_paths(folder: &Path) -> Vec<(PathBuf, u32)> {
    let parent = folder.parent().unwrap_or(Path::new("/")).to_owned();
    let mut watched = vec![
        (parent, libc::NOTE_WRITE),
        (
            folder.to_owned(),
            libc::NOTE_WRITE | libc::NOTE_DELETE | libc::NOTE_RENAME,
        ),
    ];
    let files = std::fs::read_dir(folder).into_iter().flatten().flatten();
    watched.extend(files.map(|entry| (entry.path(), FILE_EVENTS)));
    watched
}

/// A new kqueue with the [`STOP`] user event added.
pub(super) fn stop_queue() -> Option<OwnedFd> {
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

/// Sends on `changed` after each change to the paths `watched` gives,
/// until the [`STOP`] event arrives or the receiver is gone.
///
/// A change is sent only once the paths are watched again, so a change
/// made while they weren't, such as the rest of a write that truncated
/// the file first, is seen by whoever reads the file on hearing of it.
pub(super) fn watch_until_stopped(
    watched: &dyn Fn() -> Vec<(PathBuf, u32)>,
    queue: i32,
    changed: &mpsc::UnboundedSender<()>,
) {
    let mut pending = false;
    loop {
        let paths = watched();
        let watched: Vec<_> = paths
            .iter()
            .map(|(path, flags)| (open_for_events(path), *flags))
            .collect();
        let close = || {
            for fd in watched.iter().filter_map(|&(fd, _)| fd) {
                // SAFETY: `fd` was opened above and is closed once; closing
                // it also removes its kqueue registration.
                unsafe { libc::close(fd) };
            }
        };
        let changes: Vec<libc::kevent> = watched
            .iter()
            .filter_map(|&(fd, flags)| fd.map(|fd| vnode_event(fd, flags)))
            .collect();
        // SAFETY: `changes` is valid for its length, and with no room for
        // events the call only adds them, without waiting.
        unsafe {
            libc::kevent(
                queue,
                changes.as_ptr(),
                changes.len() as i32,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        // A file replaced between being opened and watched would be
        // watched no more, its events lost before it was: start again.
        let replaced = paths
            .iter()
            .zip(&watched)
            .any(|((path, _), &(fd, _))| !still_at(path, fd));
        if replaced {
            close();
            pending = true;
            continue;
        }
        if std::mem::take(&mut pending) && changed.unbounded_send(()).is_err() {
            break;
        }
        let mut event = vnode_event(0, 0);
        // SAFETY: `event` is valid for the one event asked for, and the
        // call blocks until an event arrives.
        let received =
            unsafe { libc::kevent(queue, std::ptr::null(), 0, &mut event, 1, std::ptr::null()) };
        close();
        if received < 0 {
            // A signal interrupted the wait: just wait again.
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        if event.filter == libc::EVFILT_USER {
            break;
        }
        pending = true;
    }
}

/// Opens `path` only to receive events about it, or `None` if it doesn't
/// exist.
fn open_for_events(path: &Path) -> Option<i32> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `path` is a valid C string. O_CLOEXEC keeps the descriptor
    // out of child processes the app starts.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_EVTONLY | libc::O_CLOEXEC) };
    (fd >= 0).then_some(fd)
}

/// Whether `path` is still the file `fd` was opened on, or still missing
/// if it wasn't there to open.
fn still_at(path: &Path, fd: Option<i32>) -> bool {
    use std::os::unix::fs::MetadataExt;

    let now = std::fs::metadata(path).ok();
    let Some(fd) = fd else {
        return now.is_none();
    };
    let mut opened = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `fd` is open, and `opened` has room for the result.
    if unsafe { libc::fstat(fd, opened.as_mut_ptr()) } != 0 {
        return false;
    }
    // SAFETY: fstat succeeded, so it filled `opened` in.
    let opened = unsafe { opened.assume_init() };
    now.is_some_and(|now| now.dev() == opened.st_dev as u64 && now.ino() == opened.st_ino)
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
