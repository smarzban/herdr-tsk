use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::domain::{HumanStatus, Task};
use crate::fsperm;

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

pub fn save(dir: &Path, document: &DeliveryDocument) -> io::Result<()> {
    fsperm::ensure_private_dir(dir)?;
    let target = dir.join(DELIVERY_FILE);
    let tmp = unique_tmp_path(dir);
    let data = serde_json::to_string(document).map_err(io::Error::other)?;
    let write_result = (|| -> io::Result<()> {
        let mut temp_file = fsperm::create_private_file(&tmp)?;
        temp_file.write_all(data.as_bytes())?;
        temp_file.sync_all()?;
        drop(temp_file);
        fs::rename(&tmp, &target)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    write_result
}

/// A notice the board verbs have taken off the working board: done (`ctrl+d`),
/// archived (`ctrl+f`), or deleted (`ctrl+x`). One rule for all three.
pub fn is_dismissed(task: &Task) -> bool {
    task.is_notice() && (task.status == HumanStatus::Done || task.archived || task.soft_deleted)
}

/// Mark every dismissed notice's catalog id delivered so no later open seeds it again.
/// Reads and writes nothing when no notice is dismissed or every one is already marked.
pub fn record_dismissed_notices(dir: &Path, tasks: &[Task]) -> io::Result<()> {
    let dismissed: BTreeSet<&str> = tasks
        .iter()
        .filter(|task| is_dismissed(task))
        .filter_map(|task| task.notice.as_ref())
        .map(|notice| notice.catalog_id.as_str())
        .collect();
    if dismissed.is_empty() {
        return Ok(());
    }
    let mut document = load(dir);
    let before = document.guides.len();
    document
        .guides
        .extend(dismissed.into_iter().map(str::to_string));
    if document.guides.len() == before {
        return Ok(());
    }
    save(dir, &document)
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

        record_dismissed_notices(&dir, state.tasks()).expect("record");
        let expected: BTreeSet<String> = ["guide.done", "guide.archived", "guide.deleted"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(load(&dir).guides, expected);

        fs::remove_file(dir.join(DELIVERY_FILE)).expect("remove");
        record_dismissed_notices(&dir, state.tasks()).expect("record again");
        assert_eq!(load(&dir).guides, expected, "a lost record is rebuilt");

        save(
            &dir,
            &DeliveryDocument {
                guides: ["guide.other".to_string()].into_iter().collect(),
                announcement_watermark: 3,
            },
        )
        .expect("seed");
        record_dismissed_notices(&dir, state.tasks()).expect("extend");
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
        record_dismissed_notices(&dir, state.tasks()).expect("record");
        assert!(!dir.join(DELIVERY_FILE).exists());
        let _ = fs::remove_dir_all(dir);
    }
}
