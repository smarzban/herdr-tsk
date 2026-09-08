use super::*;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = env::temp_dir().join(format!(
            "tsk-setup-unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn real_confirmation_parser_accepts_only_yes_and_rejects_eof() {
    for (answer, expected) in [
        (" y \n", true),
        ("YES\n", true),
        ("n\n", false),
        ("\n", false),
        ("sure\n", false),
    ] {
        let mut output = Vec::new();
        let mut input = io::Cursor::new(answer);
        assert_eq!(
            confirm(
                &mut input,
                &mut output,
                "prefix+t",
                "bad\x1b]52;c;evil\x07\nnext"
            )
            .unwrap(),
            expected
        );
        assert!(!output.contains(&0x1b));
        assert!(!output.contains(&7));
        assert!(String::from_utf8(output).unwrap().contains("[y/N]"));
    }
    assert!(confirm(
        &mut io::Cursor::new(""),
        &mut Vec::new(),
        "prefix+t",
        "existing"
    )
    .unwrap_err()
    .to_string()
    .contains("no changes"));
}
#[test]
fn upgrade_replaces_one_registration_and_removes_only_the_intact_old_root() {
    let temp = Temp::new();
    let config = temp.0.join("config.toml");
    let registry = RefCell::new(BTreeMap::<String, PathBuf>::new());
    let mut host = |args: &[&str], _: &Path| -> io::Result<String> {
        if args.starts_with(&["plugin", "link"]) {
            registry
                .borrow_mut()
                .insert("herdr-tsk".into(), args[2].into());
        }
        Ok(serde_json::json!({"result":{"plugins":registry.borrow().iter().map(|(id,root)|serde_json::json!({"plugin_id":id,"plugin_root":root})).collect::<Vec<_>>()}}).to_string())
    };
    let a = run_at(
        &config,
        "0.5.0",
        &mut io::Cursor::new(""),
        &mut Vec::new(),
        false,
        &mut host,
    )
    .unwrap();
    let b = run_at(
        &config,
        "0.5.1",
        &mut io::Cursor::new(""),
        &mut Vec::new(),
        false,
        &mut host,
    )
    .unwrap();
    assert_ne!(a.root, b.root);
    assert!(!a.root.exists());
    assert!(b.root.exists());
    assert_eq!(registry.borrow().len(), 1);
    assert_eq!(registry.borrow()["herdr-tsk"], b.root);
    assert_eq!(fs::read_dir(b.root.parent().unwrap()).unwrap().count(), 1);
    let doc = fs::read_to_string(b.root.join("herdr-plugin.toml"))
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(doc["version"].as_str(), Some("0.5.1"));
    let again = run_at(
        &config,
        "0.5.1",
        &mut io::Cursor::new(""),
        &mut Vec::new(),
        false,
        &mut host,
    )
    .unwrap();
    assert_eq!(again.root, b.root);
}
#[test]
fn unsuccessful_upgrade_keeps_previous_registration_and_assets() {
    let temp = Temp::new();
    let config = temp.0.join("config.toml");
    let old = RefCell::new(None::<PathBuf>);
    let fail = std::cell::Cell::new(false);
    let mut host = |args: &[&str], _: &Path| -> io::Result<String> {
        if args.starts_with(&["plugin", "link"]) {
            if fail.get() {
                return Err(error("link failed"));
            }
            *old.borrow_mut() = Some(args[2].into());
        }
        Ok(serde_json::json!({"result":{"plugins":old.borrow().iter().map(|p|serde_json::json!({"plugin_id":"herdr-tsk","plugin_root":p})).collect::<Vec<_>>()}}).to_string())
    };
    let a = run_at(
        &config,
        "0.5.0",
        &mut io::Cursor::new(""),
        &mut Vec::new(),
        false,
        &mut host,
    )
    .unwrap();
    let before = fs::read(&config).unwrap();
    fail.set(true);
    assert!(run_at(
        &config,
        "0.5.1",
        &mut io::Cursor::new(""),
        &mut Vec::new(),
        false,
        &mut host
    )
    .is_err());
    assert!(a.root.exists());
    assert_eq!(*old.borrow(), Some(a.root));
    assert_eq!(fs::read(&config).unwrap(), before);
}
#[test]
fn descriptor_writes_and_cleanup_stay_pinned_after_parent_swap() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    let dir = Dir::open(&temp.0.join("config"), true).unwrap();
    let outside = temp.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::rename(&dir.path, temp.0.join("moved")).unwrap();
    symlink(&outside, &dir.path).unwrap();
    assert!(dir.validate().is_err());
    dir.write_new(Path::new("staged"), "ours").unwrap();
    dir.rename(Path::new("staged"), Path::new("config.toml"))
        .unwrap();
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    assert_eq!(
        fs::read_to_string(temp.0.join("moved/config.toml")).unwrap(),
        "ours"
    );
    dir.remove(Path::new("config.toml"), false).unwrap();
    assert!(!temp.0.join("moved/config.toml").exists());
}
