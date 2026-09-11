//! Release announcements: one desk notice per upgrade, combining every bundled entry the
//! install has not seen.
//!
//! The catalog is `announcements/catalog.toml` beside this file, edited by the maintainer
//! and compiled in. `delivery.json` keeps the highest id delivered; a fresh install takes
//! the bundled maximum and receives nothing, so a "What's new" row only ever describes an
//! upgrade.

use toml_edit::{DocumentMut, Item, Table};

use crate::delivery;
use crate::domain::{HumanStatus, TaskScope};
use crate::store::TaskStore;

pub const CATALOG_TOML: &str = include_str!("announcements/catalog.toml");
pub const CATALOG_ID_PREFIX: &str = "announce.";
pub const TITLE: &str = "What's new in tsk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub id: u64,
    pub title: String,
    pub notes: String,
}

/// The bundled catalog, ascending by id.
pub fn catalog() -> Result<Vec<Announcement>, String> {
    parse_catalog(CATALOG_TOML)
}

/// Parse a catalog: `[[announcement]]` tables with exactly `id`, `title`, and `notes`.
/// Ids are positive and strictly increasing in file order, titles carry no link.
pub fn parse_catalog(toml: &str) -> Result<Vec<Announcement>, String> {
    let document: DocumentMut = toml.parse().map_err(|error| format!("catalog: {error}"))?;
    if document.len() != 1 {
        return Err("catalog: only [[announcement]] tables are allowed".to_string());
    }
    let tables = document
        .get("announcement")
        .and_then(Item::as_array_of_tables)
        .ok_or("catalog: expected [[announcement]] tables")?;
    let mut entries: Vec<Announcement> = Vec::new();
    for table in tables.iter() {
        let entry = parse_entry(table)?;
        if entries.last().is_some_and(|last| last.id >= entry.id) {
            return Err(format!("catalog: id {} must increase", entry.id));
        }
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err("catalog: no announcements".to_string());
    }
    Ok(entries)
}

fn parse_entry(table: &Table) -> Result<Announcement, String> {
    if table.len() != 3 {
        return Err("catalog: each announcement has exactly id, title, notes".to_string());
    }
    let id = table
        .get("id")
        .and_then(Item::as_integer)
        .filter(|id| *id > 0)
        .ok_or("catalog: id must be a positive integer")?;
    let text = |key: &str| {
        table
            .get(key)
            .and_then(Item::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("catalog: announcement {id} needs a non-empty {key}"))
    };
    let title = text("title")?;
    if title.contains("http") {
        return Err(format!(
            "catalog: announcement {id} title carries a link; links go in notes"
        ));
    }
    Ok(Announcement {
        id: id as u64,
        title,
        notes: text("notes")?,
    })
}

/// Seed the bundled catalog. Returns how many notice rows were created (0 or 1).
pub fn seed_on_open(store: &TaskStore) -> Result<usize, String> {
    seed(store, &catalog()?)
}

/// Deliver every entry newer than what this install has seen as one notice, then record
/// the bundled maximum as delivered.
///
/// What the install has seen is the delivery watermark or, when a lost record leaves it
/// behind, the highest `announce.<id>` notice already in the store. Nothing seen means a
/// fresh install: the watermark jumps to the bundled maximum and no row is created.
fn seed(store: &TaskStore, catalog: &[Announcement]) -> Result<usize, String> {
    let Some(bundled) = catalog.last().map(|entry| entry.id) else {
        return Ok(0);
    };
    let dir = store.path();
    let mut record = delivery::load(dir);
    if record.announcement_watermark >= bundled {
        return Ok(0);
    }
    let created = store.locked_transition_if_changed(|state| {
        let seen = state
            .tasks()
            .iter()
            .filter_map(|task| task.notice.as_ref())
            .filter_map(|notice| notice.catalog_id.strip_prefix(CATALOG_ID_PREFIX))
            .filter_map(|id| id.parse::<u64>().ok())
            .fold(record.announcement_watermark, u64::max);
        if seen == 0 || seen >= bundled {
            return Ok((0, false));
        }
        let missed = catalog.iter().filter(|entry| entry.id > seen);
        state
            .create_notice(
                format!("{CATALOG_ID_PREFIX}{bundled}"),
                TITLE,
                Some(combined_notes(missed)),
                HumanStatus::Ready,
                TaskScope::Global,
                Vec::new(),
            )
            .map_err(|error| error.to_string())?;
        Ok((1, true))
    })?;
    record.announcement_watermark = bundled;
    delivery::save(dir, &record).map_err(|error| error.to_string())?;
    Ok(created)
}

/// Newest entry first, each under its own heading.
fn combined_notes<'a>(missed: impl DoubleEndedIterator<Item = &'a Announcement>) -> String {
    missed
        .rev()
        .map(|entry| format!("## {}\n\n{}", entry.title, entry.notes))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::domain::{DomainState, Task};
    use crate::guides;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_store(label: &str) -> TaskStore {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "tsk-announcements-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        TaskStore::new(dir)
    }

    fn announcement(id: u64) -> Announcement {
        Announcement {
            id,
            title: format!("Release {id}"),
            notes: format!("Notes for release {id}.\n\nChangelog: https://example.test/v{id}"),
        }
    }

    fn announced(state: &DomainState) -> Vec<&Task> {
        state
            .tasks()
            .iter()
            .filter(|task| {
                task.notice
                    .as_ref()
                    .is_some_and(|notice| notice.catalog_id.starts_with(CATALOG_ID_PREFIX))
            })
            .collect()
    }

    fn set_watermark(store: &TaskStore, watermark: u64) {
        let mut record = delivery::load(store.path());
        record.announcement_watermark = watermark;
        delivery::save(store.path(), &record).expect("save record");
    }

    #[test]
    fn the_bundled_catalog_parses_and_ends_with_a_changelog_link() {
        let entries = catalog().expect("bundled catalog");
        assert_eq!(entries[0].id, 1);
        for entry in &entries {
            assert!(
                entry
                    .notes
                    .contains("Changelog: https://github.com/smarzban/herdr-tsk/"),
                "{}",
                entry.id
            );
            assert!(!entry.notes.ends_with('\n'));
        }
    }

    #[test]
    fn a_fresh_install_takes_the_bundled_watermark_and_no_announcement_row() {
        let store = temp_store("fresh");
        assert_eq!(guides::seed_on_open(&store), Ok(5));
        assert_eq!(seed(&store, &[announcement(1), announcement(2)]), Ok(0));
        let state = store.load().expect("load");
        assert_eq!(state.tasks().iter().filter(|t| t.is_notice()).count(), 5);
        assert!(announced(&state).is_empty());
        let record = delivery::load(store.path());
        assert_eq!(record.announcement_watermark, 2);
        assert_eq!(record.guides.len(), 5, "the guide marks survive");
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn two_missed_entries_become_one_notice_newest_first_and_a_second_open_adds_none() {
        let store = temp_store("missed");
        set_watermark(&store, 1);
        let catalog = [announcement(1), announcement(2), announcement(3)];
        assert_eq!(seed(&store, &catalog), Ok(1));
        let state = store.load().expect("load");
        let rows = announced(&state);
        assert_eq!(rows.len(), 1);
        let row = rows[0];
        assert_eq!(row.title, TITLE);
        assert_eq!(row.status, HumanStatus::Ready);
        assert_eq!(row.scope, TaskScope::Global);
        assert_eq!(row.board_identifier(), Some("N1".to_string()));
        assert_eq!(row.notice.as_ref().unwrap().catalog_id, "announce.3");
        assert_eq!(
            row.notes.as_deref(),
            Some(
                "## Release 3\n\nNotes for release 3.\n\nChangelog: https://example.test/v3\n\n\
                 ## Release 2\n\nNotes for release 2.\n\nChangelog: https://example.test/v2"
            )
        );
        assert_eq!(delivery::load(store.path()).announcement_watermark, 3);

        assert_eq!(seed(&store, &catalog), Ok(0));
        assert_eq!(announced(&store.load().expect("reload")).len(), 1);
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn a_gap_in_ids_still_combines_everything_above_the_watermark() {
        let store = temp_store("gap");
        set_watermark(&store, 1);
        assert_eq!(
            seed(&store, &[announcement(1), announcement(3), announcement(5)]),
            Ok(1)
        );
        let state = store.load().expect("load");
        let notes = announced(&state)[0].notes.clone().unwrap();
        assert!(notes.starts_with("## Release 5\n"));
        assert!(notes.contains("## Release 3\n"));
        assert!(!notes.contains("Release 1"));
        assert_eq!(delivery::load(store.path()).announcement_watermark, 5);
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn a_dismissed_notice_never_comes_back_even_after_it_leaves_the_store() {
        let store = temp_store("dismiss");
        set_watermark(&store, 1);
        let catalog = [announcement(1), announcement(2)];
        assert_eq!(seed(&store, &catalog), Ok(1));
        let mut state = store.load().expect("load");
        let id = announced(&state)[0].id;
        state.complete(id).expect("complete");
        store.save(&state).expect("save");
        delivery::record_dismissed_notices(store.path(), state.tasks()).expect("record");
        assert!(delivery::load(store.path()).guides.contains("announce.2"));

        store.save(&DomainState::new()).expect("forget the row");
        assert_eq!(seed(&store, &catalog), Ok(0));
        assert!(announced(&store.load().expect("reload")).is_empty());
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn a_lost_record_is_rebuilt_from_the_store_and_only_newer_entries_are_delivered() {
        let store = temp_store("lost");
        set_watermark(&store, 1);
        assert_eq!(seed(&store, &[announcement(1), announcement(2)]), Ok(1));
        fs::remove_file(store.path().join(delivery::DELIVERY_FILE)).expect("lose record");

        assert_eq!(seed(&store, &[announcement(1), announcement(2)]), Ok(0));
        assert_eq!(delivery::load(store.path()).announcement_watermark, 2);
        assert_eq!(announced(&store.load().expect("load")).len(), 1);

        fs::remove_file(store.path().join(delivery::DELIVERY_FILE)).expect("lose again");
        let newer = [announcement(1), announcement(2), announcement(3)];
        assert_eq!(seed(&store, &newer), Ok(1));
        let state = store.load().expect("reload");
        let rows = announced(&state);
        assert_eq!(rows.len(), 2);
        let notes = rows[1].notes.clone().unwrap();
        assert!(notes.contains("Release 3") && !notes.contains("Release 2"));
        assert_eq!(delivery::load(store.path()).announcement_watermark, 3);
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn a_bad_catalog_fails_to_parse() {
        let good = "[[announcement]]\nid = 1\ntitle = \"a\"\nnotes = \"b\"\n";
        assert_eq!(parse_catalog(good).map(|c| c.len()), Ok(1));
        let bad = [
            ("not toml", "[[announcement]\n"),
            ("no entries", "announcement = []\n"),
            (
                "missing id",
                "[[announcement]]\ntitle = \"a\"\nnotes = \"b\"\n",
            ),
            (
                "zero id",
                "[[announcement]]\nid = 0\ntitle = \"a\"\nnotes = \"b\"\n",
            ),
            (
                "string id",
                "[[announcement]]\nid = \"1\"\ntitle = \"a\"\nnotes = \"b\"\n",
            ),
            (
                "empty notes",
                "[[announcement]]\nid = 1\ntitle = \"a\"\nnotes = \"  \"\n",
            ),
            (
                "unknown key",
                "[[announcement]]\nid = 1\ntitle = \"a\"\nnotes = \"b\"\nversion = \"x\"\n",
            ),
            (
                "link in title",
                "[[announcement]]\nid = 1\ntitle = \"see https://x\"\nnotes = \"b\"\n",
            ),
            (
                "ids out of order",
                "[[announcement]]\nid = 2\ntitle = \"a\"\nnotes = \"b\"\n\
                 [[announcement]]\nid = 1\ntitle = \"c\"\nnotes = \"d\"\n",
            ),
            (
                "duplicate id",
                "[[announcement]]\nid = 1\ntitle = \"a\"\nnotes = \"b\"\n\
                 [[announcement]]\nid = 1\ntitle = \"c\"\nnotes = \"d\"\n",
            ),
            (
                "stray top-level key",
                "title = \"x\"\n[[announcement]]\nid = 1\ntitle = \"a\"\nnotes = \"b\"\n",
            ),
        ];
        for (label, toml) in bad {
            assert!(parse_catalog(toml).is_err(), "{label} should fail");
        }
    }
}
