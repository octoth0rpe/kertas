//! Notifications when a file changes, using macOS kqueue.
//!
//! Editors often save by writing a new file and renaming it over the old
//! one, so this watches both the file and its directory, and starts again
//! after every change to follow the file that's there now.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use futures::channel::mpsc;

/// Sends a message whenever the file at `path` may have changed: written,
/// replaced, created or deleted. Its directory must exist.
pub fn watch(path: PathBuf) -> mpsc::UnboundedReceiver<()> {
    let (changed, receiver) = mpsc::unbounded();
    std::thread::Builder::new()
        .name("kami-file-watch".into())
        .spawn(move || {
            // SAFETY: kqueue takes no arguments; a negative result is an
            // error, checked below.
            let queue = unsafe { libc::kqueue() };
            if queue < 0 {
                return;
            }
            let directory = path.parent().unwrap_or(Path::new("/")).to_owned();
            loop {
                let watched = [
                    (open_for_events(&directory), libc::NOTE_WRITE),
                    (
                        open_for_events(&path),
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
                // SAFETY: `changes` and `event` are valid for the lengths
                // given, and the call blocks until an event arrives.
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
                    // SAFETY: `fd` was opened above and is closed once;
                    // closing it also removes its kqueue registration.
                    unsafe { libc::close(fd) };
                }
                if received < 0 || changed.unbounded_send(()).is_err() {
                    break;
                }
            }
            // SAFETY: `queue` is a descriptor from kqueue above.
            unsafe { libc::close(queue) };
        })
        .ok();
    receiver
}

/// Opens `path` only to receive events about it, or `None` if it doesn't
/// exist.
fn open_for_events(path: &Path) -> Option<i32> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `path` is a valid C string.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_EVTONLY) };
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
