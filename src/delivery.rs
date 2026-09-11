use std::collections::BTreeSet;
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::domain::{HumanStatus, Task};
use crate::fsperm;
use crate::store::{AtomicFilesystem, StdFilesystem, TaskStore};

pub const DELIVERY_FILE: &str = "delivery.json";
pub const DELIVERY_TEMP_PREFIX: &str = ".delivery.json.tmp.";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliveryDocument {
    /// Guide catalog ids this state dir has seeded or dismissed. Never seeded again.
    pub guides: BTreeSet<String>,
    /// Highest bundled announcement catalog id already delivered. Stays 0 until the
    /// announcement seeder writes it.
    pub announcement_watermark: u64,
}

/// The record on disk; a missing or unreadable file reads as nothing delivered.
pub fn load(dir: &Path) -> DeliveryDocument {
    fs::read_to_string(dir.join(DELIVERY_FILE))
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

/// Persist the record atomically: private temp file in the same directory, synced,
/// renamed over the target, and the containing directory synced so the rename itself
/// survives a power loss — the same durable stages the store document uses. The record
/// decides whether a fresh install suppresses bundled announcements, so a lost rename
/// must not be able to resurface them.
pub fn save(dir: &Path, document: &DeliveryDocument) -> io::Result<()> {
    save_with(&StdFilesystem, dir, document)
}

/// [`save`] over an injectable filesystem, so tests can verify the stage order and
/// failure propagation the durability claim rests on.
pub(crate) fn save_with<F: AtomicFilesystem>(
    filesystem: &F,
    dir: &Path,
    document: &DeliveryDocument,
) -> io::Result<()> {
    fsperm::ensure_private_dir(dir)?;
    let target = dir.join(DELIVERY_FILE);
    let tmp = unique_tmp_path(dir);
    let data = serde_json::to_string(document).map_err(io::Error::other)?;
    let write_result = (|| -> io::Result<()> {
        let mut temp_file = filesystem.create_file(&tmp)?;
        filesystem.write_all(&mut temp_file, data.as_bytes())?;
        filesystem.sync_file(&temp_file)?;
        drop(temp_file);
        filesystem.rename(&tmp, &target)?;
        filesystem.sync_directory(dir)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = filesystem.remove_file(&tmp);
    }
    write_result
}

/// A notice the board verbs have taken off the working board: done (`ctrl+d`),
/// archived (`ctrl+f`), or deleted (`ctrl+x`). One rule for all three.
pub fn is_dismissed(task: &Task) -> bool {
    task.is_notice() && (task.status == HumanStatus::Done || task.archived || task.soft_deleted)
}

/// Mark every dismissed notice's catalog id delivered so no later open seeds it again.
/// The read-modify-write runs under the store's exclusive lock, so a dismissal cannot
/// be saved from a record that went stale mid-write and lose a concurrent process's
/// guide marks or announcement watermark. Reads and writes nothing when no notice is
/// dismissed or every one is already marked.
pub fn record_dismissed_notices(store: &TaskStore, tasks: &[Task]) -> io::Result<()> {
    let dismissed: BTreeSet<&str> = tasks
        .iter()
        .filter(|task| is_dismissed(task))
        .filter_map(|task| task.notice.as_ref())
        .map(|notice| notice.catalog_id.as_str())
        .collect();
    if dismissed.is_empty() {
        return Ok(());
    }
    store.locked_delivery_update(|document| {
        let before = document.guides.len();
        document
            .guides
            .extend(dismissed.iter().map(|catalog_id| (*catalog_id).to_string()));
        document.guides.len() != before
    })
}

fn unique_tmp_path(dir: &Path) -> PathBuf {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!("{DELIVERY_TEMP_PREFIX}{pid}.{nanos}"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::domain::{DomainState, ProvenanceOrigin, TaskScope};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tsk-delivery-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn notice(state: &mut DomainState, catalog_id: &str) -> uuid::Uuid {
        state
            .create_notice(
                catalog_id,
                catalog_id,
                None,
                HumanStatus::Ready,
                TaskScope::Global,
                Vec::new(),
            )
            .expect("notice")
    }

    #[test]
    fn missing_file_reads_empty_and_a_saved_record_reads_back() {
        let dir = temp_dir("roundtrip");
        assert_eq!(load(&dir), DeliveryDocument::default());
        let document = DeliveryDocument {
            guides: ["guide.a".to_string(), "guide.b".to_string()]
                .into_iter()
                .collect(),
            announcement_watermark: 0,
        };
        save(&dir, &document).expect("save");
        assert_eq!(load(&dir), document);
        assert_eq!(
            fs::read_to_string(dir.join(DELIVERY_FILE)).expect("file"),
            r#"{"guides":["guide.a","guide.b"],"announcement_watermark":0}"#
        );
        assert!(
            fs::read_dir(&dir)
                .expect("dir")
                .flatten()
                .all(|entry| entry.file_name() == DELIVERY_FILE),
            "no temp file is left beside the record"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_record_with_only_the_watermark_keeps_it_and_reads_no_guides() {
        let dir = temp_dir("watermark");
        fs::write(dir.join(DELIVERY_FILE), r#"{"announcement_watermark":7}"#).expect("write");
        assert_eq!(
            load(&dir),
            DeliveryDocument {
                guides: BTreeSet::new(),
                announcement_watermark: 7,
            }
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dismissed_notices_are_recorded_once_and_ordinary_or_live_rows_are_not() {
        let dir = temp_dir("dismiss");
        let mut state = DomainState::new();
        let done = notice(&mut state, "guide.done");
        let archived = notice(&mut state, "guide.archived");
        let deleted = notice(&mut state, "guide.deleted");
        let _live = notice(&mut state, "guide.live");
        let ordinary = state
            .create(
                "ordinary",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("task");
        state.complete(done).expect("complete");
        state.archive_task(archived).expect("archive");
        state.soft_delete(deleted).expect("delete");
        state.complete(ordinary).expect("complete ordinary");

        record_dismissed_notices(&TaskStore::new(&dir), state.tasks()).expect("record");
        let expected: BTreeSet<String> = ["guide.done", "guide.archived", "guide.deleted"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(load(&dir).guides, expected);

        fs::remove_file(dir.join(DELIVERY_FILE)).expect("remove");
        record_dismissed_notices(&TaskStore::new(&dir), state.tasks()).expect("record again");
        assert_eq!(load(&dir).guides, expected, "a lost record is rebuilt");

        save(
            &dir,
            &DeliveryDocument {
                guides: ["guide.other".to_string()].into_iter().collect(),
                announcement_watermark: 3,
            },
        )
        .expect("seed");
        record_dismissed_notices(&TaskStore::new(&dir), state.tasks()).expect("extend");
        let merged = load(&dir);
        assert_eq!(merged.announcement_watermark, 3, "the watermark survives");
        assert!(
            merged.guides.contains("guide.other"),
            "existing marks survive"
        );
        assert!(merged.guides.is_superset(&expected));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_dismissed_notice_writes_no_file() {
        let dir = temp_dir("untouched");
        let mut state = DomainState::new();
        let _ = notice(&mut state, "guide.live");
        record_dismissed_notices(&TaskStore::new(&dir), state.tasks()).expect("record");
        assert!(!dir.join(DELIVERY_FILE).exists());
        let _ = fs::remove_dir_all(dir);
    }

    /// Mirrors the store document's durability order: a saved record is successful only
    /// after the temp write, the file sync, the rename, and the containing-directory sync
    /// have all run, and any stage failure cleans the temp up and is reported.
    #[test]
    fn save_orders_durable_stages_and_propagates_every_stage_failure() {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Stage {
            Write,
            FileSync,
            Rename,
            DirectorySync,
        }

        struct RecordingFilesystem {
            events: std::cell::RefCell<Vec<Stage>>,
            fail_at: Option<Stage>,
        }

        impl RecordingFilesystem {
            fn new(fail_at: Option<Stage>) -> Self {
                Self {
                    events: std::cell::RefCell::new(Vec::new()),
                    fail_at,
                }
            }

            fn record(&self, stage: Stage) -> io::Result<()> {
                self.events.borrow_mut().push(stage);
                if self.fail_at == Some(stage) {
                    return Err(io::Error::other("injected failure"));
                }
                Ok(())
            }
        }

        impl AtomicFilesystem for RecordingFilesystem {
            type File = ();

            fn create_file(&self, _path: &Path) -> io::Result<Self::File> {
                Ok(())
            }

            fn write_all(&self, _file: &mut Self::File, _data: &[u8]) -> io::Result<()> {
                self.record(Stage::Write)
            }

            fn sync_file(&self, _file: &Self::File) -> io::Result<()> {
                self.record(Stage::FileSync)
            }

            fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
                self.record(Stage::Rename)
            }

            fn sync_directory(&self, _path: &Path) -> io::Result<()> {
                self.record(Stage::DirectorySync)
            }

            fn remove_file(&self, _path: &Path) -> io::Result<()> {
                Ok(())
            }
        }

        let dir = temp_dir("durability-order");
        let document = DeliveryDocument::default();
        let expected = [
            Stage::Write,
            Stage::FileSync,
            Stage::Rename,
            Stage::DirectorySync,
        ];

        let filesystem = RecordingFilesystem::new(None);
        save_with(&filesystem, &dir, &document).expect("all durable stages succeed");
        assert_eq!(filesystem.events.borrow().clone(), expected);

        for (index, stage) in expected.iter().copied().enumerate() {
            let filesystem = RecordingFilesystem::new(Some(stage));
            let error = save_with(&filesystem, &dir, &document);
            assert!(error.is_err(), "{stage:?} failure must be reported");
            assert_eq!(filesystem.events.borrow().clone(), expected[..=index]);
        }
        let _ = fs::remove_dir_all(dir);
    }
}
