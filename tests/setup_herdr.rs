use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use tsk_tui::setup::edit_bindings;
static NEXT: AtomicU64 = AtomicU64::new(0);
fn temp() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tsk-setup-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn creates_bindings_once_and_preserves_other_settings() {
    let source =
        "# my settings\n[ui]\nmouse_capture = true\n[keys]\nprefix = 'alt+z' # custom prefix\n";
    let first = edit_bindings(source, false, |_, _| panic!("no conflicts")).unwrap();
    assert!(first.contains("# my settings"));
    assert!(first.contains("prefix = 'alt+z' # custom prefix"));
    assert!(first.contains("herdr-tsk.open-board"));
    assert!(first.contains("herdr-tsk.quick-capture"));
    assert_eq!(
        first,
        edit_bindings(&first, false, |_, _| panic!("idempotent")).unwrap()
    );
}
#[test]
fn conflict_requires_terminal_and_decline_preserves_original_binding() {
    let source = "[[keys.command]]\nkey = 'prefix+t'\ntype = 'shell'\ncommand = 'my-tool'\n";
    assert!(edit_bindings(source, false, |_, _| panic!("must not prompt")).is_err());
    let declined = edit_bindings(source, true, |key, _| {
        assert_eq!(key, "prefix+t");
        Ok(false)
    })
    .unwrap();
    assert!(declined.contains("my-tool"));
    assert!(!declined.contains("herdr-tsk.open-board"));
    assert!(declined.contains("herdr-tsk.quick-capture"));
    let accepted = edit_bindings(source, true, |_, _| Ok(true)).unwrap();
    assert!(!accepted.contains("my-tool"));
    assert!(accepted.contains("herdr-tsk.open-board"));
}
#[test]
fn builtin_and_multi_binding_conflicts_preserve_unrelated_keys() {
    let source = "[keys]\nnew_tab = ['prefix+t', 'prefix+c']\ncommand = [{key = ['prefix+a', 'prefix+f'], type = 'shell', command = 'other'}]\n";
    let mut prompts = 0;
    let updated = edit_bindings(source, true, |_, _| {
        prompts += 1;
        Ok(true)
    })
    .unwrap();
    assert_eq!(prompts, 2);
    let doc = updated.parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(doc["keys"]["new_tab"].as_array().unwrap().len(), 1);
    assert_eq!(doc["keys"]["new_tab"][0].as_str(), Some("prefix+c"));
    assert!(updated.contains("prefix+f"));
    assert!(updated.contains("other"));
}
#[test]
fn noninteractive_conflict_writes_nothing_and_never_calls_herdr() {
    let root = temp();
    let config = root.join("config.toml");
    let source = "[keys]\nnew_tab = 'prefix+t'\n";
    fs::write(&config, source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_tsk"))
        .args(["setup", "herdr"])
        .env("HERDR_CONFIG_PATH", &config)
        .env("XDG_CONFIG_HOME", &root)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(&config).unwrap(), source);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn setup_help_is_headless_and_bad_arguments_are_usage_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_tsk"))
        .args(["setup", "herdr", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("prefix+t"));
    let output = Command::new(env!("CARGO_BIN_EXE_tsk"))
        .args(["setup", "herdr", "--force"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn installed_symlink_is_shared_with_plugin_and_rerun_does_not_duplicate_assets() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = temp();
    let bin = root.join("bin with spaces");
    fs::create_dir(&bin).unwrap();
    let installed = bin.join("tsk");
    symlink(env!("CARGO_BIN_EXE_tsk"), &installed).unwrap();
    let host = bin.join("herdr");
    fs::write(&host, "#!/bin/sh\nif [ \"$1 $2\" = 'plugin link' ]; then printf '%s' \"$3\" > \"$SETUP_LINK\"; fi\nexit 0\n").unwrap();
    fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).unwrap();
    let config = root.join("herdr/config.toml");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let run = || {
        Command::new(&installed)
            .args(["setup", "herdr"])
            .env("PATH", &path)
            .env("HERDR_CONFIG_PATH", &config)
            .env("SETUP_LINK", root.join("linked"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let first = fs::read_to_string(&config).unwrap();
    let linked = fs::read_to_string(root.join("linked")).unwrap();
    let manifest = fs::read_to_string(PathBuf::from(&linked).join("herdr-plugin.toml"))
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    assert!(manifest.get("build").is_none());
    assert_eq!(
        manifest["panes"]
            .as_array_of_tables()
            .unwrap()
            .get(0)
            .unwrap()["command"][0]
            .as_str(),
        installed.to_str()
    );
    let script = fs::read_to_string(PathBuf::from(&linked).join("scripts/open-board.sh")).unwrap();
    assert!(script.contains(&format!("plugin_bin='{}'", installed.display())));
    assert!(!script.contains("plugin_bin=\"${TSK_BIN:"));
    assert!(run().status.success());
    assert_eq!(fs::read_to_string(&config).unwrap(), first);
    assert_eq!(fs::read_to_string(root.join("linked")).unwrap(), linked);
    assert_eq!(
        fs::read_dir(root.join("herdr/tsk-plugins"))
            .unwrap()
            .count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn failed_host_registration_keeps_original_config_and_cleans_lock() {
    use std::os::unix::fs::PermissionsExt;
    let root = temp();
    let host = root.join("herdr");
    fs::write(
        &host,
        "#!/bin/sh\nif [ \"$1 $2\" = 'plugin link' ]; then echo refused >&2; exit 1; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&host, fs::Permissions::from_mode(0o755)).unwrap();
    let config = root.join("config.toml");
    let original = "# keep this\n[ui]\nmouse_capture = true\n";
    fs::write(&config, original).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tsk"))
        .args(["setup", "herdr"])
        .env(
            "PATH",
            format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
        )
        .env("HERDR_CONFIG_PATH", &config)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refused"));
    assert_eq!(fs::read_to_string(&config).unwrap(), original);
    assert!(!root.join(".tsk-setup.lock").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn whitespace_in_chord_is_detected_but_shifted_uppercase_is_not_replaced() {
    assert!(edit_bindings("[keys]\nnew_tab='prefix+ t'\n", false, |_, _| panic!()).is_err());
    let updated = edit_bindings("[keys]\nnew_tab='prefix+T'\n", false, |_, _| panic!()).unwrap();
    assert!(updated.contains("new_tab='prefix+T'"));
}
