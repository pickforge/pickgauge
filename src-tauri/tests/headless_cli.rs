use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn isolated_home() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "pickgauge-headless-cli-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("isolated home is created");
    path
}

fn run_headless(home: &Path, args: &[&str]) -> Output {
    let config = home.join("config");
    let data = home.join("data");
    let cache = home.join("cache");

    Command::new(env!("CARGO_BIN_EXE_pickgauge"))
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .env("APPDATA", &config)
        .env("LOCALAPPDATA", &data)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("XAUTHORITY")
        .output()
        .expect("headless command starts")
}

#[test]
fn real_binary_runs_version_and_usage_json_without_a_display() {
    let home = isolated_home();

    let version = run_headless(&home, &["--version"]);
    assert!(version.status.success(), "--version failed: {version:?}");
    assert_eq!(version.stderr, b"");
    assert_eq!(
        version.stdout,
        format!("pickgauge {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );

    let usage = run_headless(&home, &["usage", "--json"]);
    assert!(usage.status.success(), "usage --json failed: {usage:?}");
    assert_eq!(usage.stderr, b"");
    let document: Value = serde_json::from_slice(&usage.stdout).expect("usage output is JSON");
    assert_eq!(document.get("version"), Some(&Value::from(1)));
    assert!(
        document.get("services").is_some_and(Value::is_array),
        "services must be an array: {document}"
    );

    let invalid = run_headless(&home, &["usage", "--yaml"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(invalid.stdout, b"");
    assert_eq!(invalid.stderr, b"Usage: pickgauge usage [--json]\n");

    fs::remove_dir_all(home).expect("isolated home is removed");
}
