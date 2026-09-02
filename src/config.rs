//! Walkthrough dismissal record: one durable value under the plugin config dir.
//!
//! Reading never fails: missing, empty, malformed, unreadable, or field-less
//! documents all read as not dismissed.
//!
//! # What the write does and does not guarantee
//!
//! The atomic-rename shape is copied from [`crate::store`], but two of the store's
//! properties are deliberately *not* reproduced here. Do not read this module as
//! equivalent to the store, and do not lift these lines into a context where the
//! assumptions below stop holding.
//!
//! This module does not `fsync` the containing directory and does not take a lock.
//! A crash can lose a just-recorded dismissal; the walkthrough would reappear once.
//! The payload is a constant, so two writers producing the same bytes is harmless.
//! A caller whose payload varies needs a lock, not this code.
//!
//! # Empty environment values
//!
//! An empty `TSK_CONFIG_DIR` falls through to `$HOME/.tsk` rather than resolving to `""`,
//! which would put `walkthrough.json` in whatever directory the board was launched from.

use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::store::StoreError;

/// On-disk document name under the config directory (distinct from the task store's).
const CONFIG_FILE: &str = "walkthrough.json";

/// The persisted document. A named field, so it can gain siblings without a migration.
///
/// `#[serde(default)]` is on the field for that same reason: a later field can be added
/// without a migration. It is not what makes a missing `dismissed` read as not dismissed
/// today: a parse error would reach the same answer through `.ok()` in [`WalkthroughRecord::is_dismissed`].
#[derive(Serialize, Deserialize)]
struct WalkthroughDocument {
    #[serde(default)]
    dismissed: bool,
}

/// Config dir from `TSK_CONFIG_DIR`, else `~/.tsk`.
///
/// One store everywhere: the herdr plugin pane and a bare terminal run resolve to the same
/// files, so settings live beside the board's data. Herdr's injected
/// `HERDR_PLUGIN_CONFIG_DIR` is deliberately ignored — see [`crate::store::default_state_dir`].
/// Mirrors its refusal to fall back to a shared world path under `std::env::temp_dir()`.
pub fn default_config_dir() -> PathBuf {
    resolve_config_dir(
        env::var_os("TSK_CONFIG_DIR").as_deref(),
        env::var_os("HOME").as_deref(),
    )
}

/// Pure resolution over the candidate values, in precedence order.
///
/// Split out from [`default_config_dir`] so every branch is testable without mutating the
/// process environment: `setenv` racing another thread's `getenv` is a data race, and this
/// crate's tests run in threads inside one process. The relative last resort is a constant
/// and needs no input. Empty values fall through.
fn resolve_config_dir(tsk_env: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf {
    if let Some(dir) = tsk_env {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Some(home) = home {
        if !home.is_empty() {
            return PathBuf::from(home).join(".tsk");
        }
    }
    // Last resort: relative per-process dir (still not shared /tmp/tsk-config).
    PathBuf::from(".tsk-config")
}

/// The single durable fact: the walkthrough was completed or skipped.
#[derive(Debug, Clone)]
pub struct WalkthroughRecord {
    dir: PathBuf,
}

impl WalkthroughRecord {
    /// `dir` is the plugin config directory (not the JSON file itself).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Whether the walkthrough has been dismissed.
    ///
    /// Any unusable document reads as not dismissed: the board must open whatever the
    /// state of this file.
    pub fn is_dismissed(&self) -> bool {
        fs::read_to_string(self.document_path())
            .ok()
            .and_then(|data| serde_json::from_str::<WalkthroughDocument>(&data).ok())
            .is_some_and(|document| document.dismissed)
    }

    /// Record that the walkthrough was completed or skipped.
    ///
    /// # The returned error is to be *presented*, never propagated
    ///
    /// requires the board to stay open and usable when this record cannot be
    /// written, so a caller inside the board must surface the failure in the UI and carry
    /// on. Writing
    ///
    /// ```ignore
    /// record.record_dismissed()?; // WRONG inside a board key handler
    /// ```
    ///
    /// lets the error flow out to `app::run` and tears the board down on a read-only
    /// config directory, which is exactly the violation. Handle it locally:
    ///
    /// ```ignore
    /// if let Err(error) = record.record_dismissed() {
    /// self.set_message(format!("Could not save: {error}"));
    /// }
    /// ```
    ///
    /// Atomic against readers: a unique temp file in the same directory, then rename over
    /// the target, with the temp removed on failure. See the module doc for the two
    /// guarantees this does *not* carry (no directory fsync, no lock).
    pub fn record_dismissed(&self) -> Result<(), StoreError> {
        fs::create_dir_all(&self.dir)?;
        let document = self.document_path();
        let tmp = self.unique_tmp_path();
        let data = serde_json::to_string_pretty(&WalkthroughDocument { dismissed: true })?;
        let write_result = (|| -> Result<(), StoreError> {
            let mut temp_file = File::create(&tmp)?;
            temp_file.write_all(data.as_bytes())?;
            temp_file.sync_all()?;
            drop(temp_file);
            fs::rename(&tmp, &document)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        write_result
    }

    fn document_path(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE)
    }

    /// Unique temp path so concurrent writers never share `.walkthrough.json.tmp`.
    ///
    /// Unlike the store's, this scheme runs without a lock behind it. It is load-bearing
    /// only because this module's payload is a constant, so a collision is benign: see the
    /// module doc before reusing it.
    fn unique_tmp_path(&self) -> PathBuf {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.dir.join(format!(".{CONFIG_FILE}.tmp.{pid}.{nanos}"))
    }
}

/// Chord used for mutating board verbs (`d`, `e`, `space`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerbModifier {
    /// Old settings that named `alt` migrate to the fixed Ctrl modifier.
    #[serde(alias = "alt")]
    #[default]
    Ctrl,
}

impl VerbModifier {
    pub fn prefix(self) -> &'static str {
        "ctrl+"
    }
}

const SETTINGS_FILE: &str = "settings.json";

#[derive(Serialize, Deserialize, Default)]
struct SettingsDocument {
    #[serde(default)]
    verb_modifier: VerbModifier,
}

/// Durable board settings under the plugin config dir.
pub struct SettingsRecord {
    dir: PathBuf,
}

impl SettingsRecord {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn verb_modifier(&self) -> VerbModifier {
        fs::read_to_string(self.dir.join(SETTINGS_FILE))
            .ok()
            .and_then(|data| serde_json::from_str::<SettingsDocument>(&data).ok())
            .map(|document| document.verb_modifier)
            .unwrap_or_default()
    }

    pub fn set_verb_modifier(&self, verb_modifier: VerbModifier) -> Result<(), StoreError> {
        fs::create_dir_all(&self.dir)?;
        let document = self.dir.join(SETTINGS_FILE);
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = self.dir.join(format!(".{SETTINGS_FILE}.tmp.{pid}.{nanos}"));
        let payload = serde_json::to_vec_pretty(&SettingsDocument { verb_modifier })?;
        let write = (|| {
            let mut file = File::create(&tmp)?;
            file.write_all(&payload)?;
            file.sync_all()
        })();
        if let Err(error) = write {
            let _ = fs::remove_file(&tmp);
            return Err(error.into());
        }
        if let Err(error) = fs::rename(&tmp, &document) {
            let _ = fs::remove_file(&tmp);
            return Err(error.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let seq = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        env::temp_dir().join(format!("tsk-config-{label}-{nanos}-{seq}"))
    }

    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn seed(label: &str, contents: &str) -> (PathBuf, TempDirGuard) {
        let dir = temp_dir(label);
        fs::create_dir_all(&dir).expect("mkdir");
        let guard = TempDirGuard(dir.clone());
        fs::write(dir.join(CONFIG_FILE), contents).expect("seed document");
        (dir, guard)
    }

    fn temp_files(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp."))
            .collect()
    }

    fn os(value: &str) -> &OsStr {
        OsStr::new(value)
    }

    #[test]
    fn empty_override_never_resolves_to_the_current_directory() {
        assert_eq!(
            resolve_config_dir(Some(os("")), Some(os("/home/u"))),
            PathBuf::from("/home/u/.tsk"),
            "an empty override must not resolve to the current working directory"
        );
    }

    #[test]
    fn tsk_env_wins_over_home() {
        assert_eq!(
            resolve_config_dir(Some(os("/tsk-env")), Some(os("/home/u"))),
            PathBuf::from("/tsk-env")
        );
    }

    #[test]
    fn empty_tsk_env_falls_through_to_home() {
        assert_eq!(
            resolve_config_dir(Some(os("")), Some(os("/home/u"))),
            PathBuf::from("/home/u/.tsk")
        );
    }

    #[test]
    fn home_resolves_to_the_dot_tsk_store_root() {
        assert_eq!(
            resolve_config_dir(None, Some(os("/home/u"))),
            PathBuf::from("/home/u/.tsk"),
            "config lives beside the board's data in one store root"
        );
    }

    #[test]
    fn empty_home_falls_through_to_the_relative_fallback() {
        assert_eq!(
            resolve_config_dir(None, Some(os(""))),
            PathBuf::from(".tsk-config")
        );
    }

    #[test]
    fn no_candidate_resolves_to_a_relative_dir_never_a_shared_temp_path() {
        let fallback = resolve_config_dir(None, None);

        assert_eq!(fallback, PathBuf::from(".tsk-config"));
        assert!(
            fallback.is_relative(),
            "the last resort must be per-process, not a shared world-writable path"
        );
    }

    #[test]
    fn default_config_dir_reads_the_real_environment() {
        // No environment mutation anywhere in this module's tests: this only asserts the
        // wrapper agrees with the pure resolver for whatever the process environment holds.
        assert_eq!(
            default_config_dir(),
            resolve_config_dir(
                env::var_os("TSK_CONFIG_DIR").as_deref(),
                env::var_os("HOME").as_deref(),
            )
        );
    }

    #[test]
    fn fresh_location_reads_as_not_dismissed() {
        let dir = temp_dir("fresh");
        let _guard = TempDirGuard(dir.clone());

        assert!(!WalkthroughRecord::new(&dir).is_dismissed());
    }

    #[test]
    fn recorded_dismissal_reads_back_as_dismissed() {
        let dir = temp_dir("round-trip");
        let _guard = TempDirGuard(dir.clone());
        let record = WalkthroughRecord::new(&dir);

        record.record_dismissed().expect("record dismissal");

        assert!(WalkthroughRecord::new(&dir).is_dismissed());
        assert!(
            temp_files(&dir).is_empty(),
            "a successful write must leave no temp file behind"
        );
    }

    #[test]
    fn malformed_document_reads_as_not_dismissed() {
        for (label, contents) in [
            ("empty", ""),
            ("not-json", "not json at all"),
            ("missing-field", "{}"),
            ("wrong-type", "{\"dismissed\": \"yes\"}"),
        ] {
            let (dir, _guard) = seed(label, contents);
            assert!(
                !WalkthroughRecord::new(&dir).is_dismissed(),
                "{label} document must read as not dismissed"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_document_reads_as_not_dismissed() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, _guard) = seed("unreadable", "{\"dismissed\": true}");
        let file = dir.join(CONFIG_FILE);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).expect("chmod");
        if fs::read(&file).is_ok() {
            // Running as a user that bypasses permissions (e.g. root). Say so out loud:
            // a silent early return reports green while asserting nothing.
            eprintln!(
                "SKIPPED unreadable_document_reads_as_not_dismissed: \
                 permission bits do not bite for this user"
            );
            return;
        }

        assert!(!WalkthroughRecord::new(&dir).is_dismissed());
    }

    #[cfg(unix)]
    #[test]
    fn unwritable_location_returns_a_presentable_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("unwritable");
        fs::create_dir_all(&dir).expect("mkdir");
        let _guard = TempDirGuard(dir.clone());
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod");
        if fs::write(dir.join("probe"), b"probe").is_ok() {
            // Running as a user that bypasses permissions (e.g. root). Say so out loud:
            // a silent early return reports green while asserting nothing.
            eprintln!(
                "SKIPPED unwritable_location_returns_a_presentable_error: \
                 permission bits do not bite for this user"
            );
            return;
        }

        let error = WalkthroughRecord::new(&dir)
            .record_dismissed()
            .expect_err("an unwritable location must return an error, not panic");

        assert!(
            !error.to_string().is_empty(),
            "the caller must have something to present"
        );
        assert!(
            temp_files(&dir).is_empty(),
            "a failed write must leave no temp file behind"
        );
    }

    #[test]
    fn missing_settings_default_to_ctrl() {
        let dir = temp_dir("settings-fresh");
        let _guard = TempDirGuard(dir.clone());
        assert_eq!(
            SettingsRecord::new(&dir).verb_modifier(),
            VerbModifier::Ctrl
        );
    }

    #[test]
    fn verb_modifier_round_trips() {
        let dir = temp_dir("settings-round-trip");
        let _guard = TempDirGuard(dir.clone());
        let record = SettingsRecord::new(&dir);
        record.set_verb_modifier(VerbModifier::Ctrl).expect("write");
        assert_eq!(
            SettingsRecord::new(&dir).verb_modifier(),
            VerbModifier::Ctrl
        );
    }

    #[test]
    fn legacy_alt_verb_setting_migrates_to_ctrl() {
        let dir = temp_dir("settings-legacy-alt");
        let _guard = TempDirGuard(dir.clone());
        fs::create_dir_all(&dir).expect("settings directory");
        fs::write(dir.join(SETTINGS_FILE), r#"{"verb_modifier":"alt"}"#).expect("legacy settings");

        assert_eq!(
            SettingsRecord::new(&dir).verb_modifier(),
            VerbModifier::Ctrl
        );
    }
}
