//! The `kt` command line script (macos/kt), run with stand-ins for macOS's
//! `open` and for Kertas's executable that print their arguments.

#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Runs the script from inside a fake Kertas.app, in `cwd`, returning the
/// arguments it passed to `open` or to Kertas's executable, one per line.
/// The fake executable prints "Kertas" first, and fails with status 3.
/// Each call gets its own folder, since the tests run in parallel.
fn run(args: &[&str], cwd: &str) -> Vec<String> {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let run = RUNS.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-script-{run}"));
    fs::remove_dir_all(&dir).ok();
    let contents = dir.join("Kertas.app/Contents");
    fs::create_dir_all(contents.join("Resources")).unwrap();
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    let script = contents.join("Resources/kt");
    fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/macos/kt"), &script).unwrap();
    let fake = |path: &PathBuf, script: &str| {
        fs::write(path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    };
    let print_args = "for arg in \"$@\"; do echo \"$arg\"; done\n";
    let fake_open = dir.join("fake-open");
    fake(&fake_open, &format!("#!/bin/sh\n{print_args}"));
    fake(
        &contents.join("MacOS/Kertas"),
        &format!("#!/bin/sh\necho Kertas\n{print_args}exit 3\n"),
    );

    let output = Command::new(&script)
        .args(args)
        .current_dir(cwd)
        .env("KERTAS_OPEN", &fake_open)
        .output()
        .unwrap();
    let ran_kertas = output.stdout.starts_with(b"Kertas\n");
    assert_eq!(
        output.status.code(),
        Some(if ran_kertas { 3 } else { 0 }),
        "{output:?}"
    );
    let app = fs::canonicalize(dir.join("Kertas.app")).unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.replace(app.to_str().unwrap(), "APP"))
        .collect()
}

#[test]
fn no_files_brings_kertas_forward() {
    assert_eq!(run(&[], "/tmp"), ["-a", "APP", "kertas://activate"]);
}

#[test]
fn files_become_encoded_absolute_paths() {
    assert_eq!(
        run(&["/tmp/My Notes/café & 100%.md", "b.md"], "/usr"),
        [
            "-a",
            "APP",
            "kertas://open?path=/tmp/My%20Notes/caf%C3%A9%20%26%20100%25.md",
            "kertas://open?path=/usr/b.md",
        ]
    );
}

#[test]
fn convert_runs_kertas_directly_and_passes_its_status_on() {
    assert_eq!(
        run(&["--convert", "in.md", "out.pdf"], "/tmp"),
        ["Kertas", "--convert", "in.md", "out.pdf"]
    );
}
