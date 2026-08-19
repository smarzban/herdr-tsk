//! Queue Section Query: pure derivation of the board sections from a task snapshot.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::Path;

use uuid::Uuid;

use crate::domain::{HumanStatus, Task, TaskScope};

/// Which list region a section belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    InMotion,
    OnDeck,
    Done,
}

/// Session filter applied to every visible queue section.
///
/// `All` shows every task. `Project` / `Global` narrow IN MOTION, ON DECK, and
/// the done drawer to one scope (emitting an empty-hint deck section when that
/// scope has no open tasks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeckScope<'a> {
    #[default]
    All,
    Global,
    Project(&'a Path),
}

/// One ordered section of the queue board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSection {
    pub kind: SectionKind,
    /// Project path for an ON DECK project group. `None` for IN MOTION, DONE, and the global ON DECK group.
    pub project_label: Option<String>,
    pub task_ids: Vec<Uuid>,
    pub count: usize,
    /// True when a scoped deck group has zero open tasks (renderer paints the empty hint; no invented ids).
    pub empty_hint: bool,
}

/// Status-line counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusCounts {
    pub in_motion: usize,
    pub done: usize,
    pub need: usize,
}

/// Ordered sections plus status-line counts for one snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueView {
    pub sections: Vec<QueueSection>,
    pub counts: StatusCounts,
}

/// Derive the queue sections from a task snapshot.
///
/// Pure: no I/O, no clock. Soft-deleted tasks never appear. Under `DeckScope::All`, an empty
/// snapshot yields empty sections. A selected scope filters IN MOTION, ON DECK, the done drawer,
/// and their status-line counts.
pub fn query(
    tasks: &[Task],
    current_repo: Option<&Path>,
    scope: DeckScope<'_>,
    drawer_open: bool,
) -> QueueView {
    // SHORTCUT: linear scan + sort -- fine to ~1k tasks; index when the board can grow unbounded.
    let live: Vec<&Task> = tasks
        .iter()
        .filter(|task| !task.soft_deleted && task_matches_scope(task, scope))
        .collect();

    let mut motion: Vec<&Task> = live
        .iter()
        .copied()
        .filter(|t| t.status == HumanStatus::Started)
        .collect();
    sort_by_updated_desc(&mut motion);

    let mut done: Vec<&Task> = live
        .iter()
        .copied()
        .filter(|t| t.status == HumanStatus::Done)
        .collect();
    sort_by_updated_desc(&mut done);

    // ON DECK: non-done, non-doing (todo / blocked / review), grouped by project.
    let mut project_groups: BTreeMap<&str, Vec<&Task>> = BTreeMap::new();
    let mut global: Vec<&Task> = Vec::new();
    for task in live
        .iter()
        .copied()
        .filter(|t| !matches!(t.status, HumanStatus::Started | HumanStatus::Done))
    {
        match &task.scope {
            TaskScope::Project { path } => {
                project_groups.entry(path.as_str()).or_default().push(task)
            }
            TaskScope::Global => global.push(task),
        }
    }
    for group in project_groups.values_mut() {
        sort_by_updated_desc(group);
    }
    sort_by_updated_desc(&mut global);

    let mut sections = Vec::new();

    if !motion.is_empty() {
        sections.push(section_from(SectionKind::InMotion, None, &motion));
    }

    match scope {
        DeckScope::All => {
            let mut project_order: Vec<&str> = project_groups.keys().copied().collect();
            if let Some(repo) = current_repo {
                if let Some(pos) = project_order.iter().position(|p| Path::new(*p) == repo) {
                    let current = project_order.remove(pos);
                    project_order.insert(0, current);
                }
            }
            // Remaining keys stay alphabetical via BTreeMap; current (if any) was moved to front.
            for path in project_order {
                let group = &project_groups[path];
                if !group.is_empty() {
                    sections.push(section_from(
                        SectionKind::OnDeck,
                        Some(path.to_string()),
                        group,
                    ));
                }
            }
            if !global.is_empty() {
                sections.push(section_from(SectionKind::OnDeck, None, &global));
            }
        }
        DeckScope::Project(path) => {
            let (label, group) = project_groups
                .iter()
                .find(|(stored_path, _)| Path::new(*stored_path) == path)
                .map(|(stored_path, group)| ((*stored_path).to_string(), group.as_slice()))
                .unwrap_or_else(|| (path.to_string_lossy().into_owned(), &[]));
            sections.push(deck_section(Some(label), group));
        }
        DeckScope::Global => {
            sections.push(deck_section(None, &global));
        }
    }

    if drawer_open && !done.is_empty() {
        sections.push(section_from(SectionKind::Done, None, &done));
    }

    QueueView {
        sections,
        counts: StatusCounts {
            in_motion: motion.len(),
            done: done.len(),
            need: 0,
        },
    }
}

fn task_matches_scope(task: &Task, scope: DeckScope<'_>) -> bool {
    match scope {
        DeckScope::All => true,
        DeckScope::Global => matches!(task.scope, TaskScope::Global),
        DeckScope::Project(selected) => matches!(
            &task.scope,
            TaskScope::Project { path } if Path::new(path) == selected
        ),
    }
}

fn sort_by_updated_desc(tasks: &mut [&Task]) {
    tasks.sort_by_key(|t| Reverse(t.updated_at));
}

fn section_from(kind: SectionKind, project_label: Option<String>, tasks: &[&Task]) -> QueueSection {
    let task_ids: Vec<Uuid> = tasks.iter().map(|t| t.id).collect();
    let count = task_ids.len();
    QueueSection {
        kind,
        project_label,
        task_ids,
        count,
        empty_hint: false,
    }
}

/// ON DECK section for a scoped group: empty groups keep the header and set `empty_hint`.
fn deck_section(project_label: Option<String>, tasks: &[&Task]) -> QueueSection {
    let task_ids: Vec<Uuid> = tasks.iter().map(|t| t.id).collect();
    let count = task_ids.len();
    QueueSection {
        kind: SectionKind::OnDeck,
        project_label,
        task_ids,
        count,
        empty_hint: count == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use crate::domain::{HumanStatus, ProvenanceOrigin, TaskEvent, TaskEventKind, TaskScope};

    fn task(
        id: u128,
        status: HumanStatus,
        scope: TaskScope,
        soft_deleted: bool,
        updated_secs: u64,
    ) -> Task {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(updated_secs);
        Task {
            id: Uuid::from_u128(id),
            revision: Some(Uuid::from_u128(id)),
            merge_base_revision: None,
            title: format!("task-{id}"),
            notes: None,
            status,
            scope,
            capsule: None,
            agent_meta: None,
            last_observed: None,
            provenance: ProvenanceOrigin::Manual,
            history: vec![TaskEvent {
                kind: TaskEventKind::Created,
                at,
            }],
            soft_deleted,
            created_at: at,
            updated_at: at,
        }
    }

    fn project(path: &str) -> TaskScope {
        TaskScope::Project {
            path: path.to_string(),
        }
    }

    fn ids(section: &QueueSection) -> Vec<Uuid> {
        section.task_ids.clone()
    }

    fn section_ids(view: &QueueView, kind: SectionKind) -> Vec<Uuid> {
        view.sections
            .iter()
            .filter(|s| s.kind == kind)
            .flat_map(|s| s.task_ids.iter().copied())
            .collect()
    }

    fn all_listed_ids(view: &QueueView) -> Vec<Uuid> {
        view.sections
            .iter()
            .flat_map(|s| s.task_ids.iter().copied())
            .collect()
    }

    #[test]
    fn in_motion_is_non_deleted_doing_sorted_by_updated_at_desc() {
        let tasks = vec![
            task(1, HumanStatus::Started, TaskScope::Global, false, 10),
            task(2, HumanStatus::Started, project("/repos/a"), false, 30),
            task(3, HumanStatus::Started, TaskScope::Global, true, 40),
            task(4, HumanStatus::Ready, TaskScope::Global, false, 50),
            task(5, HumanStatus::Started, project("/repos/b"), false, 20),
        ];

        let view = query(&tasks, None, DeckScope::All, false);

        let motion: Vec<_> = view
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::InMotion)
            .collect();
        assert_eq!(motion.len(), 1, "exactly one IN MOTION section");
        assert_eq!(motion[0].project_label, None);
        assert_eq!(
            ids(motion[0]),
            vec![Uuid::from_u128(2), Uuid::from_u128(5), Uuid::from_u128(1),]
        );
        assert_eq!(motion[0].count, 3);
        assert_eq!(view.counts.in_motion, 3);
        assert_eq!(view.counts.need, 0);
        assert!(!section_ids(&view, SectionKind::InMotion).contains(&Uuid::from_u128(3)));
        assert!(!section_ids(&view, SectionKind::InMotion).contains(&Uuid::from_u128(4)));
    }

    #[test]
    fn on_deck_groups_current_repo_first_then_alpha_projects_then_global_within_group_updated_desc()
    {
        let current = PathBuf::from("/repos/current");
        let tasks = vec![
            // current repo: newer first
            task(
                10,
                HumanStatus::Ready,
                project("/repos/current"),
                false,
                100,
            ),
            task(
                11,
                HumanStatus::Blocked,
                project("/repos/current"),
                false,
                50,
            ),
            // alpha later project
            task(20, HumanStatus::Ready, project("/repos/zebra"), false, 80),
            task(21, HumanStatus::Review, project("/repos/zebra"), false, 90),
            // alpha earlier project
            task(30, HumanStatus::Ready, project("/repos/alpha"), false, 70),
            // global
            task(40, HumanStatus::Ready, TaskScope::Global, false, 60),
            task(41, HumanStatus::Blocked, TaskScope::Global, false, 65),
            // not on deck
            task(
                50,
                HumanStatus::Started,
                project("/repos/current"),
                false,
                200,
            ),
            task(51, HumanStatus::Done, project("/repos/alpha"), false, 200),
            task(52, HumanStatus::Ready, project("/repos/ghost"), true, 200),
        ];

        let view = query(&tasks, Some(current.as_path()), DeckScope::All, false);

        let deck: Vec<_> = view
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::OnDeck)
            .collect();
        assert_eq!(deck.len(), 4, "current, alpha, zebra, global");

        assert_eq!(deck[0].project_label.as_deref(), Some("/repos/current"));
        assert_eq!(ids(deck[0]), vec![Uuid::from_u128(10), Uuid::from_u128(11)]);
        assert_eq!(deck[0].count, 2);

        assert_eq!(deck[1].project_label.as_deref(), Some("/repos/alpha"));
        assert_eq!(ids(deck[1]), vec![Uuid::from_u128(30)]);

        assert_eq!(deck[2].project_label.as_deref(), Some("/repos/zebra"));
        assert_eq!(ids(deck[2]), vec![Uuid::from_u128(21), Uuid::from_u128(20)]);

        assert_eq!(
            deck[3].project_label, None,
            "global group has no project label"
        );
        assert_eq!(ids(deck[3]), vec![Uuid::from_u128(41), Uuid::from_u128(40)]);

        // doing stays in motion, done nowhere when drawer closed
        assert_eq!(
            section_ids(&view, SectionKind::InMotion),
            vec![Uuid::from_u128(50)]
        );
        assert!(section_ids(&view, SectionKind::Done).is_empty());
        assert!(!all_listed_ids(&view).contains(&Uuid::from_u128(51)));
        assert!(!all_listed_ids(&view).contains(&Uuid::from_u128(52)));
    }

    #[test]
    fn done_drawer_lists_non_deleted_done_updated_desc_and_closed_drawer_excludes_them_from_sections(
    ) {
        let tasks = vec![
            task(1, HumanStatus::Done, TaskScope::Global, false, 10),
            task(2, HumanStatus::Done, project("/repos/a"), false, 30),
            task(3, HumanStatus::Done, TaskScope::Global, true, 40),
            task(4, HumanStatus::Ready, TaskScope::Global, false, 50),
            task(5, HumanStatus::Done, project("/repos/b"), false, 20),
        ];

        let closed = query(&tasks, None, DeckScope::All, false);
        assert!(
            closed.sections.iter().all(|s| s.kind != SectionKind::Done),
            "closed drawer emits no DONE section"
        );
        assert!(
            !all_listed_ids(&closed).contains(&Uuid::from_u128(1))
                && !all_listed_ids(&closed).contains(&Uuid::from_u128(2))
                && !all_listed_ids(&closed).contains(&Uuid::from_u128(5)),
            "done ids absent from every section while drawer closed"
        );
        assert_eq!(closed.counts.done, 3, "status-line still counts done");

        let open = query(&tasks, None, DeckScope::All, true);
        let done: Vec<_> = open
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::Done)
            .collect();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].project_label, None);
        assert_eq!(
            ids(done[0]),
            vec![Uuid::from_u128(2), Uuid::from_u128(5), Uuid::from_u128(1),]
        );
        assert_eq!(done[0].count, 3);
        assert_eq!(open.counts.done, 3);
        assert!(!ids(done[0]).contains(&Uuid::from_u128(3)));
        assert!(!ids(done[0]).contains(&Uuid::from_u128(4)));
    }

    #[test]
    fn blocked_and_review_sit_in_deck_groups_with_no_non_deleted_task_absent_from_sections_and_drawer(
    ) {
        let tasks = vec![
            task(1, HumanStatus::Blocked, project("/repos/a"), false, 10),
            task(2, HumanStatus::Review, TaskScope::Global, false, 20),
            task(3, HumanStatus::Ready, project("/repos/a"), false, 30),
            task(4, HumanStatus::Started, project("/repos/b"), false, 40),
            task(5, HumanStatus::Done, TaskScope::Global, false, 50),
            task(6, HumanStatus::Blocked, TaskScope::Global, true, 60),
        ];

        let view = query(&tasks, Some(Path::new("/repos/a")), DeckScope::All, true);

        let listed = all_listed_ids(&view);
        assert!(listed.contains(&Uuid::from_u128(1)), "blocked on deck");
        assert!(listed.contains(&Uuid::from_u128(2)), "review on deck");
        assert!(section_ids(&view, SectionKind::InMotion).contains(&Uuid::from_u128(4)));
        assert!(section_ids(&view, SectionKind::Done).contains(&Uuid::from_u128(5)));

        let non_deleted: Vec<Uuid> = tasks
            .iter()
            .filter(|t| !t.soft_deleted)
            .map(|t| t.id)
            .collect();
        for id in &non_deleted {
            assert!(
                listed.contains(id),
                "non-deleted task {id} must appear in some section or drawer"
            );
        }
        assert!(!listed.contains(&Uuid::from_u128(6)));

        // blocked/review are deck, not motion
        assert!(!section_ids(&view, SectionKind::InMotion).contains(&Uuid::from_u128(1)));
        assert!(!section_ids(&view, SectionKind::InMotion).contains(&Uuid::from_u128(2)));
        assert!(section_ids(&view, SectionKind::OnDeck).contains(&Uuid::from_u128(1)));
        assert!(section_ids(&view, SectionKind::OnDeck).contains(&Uuid::from_u128(2)));
    }

    #[test]
    fn soft_deleted_tasks_appear_in_no_section_or_drawer() {
        let tasks = vec![
            task(1, HumanStatus::Started, TaskScope::Global, true, 10),
            task(2, HumanStatus::Ready, project("/repos/a"), true, 20),
            task(3, HumanStatus::Blocked, TaskScope::Global, true, 30),
            task(4, HumanStatus::Review, project("/repos/a"), true, 40),
            task(5, HumanStatus::Done, TaskScope::Global, true, 50),
            task(6, HumanStatus::Ready, TaskScope::Global, false, 60),
        ];

        let view = query(&tasks, None, DeckScope::All, true);
        let listed = all_listed_ids(&view);
        for id in 1..=5_u128 {
            assert!(
                !listed.contains(&Uuid::from_u128(id)),
                "soft-deleted {id} must not appear"
            );
        }
        assert_eq!(listed, vec![Uuid::from_u128(6)]);
        assert_eq!(view.counts.in_motion, 0);
        assert_eq!(view.counts.done, 0);
    }

    #[test]
    fn project_scope_filters_in_motion_on_deck_and_done_to_matching_project() {
        let tasks = vec![
            task(1, HumanStatus::Started, project("/repos/a"), false, 100),
            task(2, HumanStatus::Started, project("/repos/b"), false, 90),
            task(3, HumanStatus::Started, TaskScope::Global, false, 80),
            task(10, HumanStatus::Ready, project("/repos/a"), false, 70),
            task(11, HumanStatus::Blocked, project("/repos/b"), false, 60),
            task(12, HumanStatus::Review, TaskScope::Global, false, 50),
            task(20, HumanStatus::Done, project("/repos/a"), false, 40),
            task(21, HumanStatus::Done, project("/repos/b"), false, 30),
            task(22, HumanStatus::Done, TaskScope::Global, false, 20),
        ];

        let scoped_a = query(
            &tasks,
            Some(Path::new("/repos/a")),
            DeckScope::Project(Path::new("/repos/a")),
            true,
        );
        let scoped_global = query(&tasks, Some(Path::new("/repos/a")), DeckScope::Global, true);

        assert_eq!(
            section_ids(&scoped_a, SectionKind::InMotion),
            vec![Uuid::from_u128(1)]
        );
        assert_eq!(
            section_ids(&scoped_a, SectionKind::Done),
            vec![Uuid::from_u128(20)]
        );
        assert_eq!(scoped_a.counts.in_motion, 1);
        assert_eq!(scoped_a.counts.done, 1);

        assert_eq!(
            section_ids(&scoped_global, SectionKind::InMotion),
            vec![Uuid::from_u128(3)]
        );
        assert_eq!(
            section_ids(&scoped_global, SectionKind::Done),
            vec![Uuid::from_u128(22)]
        );
        assert_eq!(scoped_global.counts.in_motion, 1);
        assert_eq!(scoped_global.counts.done, 1);

        let deck_a: Vec<_> = scoped_a
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::OnDeck)
            .collect();
        assert_eq!(deck_a.len(), 1);
        assert_eq!(deck_a[0].project_label.as_deref(), Some("/repos/a"));
        assert_eq!(ids(deck_a[0]), vec![Uuid::from_u128(10)]);
        assert!(!deck_a[0].empty_hint);
        assert!(!section_ids(&scoped_a, SectionKind::OnDeck).contains(&Uuid::from_u128(11)));
        assert!(!section_ids(&scoped_a, SectionKind::OnDeck).contains(&Uuid::from_u128(12)));

        let deck_g: Vec<_> = scoped_global
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::OnDeck)
            .collect();
        assert_eq!(deck_g.len(), 1);
        assert_eq!(deck_g[0].project_label, None);
        assert_eq!(ids(deck_g[0]), vec![Uuid::from_u128(12)]);
        assert!(!deck_g[0].empty_hint);
    }

    #[test]
    fn scoped_project_matches_stored_path_with_trailing_slash() {
        let tasks = vec![task(
            1,
            HumanStatus::Ready,
            project("/repos/a/"),
            false,
            100,
        )];

        let view = query(
            &tasks,
            None,
            DeckScope::Project(Path::new("/repos/a")),
            false,
        );

        let deck = view
            .sections
            .iter()
            .find(|section| section.kind == SectionKind::OnDeck)
            .expect("scoped ON DECK section");
        assert_eq!(deck.project_label.as_deref(), Some("/repos/a/"));
        assert_eq!(ids(deck), vec![Uuid::from_u128(1)]);
        assert!(!deck.empty_hint);
    }

    #[test]
    fn scoped_project_with_zero_open_deck_tasks_yields_header_plus_empty_hint_flag_never_invented_row(
    ) {
        let tasks = vec![
            // project a has only doing + done — zero open deck tasks
            task(1, HumanStatus::Started, project("/repos/a"), false, 100),
            task(2, HumanStatus::Done, project("/repos/a"), false, 90),
            // other scopes still have deck work
            task(10, HumanStatus::Ready, project("/repos/b"), false, 80),
            task(11, HumanStatus::Ready, TaskScope::Global, false, 70),
        ];

        let view = query(
            &tasks,
            Some(Path::new("/repos/a")),
            DeckScope::Project(Path::new("/repos/a")),
            true,
        );

        let deck: Vec<_> = view
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::OnDeck)
            .collect();
        assert_eq!(deck.len(), 1, "scoped empty project still emits its header");
        assert_eq!(deck[0].project_label.as_deref(), Some("/repos/a"));
        assert_eq!(deck[0].count, 0);
        assert!(deck[0].task_ids.is_empty(), "no invented task ids");
        assert!(deck[0].empty_hint);

        // other projects' deck tasks must not leak in
        assert!(!all_listed_ids(&view).contains(&Uuid::from_u128(10)));
        assert!(!all_listed_ids(&view).contains(&Uuid::from_u128(11)));

        // motion + done still present and unfiltered
        assert_eq!(
            section_ids(&view, SectionKind::InMotion),
            vec![Uuid::from_u128(1)]
        );
        assert_eq!(
            section_ids(&view, SectionKind::Done),
            vec![Uuid::from_u128(2)]
        );

        // global scope with zero global deck tasks: same empty-hint contract
        let only_project_deck = vec![task(30, HumanStatus::Ready, project("/repos/a"), false, 50)];
        let global_empty = query(&only_project_deck, None, DeckScope::Global, false);
        let gdeck: Vec<_> = global_empty
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::OnDeck)
            .collect();
        assert_eq!(gdeck.len(), 1);
        assert_eq!(gdeck[0].project_label, None);
        assert_eq!(gdeck[0].count, 0);
        assert!(gdeck[0].task_ids.is_empty());
        assert!(gdeck[0].empty_hint);
    }
}
