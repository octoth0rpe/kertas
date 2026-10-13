//! The `kertas` command line script (macos/kertas), run with a stand-in for
//! macOS's `open` that prints its arguments.

#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

/// Runs the script from inside a fake Kertas.app, in `cwd`, returning the
/// arguments it passed to `open`, one per line.
fn run(args: &[&str], cwd: &str) -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-script");
    fs::remove_dir_all(&dir).ok();
    let resources = dir.join("Kertas.app/Contents/Resources");
    fs::create_dir_all(&resources).unwrap();
    let script = resources.join("kertas");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/macos/kertas"),
        &script,
    )
    .unwrap();
    let fake_open = dir.join("fake-open");
    fs::write(
        &fake_open,
        "#!/bin/sh\nfor arg in \"$@\"; do echo \"$arg\"; done\n",
    )
    .unwrap();
    fs::set_permissions(&fake_open, fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(&script)
        .args(args)
        .current_dir(cwd)
        .env("KERTAS_OPEN", &fake_open)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
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
