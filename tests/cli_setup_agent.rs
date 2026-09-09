use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tsk_tui::cli::run_with;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tsk-agent-site-setup-{label}-{nanos}-{seq}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn cli(args: &[&str]) -> tsk_tui::cli::CliOutput {
    run_with(args.iter().copied(), Cursor::new(Vec::<u8>::new()), true)
}

fn skill_source() -> &'static str {
    include_str!("../skills/tsk-cli/SKILL.md")
}

#[test]
fn skill_dir_writes_full_skill_prints_path_exits_0() {
    let _lock = env_lock();
    let root = temp_dir("write");
    let skill_dir = root.join("skills");
    let output = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
    ]);
    let written = skill_dir.join("tsk-cli/SKILL.md");
    assert_eq!(output.code, 0);
    assert!(output.stderr.is_empty());
    assert!(
        output.stdout.contains(written.to_str().expect("utf-8")),
        "stdout should print the written path, got {:?}",
        output.stdout
    );
    assert_eq!(
        fs::read_to_string(&written).expect("read written skill"),
        skill_source()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn second_run_without_force_is_skill_exists_and_leaves_file() {
    let _lock = env_lock();
    let root = temp_dir("exists");
    let skill_dir = root.join("skills");
    let first = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
    ]);
    assert_eq!(first.code, 0);
    let written = skill_dir.join("tsk-cli/SKILL.md");
    fs::write(&written, "stale-marker\n").expect("stamp file");
    let second = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
    ]);
    assert_eq!(second.code, 1);
    assert!(
        second.stderr.contains("skill-exists"),
        "stderr should name skill-exists, got {:?}",
        second.stderr
    );
    assert_eq!(
        fs::read_to_string(&written).expect("unchanged"),
        "stale-marker\n"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn force_overwrites() {
    let _lock = env_lock();
    let root = temp_dir("force");
    let skill_dir = root.join("skills");
    let written = skill_dir.join("tsk-cli/SKILL.md");
    fs::create_dir_all(written.parent().expect("parent")).expect("mkdir");
    fs::write(&written, "stale-marker\n").expect("stamp file");
    let output = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
        "--force",
    ]);
    assert_eq!(output.code, 0);
    assert_eq!(
        fs::read_to_string(&written).expect("overwritten"),
        skill_source()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bare_setup_lists_targets_and_writes_nothing() {
    let _lock = env_lock();
    let root = temp_dir("list");
    let home = root.join("home");
    fs::create_dir_all(&home).expect("home");
    let previous_home = std::env::var_os("HOME");
    let previous_xdg = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
    let output = cli(&["tsk", "setup"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_xdg {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    assert_eq!(output.code, 0);
    for token in ["herdr", "claude", "pi", "codex", "cursor", "--skill-dir"] {
        assert!(
            output.stdout.contains(token),
            "bare setup should list {token}, got {:?}",
            output.stdout
        );
    }
    assert!(
        !home.join(".claude").exists(),
        "bare setup must not write agent skills"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn two_targets_or_herdr_plus_skill_dir_are_usage() {
    let _lock = env_lock();
    let root = temp_dir("usage");
    let skill_dir = root.join("skills");
    let two = cli(&["tsk", "setup", "claude", "pi"]);
    assert_eq!(two.code, 2);
    let mixed = cli(&[
        "tsk",
        "setup",
        "herdr",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
    ]);
    assert_eq!(mixed.code, 2);
    assert!(!skill_dir.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn json_emits_written_exists_or_listed() {
    let _lock = env_lock();
    let root = temp_dir("json");
    let skill_dir = root.join("skills");
    let path = skill_dir.to_str().expect("utf-8");
    let written = cli(&["tsk", "setup", "--skill-dir", path, "--json"]);
    assert_eq!(written.code, 0);
    let written_value: serde_json::Value =
        serde_json::from_str(written.stdout.trim()).expect("written json");
    assert_eq!(written_value["outcome"], "written");
    assert!(written_value["path"].as_str().is_some());
    assert!(written_value["target"].as_str().is_some());

    let exists = cli(&["tsk", "setup", "--skill-dir", path, "--json"]);
    assert_eq!(exists.code, 1);
    let exists_value: serde_json::Value =
        serde_json::from_str(exists.stdout.trim()).expect("exists json");
    assert_eq!(exists_value["outcome"], "exists");

    let listed = cli(&["tsk", "setup", "--json"]);
    assert_eq!(listed.code, 0);
    let listed_value: serde_json::Value =
        serde_json::from_str(listed.stdout.trim()).expect("listed json");
    assert_eq!(listed_value["outcome"], "listed");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn setup_herdr_still_matches_existing_tests() {
    let help = cli(&["tsk", "setup", "herdr", "--help"]);
    assert_eq!(help.code, 0);
    assert!(help.stdout.contains("prefix+t"));
    let bad = cli(&["tsk", "setup", "herdr", "--force"]);
    assert_eq!(bad.code, 2);
}
