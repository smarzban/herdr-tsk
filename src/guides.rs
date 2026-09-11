use std::collections::BTreeSet;

use crate::delivery;
use crate::domain::{HumanStatus, TaskScope};
use crate::store::TaskStore;

#[derive(Debug, Clone, Copy)]
pub struct Guide {
    pub catalog_id: &'static str,
    pub title: &'static str,
    pub notes: &'static str,
    pub status: HumanStatus,
    pub steps: &'static [(&'static str, bool)],
}

pub const CATALOG: [Guide; 5] = [
    Guide {
        catalog_id: "guide.start-here",
        title: "Start here: Enter opens a task, Esc comes back",
        notes: "These `N` rows are guides. They sit on your desk like tasks and leave like tasks.\n\
                \n\
                - `j` `k` or the arrows move the selection\n\
                - `Enter` opens the selected task, `Esc` returns to the board\n\
                - `?` lists every key\n\
                \n\
                Review rows wait in NEEDS YOU until you decide. `ctrl+r` sends this one to ON DECK.",
        status: HumanStatus::Review,
        steps: &[
            ("Find this guide in NEEDS YOU", true),
            ("Press Enter on this step to check it", false),
            ("Press Esc to return to the board", false),
        ],
    },
    Guide {
        catalog_id: "guide.needs-you",
        title: "Blocked rows wait in NEEDS YOU on every board",
        notes: "NEEDS YOU collects blocked and review tasks from every project, so the desk \
                shows what waits on you.\n\
                \n\
                - `ctrl+b` toggles blocked, `ctrl+r` toggles review\n\
                - `1` desk · `2` selected project · `3` projects\n\
                - `p` picks a project\n\
                \n\
                Unblock this guide with `ctrl+b` and watch it move to ON DECK.",
        status: HumanStatus::Blocked,
        steps: &[("Press ctrl+b to unblock this guide", false)],
    },
    Guide {
        catalog_id: "guide.in-motion",
        title: "Started rows sit in IN MOTION, steps never finish a task",
        notes: "`ctrl+s` starts a task. Steps are a checklist inside it: check them all and the \
                task still reads started, because only you set status.\n\
                \n\
                - `Enter` on a step checks it\n\
                - the `+ step` row at the end of the list adds one\n\
                - `ctrl+d` marks the task done, `ctrl+o` reopens it",
        status: HumanStatus::Started,
        steps: &[
            ("Check this step with Enter", false),
            ("Check this one too", false),
            ("Notice the task is still started", false),
        ],
    },
    Guide {
        catalog_id: "guide.on-deck",
        title: "Ready rows live in ON DECK until ctrl+s",
        notes: "`+` adds a task to ON DECK. Type a title and press `Enter`; `Tab` expands the \
                draft into a full page first.\n\
                \n\
                - `!p name` in the title files the task under a project, bare `!p` keeps it on the desk\n\
                - `!t name` puts it on a thread\n\
                - `ctrl+e` edits the open task, `ctrl+n` its notes\n\
                \n\
                Agents add and update tasks with `tsk add`, `tsk list`, and `tsk status`. \
                `tsk guide` prints their instructions.",
        status: HumanStatus::Ready,
        steps: &[
            ("Press + and add a task of your own", false),
            ("Press ctrl+s on it", false),
        ],
    },
    Guide {
        catalog_id: "guide.dismiss",
        title: "Archive or delete a guide when you are done with it",
        notes: "Guides are yours to clear. Any of these takes one off the board for good:\n\
                \n\
                - `ctrl+d` marks it done, into the `d` drawer\n\
                - `ctrl+f` files it in the drawer's archived group\n\
                - `ctrl+x` twice deletes it, `ctrl+u` undoes\n\
                \n\
                A cleared guide never comes back, and `tsk list` never shows guides to agents.",
        status: HumanStatus::Ready,
        steps: &[("Press ctrl+d on this guide", false)],
    },
];

/// Seed every guide this state dir has not delivered, then record the whole catalog as
/// delivered. Returns how many notice rows were created.
///
/// The record is read and written under the store's exclusive lock beside the state
/// transition, so this process's stale record cannot overwrite a concurrent process's
/// dismissal or watermark write. Marks only ever accumulate, so a record already naming
/// every catalog id skips the lock entirely.
pub fn seed_on_open(store: &TaskStore) -> Result<usize, String> {
    if CATALOG.iter().all(|guide| {
        delivery::load(store.path())
            .guides
            .contains(guide.catalog_id)
    }) {
        return Ok(0);
    }
    store.locked_transition_with_delivery(|state, record| {
        let pending: Vec<&Guide> = CATALOG
            .iter()
            .filter(|guide| !record.guides.contains(guide.catalog_id))
            .collect();
        if pending.is_empty() {
            return Ok((0, false, false));
        }
        let present: BTreeSet<String> = state
            .tasks()
            .iter()
            .filter_map(|task| task.notice.as_ref())
            .map(|notice| notice.catalog_id.clone())
            .collect();
        let mut created = 0;
        for guide in pending
            .iter()
            .filter(|guide| !present.contains(guide.catalog_id))
        {
            state
                .create_notice(
                    guide.catalog_id,
                    guide.title,
                    Some(guide.notes.to_string()),
                    guide.status,
                    TaskScope::Global,
                    guide
                        .steps
                        .iter()
                        .map(|(text, done)| (text.to_string(), *done))
                        .collect(),
                )
                .map_err(|error| error.to_string())?;
            created += 1;
        }
        record
            .guides
            .extend(pending.iter().map(|guide| guide.catalog_id.to_string()));
        Ok((created, created > 0, true))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::domain::{DomainState, ProvenanceOrigin, Task};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_store(label: &str) -> TaskStore {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "tsk-guides-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        TaskStore::new(dir)
    }

    fn notices(state: &DomainState) -> Vec<&Task> {
        state
            .tasks()
            .iter()
            .filter(|task| task.is_notice())
            .collect()
    }

    fn catalog_ids() -> BTreeSet<String> {
        CATALOG
            .iter()
            .map(|guide| guide.catalog_id.to_string())
            .collect()
    }

    fn replace_store_with_ordinary_task_only(store: &TaskStore) {
        let mut forgetting = DomainState::new();
        forgetting
            .create(
                "unrelated",
                None,
                TaskScope::Global,
                ProvenanceOrigin::Manual,
                None,
            )
            .expect("task");
        store.save(&forgetting).expect("save");
    }

    #[test]
    fn first_open_seeds_five_desk_notices_and_a_second_open_adds_none() {
        let store = temp_store("empty");
        assert_eq!(seed_on_open(&store), Ok(5));
        let state = store.load().expect("load");
        let seeded = notices(&state);
        assert_eq!(
            seeded
                .iter()
                .map(|task| task.board_identifier())
                .collect::<Vec<_>>(),
            (1..=5).map(|n| Some(format!("N{n}"))).collect::<Vec<_>>()
        );
        assert!(seeded.iter().all(|task| task.scope == TaskScope::Global));
        assert!(seeded.iter().all(|task| task.number.is_none()));
        let start = seeded[0];
        assert_eq!(
            start.title,
            "Start here: Enter opens a task, Esc comes back"
        );
        assert_eq!(start.status, HumanStatus::Review);
        assert_eq!(
            start.steps.iter().map(|step| step.done).collect::<Vec<_>>(),
            vec![true, false, false]
        );
        assert!(start
            .notes
            .as_deref()
            .is_some_and(|notes| notes.contains("`Esc`")));
        assert_eq!(delivery::load(store.path()).guides, catalog_ids());

        assert_eq!(seed_on_open(&store), Ok(0));
        assert_eq!(notices(&store.load().expect("reload")).len(), 5);
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn an_existing_store_with_ordinary_tasks_gains_guides_and_keeps_its_t_numbers() {
        let store = temp_store("existing");
        let mut state = DomainState::new();
        for title in ["first", "second"] {
            state
                .create(
                    title,
                    None,
                    TaskScope::Global,
                    ProvenanceOrigin::Manual,
                    None,
                )
                .expect("task");
        }
        store.save(&state).expect("save");

        assert_eq!(seed_on_open(&store), Ok(5));
        let state = store.load().expect("load");
        let ordinary: Vec<Option<u64>> = state
            .tasks()
            .iter()
            .filter(|task| !task.is_notice())
            .map(|task| task.number)
            .collect();
        assert_eq!(ordinary, vec![Some(1), Some(2)]);
        assert_eq!(state.next_task_number, 3);
        assert_eq!(notices(&state).len(), 5);
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn a_dismissed_guide_is_not_seeded_again_even_after_it_leaves_the_store() {
        let store = temp_store("dismiss");
        assert_eq!(seed_on_open(&store), Ok(5));
        let mut state = store.load().expect("load");
        let ids: Vec<uuid::Uuid> = notices(&state).iter().map(|task| task.id).collect();
        state.complete(ids[0]).expect("complete");
        state.archive_task(ids[1]).expect("archive");
        state.soft_delete(ids[2]).expect("delete");
        store.save(&state).expect("save");
        delivery::record_dismissed_notices(&store, state.tasks()).expect("record");

        replace_store_with_ordinary_task_only(&store);
        assert_eq!(seed_on_open(&store), Ok(0));
        assert!(notices(&store.load().expect("reload")).is_empty());
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn a_lost_delivery_record_is_rebuilt_from_the_store_without_duplicates() {
        let store = temp_store("crash");
        assert_eq!(seed_on_open(&store), Ok(5));
        fs::remove_file(store.path().join(delivery::DELIVERY_FILE)).expect("simulate crash");

        assert_eq!(seed_on_open(&store), Ok(0));
        let state = store.load().expect("load");
        let ids: BTreeSet<String> = notices(&state)
            .iter()
            .map(|task| task.notice.as_ref().expect("notice").catalog_id.clone())
            .collect();
        assert_eq!(notices(&state).len(), 5, "no catalog id is created twice");
        assert_eq!(ids, catalog_ids());
        assert_eq!(delivery::load(store.path()).guides, catalog_ids());
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn a_new_catalog_row_reaches_a_store_that_already_has_the_others() {
        let store = temp_store("partial");
        let mut record = delivery::DeliveryDocument::default();
        record.guides.extend(
            CATALOG
                .iter()
                .skip(1)
                .map(|guide| guide.catalog_id.to_string()),
        );
        delivery::save(store.path(), &record).expect("save record");

        assert_eq!(seed_on_open(&store), Ok(1));
        let state = store.load().expect("load");
        let seeded = notices(&state);
        assert_eq!(seeded.len(), 1);
        assert_eq!(
            seeded[0].notice.as_ref().map(|n| n.catalog_id.as_str()),
            Some(CATALOG[0].catalog_id)
        );
        assert_eq!(delivery::load(store.path()).guides, catalog_ids());
        let _ = fs::remove_dir_all(store.path());
    }

    #[test]
    fn catalog_rows_have_unique_ids_and_paintable_text() {
        let ids: BTreeSet<&str> = CATALOG.iter().map(|guide| guide.catalog_id).collect();
        assert_eq!(ids.len(), CATALOG.len());
        for guide in &CATALOG {
            assert!(
                guide.catalog_id.starts_with("guide."),
                "{}",
                guide.catalog_id
            );
            assert!(!guide.notes.ends_with('\n'), "{}", guide.catalog_id);
            assert!(
                guide.steps.iter().all(|(text, _)| !text.trim().is_empty()),
                "{}",
                guide.catalog_id
            );
        }
    }
}
