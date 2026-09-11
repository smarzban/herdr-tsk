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

fn cli_non_tty(args: &[&str]) -> tsk_tui::cli::CliOutput {
    run_with(args.iter().copied(), Cursor::new(Vec::<u8>::new()), false)
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
fn equal_version_without_force_is_skill_exists_and_leaves_file() {
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
        skill_source()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_or_mismatched_version_updates_without_force() {
    let _lock = env_lock();
    let root = temp_dir("update");
    let skill_dir = root.join("skills");
    let written = skill_dir.join("tsk-cli/SKILL.md");
    fs::create_dir_all(written.parent().expect("parent")).expect("mkdir");
    fs::write(&written, "stale-marker\n").expect("stamp missing version");
    let missing = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
    ]);
    assert_eq!(missing.code, 0, "{missing:?}");
    assert_eq!(
        fs::read_to_string(&written).expect("updated"),
        skill_source()
    );

    let outdated =
        "---\nname: tsk-cli\ndescription: stale\nversion: 0.0.1\n---\n\nstale body\n".to_string();
    fs::write(&written, &outdated).expect("stamp old version");
    let updated = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
    ]);
    assert_eq!(updated.code, 0, "{updated:?}");
    assert_eq!(
        fs::read_to_string(&written).expect("updated again"),
        skill_source()
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
fn bare_setup_non_tty_lists_targets_and_writes_nothing() {
    let _lock = env_lock();
    let root = temp_dir("list");
    let home = root.join("home");
    fs::create_dir_all(&home).expect("home");
    let previous_home = std::env::var_os("HOME");
    let previous_xdg = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
    let output = cli_non_tty(&["tsk", "setup"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_xdg {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    assert_eq!(output.code, 0);
    for token in [
        "herdr",
        "claude",
        "pi",
        "codex",
        "cursor",
        "grok",
        "opencode",
        "--skill-dir",
    ] {
        assert!(
            output.stdout.contains(token),
            "bare setup should list {token}, got {:?}",
            output.stdout
        );
    }
    assert!(
        output.stdout.contains(".grok/skills"),
        "bare setup should advertise the grok skills dir, got {:?}",
        output.stdout
    );
    assert!(
        output.stdout.contains(".config/opencode/skills"),
        "bare setup should advertise the opencode skills dir, got {:?}",
        output.stdout
    );
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

    let detected = cli(&["tsk", "setup", "--json"]);
    assert_eq!(detected.code, 0);
    let detected_value: serde_json::Value =
        serde_json::from_str(detected.stdout.trim()).expect("detected json");
    assert_eq!(detected_value["outcome"], "detected");
    assert!(detected_value["embedded_skill_version"].as_str().is_some());
    assert!(detected_value["agents"].as_array().is_some());
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

#[test]
fn empty_skill_dir_is_usage_and_writes_nothing() {
    let _lock = env_lock();
    let root = temp_dir("empty-dir");
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&root).expect("chdir temp");
    let space = cli(&["tsk", "setup", "--skill-dir", ""]);
    let equals = cli(&["tsk", "setup", "--skill-dir="]);
    std::env::set_current_dir(&previous).expect("restore cwd");
    assert_eq!(space.code, 2, "{space:?}");
    assert_eq!(equals.code, 2, "{equals:?}");
    assert!(
        !root.join("tsk-cli").exists(),
        "empty --skill-dir must not write into cwd"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn named_agent_targets_write_under_home() {
    let _lock = env_lock();
    let root = temp_dir("named");
    let home = root.join("home");
    fs::create_dir_all(&home).expect("home");
    let previous_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home);
    let cases = [
        ("claude", ".claude/skills"),
        ("pi", ".pi/agent/skills"),
        ("cursor", ".cursor/skills"),
        ("codex", ".agents/skills"),
        ("grok", ".grok/skills"),
        ("opencode", ".config/opencode/skills"),
    ];
    for (name, suffix) in cases {
        let dest = home.join(suffix).join("tsk-cli/SKILL.md");
        let output = cli(&["tsk", "setup", name, "--force"]);
        assert_eq!(output.code, 0, "{name}: {output:?}");
        assert_eq!(
            fs::read_to_string(&dest).expect("written skill"),
            skill_source(),
            "{name} path"
        );
    }
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn force_refuses_a_symlinked_skill_file_and_directory() {
    let _lock = env_lock();
    let root = temp_dir("symlink");
    let skill_dir = root.join("skills");
    let outside = root.join("outside");
    fs::create_dir_all(&outside).expect("outside");
    let planted = outside.join("SKILL.md");
    fs::write(&planted, "do-not-clobber\n").expect("plant");

    fs::create_dir_all(skill_dir.join("tsk-cli")).expect("skill folder");
    std::os::unix::fs::symlink(&planted, skill_dir.join("tsk-cli/SKILL.md")).expect("link file");
    let file = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
        "--force",
    ]);
    assert_eq!(file.code, 1, "{file:?}");
    assert!(
        file.stderr.contains("refusing symlink"),
        "stderr should refuse the file symlink, got {:?}",
        file.stderr
    );
    assert_eq!(
        fs::read_to_string(&planted).expect("untouched file"),
        "do-not-clobber\n"
    );

    let _ = fs::remove_dir_all(skill_dir.join("tsk-cli"));
    std::os::unix::fs::symlink(&outside, skill_dir.join("tsk-cli")).expect("link dir");
    let dir = cli(&[
        "tsk",
        "setup",
        "--skill-dir",
        skill_dir.to_str().expect("utf-8"),
        "--force",
    ]);
    assert_eq!(dir.code, 1, "{dir:?}");
    assert!(
        dir.stderr.contains("refusing symlink"),
        "stderr should refuse the directory symlink, got {:?}",
        dir.stderr
    );
    assert_eq!(
        fs::read_to_string(&planted).expect("untouched dir target"),
        "do-not-clobber\n"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn detected_ids_prints_space_separated_agents() {
    let _lock = env_lock();
    let root = temp_dir("ids");
    let home = root.join("home");
    fs::create_dir_all(home.join(".cursor")).expect("cursor");
    fs::create_dir_all(home.join(".claude")).expect("claude");
    fs::create_dir_all(home.join(".config/opencode")).expect("opencode");
    let previous_home = std::env::var_os("HOME");
    let previous_path = std::env::var_os("PATH");
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", root.join("empty-bin"));
    fs::create_dir_all(root.join("empty-bin")).expect("empty bin");
    let output = cli_non_tty(&["tsk", "setup", "--detected-ids"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    assert_eq!(output.code, 0, "{output:?}");
    let ids: Vec<&str> = output.stdout.split_whitespace().collect();
    assert!(ids.contains(&"cursor"), "{ids:?}");
    assert!(ids.contains(&"claude"), "{ids:?}");
    assert!(ids.contains(&"opencode"), "{ids:?}");
    assert!(!ids.contains(&"grok"), "{ids:?}");
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn opencode_detects_via_path_binary() {
    let _lock = env_lock();
    let root = temp_dir("opencode-path");
    let home = root.join("home");
    let bin = root.join("bin");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&bin).expect("bin");
    fs::write(bin.join("opencode"), "#!/bin/sh\n").expect("stub");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(bin.join("opencode"))
        .expect("meta")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(bin.join("opencode"), perms).expect("chmod");
    let previous_home = std::env::var_os("HOME");
    let previous_path = std::env::var_os("PATH");
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", &bin);
    let output = cli_non_tty(&["tsk", "setup", "--detected-ids"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    assert_eq!(output.code, 0, "{output:?}");
    let ids: Vec<&str> = output.stdout.split_whitespace().collect();
    assert_eq!(ids, vec!["opencode"], "{ids:?}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn agents_yes_installs_without_asking() {
    let _lock = env_lock();
    let root = temp_dir("agents-yes");
    let home = root.join("home");
    fs::create_dir_all(home.join(".claude")).expect("claude");
    let previous_home = std::env::var_os("HOME");
    let previous_path = std::env::var_os("PATH");
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", root.join("empty-bin"));
    fs::create_dir_all(root.join("empty-bin")).expect("empty bin");
    let output = cli_non_tty(&["tsk", "setup", "agents", "--yes"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    assert_eq!(output.code, 0, "{output:?}");
    assert_eq!(
        fs::read_to_string(home.join(".claude/skills/tsk-cli/SKILL.md")).expect("written"),
        skill_source()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn agents_yes_json_is_machine_readable() {
    let _lock = env_lock();
    let root = temp_dir("agents-yes-json");
    let home = root.join("home");
    fs::create_dir_all(home.join(".claude")).expect("claude");
    let previous_home = std::env::var_os("HOME");
    let previous_path = std::env::var_os("PATH");
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", root.join("empty-bin"));
    fs::create_dir_all(root.join("empty-bin")).expect("empty bin");
    let output = cli_non_tty(&["tsk", "setup", "agents", "--yes", "--json"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    assert_eq!(output.code, 0, "{output:?}");
    let payload: serde_json::Value =
        serde_json::from_str(output.stdout.trim()).expect("batch json");
    assert_eq!(payload["outcome"], "batch");
    assert_eq!(payload["applied"][0]["id"], "claude");
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn agents_yes_reports_blocked_skill_roots() {
    let _lock = env_lock();
    let root = temp_dir("agents-blocked");
    let home = root.join("home");
    let outside = root.join("outside");
    fs::create_dir_all(home.join(".claude")).expect("claude");
    fs::create_dir_all(&outside).expect("outside");
    std::os::unix::fs::symlink(&outside, home.join(".claude/skills")).expect("skill link");
    let previous_home = std::env::var_os("HOME");
    let previous_path = std::env::var_os("PATH");
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", root.join("empty-bin"));
    fs::create_dir_all(root.join("empty-bin")).expect("empty bin");
    let output = cli_non_tty(&["tsk", "setup", "agents", "--yes", "--json"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    assert_eq!(output.code, 1, "{output:?}");
    assert!(output.stderr.contains("blocked agent skill roots: claude"));
    let payload: serde_json::Value =
        serde_json::from_str(output.stdout.trim()).expect("batch json");
    assert_eq!(payload["blocked"], serde_json::json!(["claude"]));
    assert!(!outside.join("tsk-cli/SKILL.md").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bare_setup_non_tty_with_detected_agents_prints_guidance_without_writing() {
    let _lock = env_lock();
    let root = temp_dir("bare-nontty");
    let home = root.join("home");
    fs::create_dir_all(home.join(".cursor")).expect("cursor");
    let previous_home = std::env::var_os("HOME");
    let previous_path = std::env::var_os("PATH");
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", root.join("empty-bin"));
    fs::create_dir_all(root.join("empty-bin")).expect("empty bin");
    let output = cli_non_tty(&["tsk", "setup"]);
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    assert_eq!(output.code, 0, "{output:?}");
    assert!(
        !home.join(".cursor/skills/tsk-cli/SKILL.md").exists(),
        "non-TTY bare setup must not write skills"
    );
    let _ = fs::remove_dir_all(root);
}
