//! Queue Section Query: pure derivation of the board sections from a task snapshot.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::time::SystemTime;

use uuid::Uuid;

use crate::domain::{HumanStatus, Task, TaskScope};

/// Home-board tab lenses. Tabs are visible only at home; project focus hides them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardTab {
    #[default]
    Desk,
    Projects,
    Threads,
}

/// Which list region a section belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    InMotion,
    OnDeck,
    Done,
}

/// Session filter for project focus (today's scoped project board).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckScope<'a> {
    Global,
    Project(&'a Path),
}

/// How the board query is scoped for one paint/selection pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardLens<'a> {
    Home(BoardTab),
    Project(&'a Path),
}

/// Ordered open tasks sharing a normalized thread name in a scoped ON DECK section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadBlock {
    pub name: String,
    pub open_count: usize,
    pub task_ids: Vec<Uuid>,
}

/// Project-scoped tasks under one thread group on the Threads tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadProjectSubgroup {
    /// `None` is the desk/global scope under this thread.
    pub project_path: Option<String>,
    pub task_ids: Vec<Uuid>,
}

/// One ordered section of the queue board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSection {
    pub kind: SectionKind,
    /// Project path for a project-group section. `None` for IN MOTION, DONE, desk ON DECK, and
    /// thread-group sections.
    pub project_label: Option<String>,
    /// Thread name for a thread-group section on the Threads tab.
    pub thread_label: Option<String>,
    /// Thread groups for scoped ON DECK sections only, newest group first.
    pub thread_blocks: Vec<ThreadBlock>,
    /// Project sub-groups for one thread on the Threads tab.
    pub thread_subgroups: Vec<ThreadProjectSubgroup>,
    /// Unthreaded scoped ON DECK tasks, after every thread block.
    pub loose_task_ids: Vec<Uuid>,
    /// Exact flattening for task-only consumers and selection.
    pub task_ids: Vec<Uuid>,
    pub count: usize,
    /// True when a scoped deck group has zero open tasks (renderer paints the empty hint).
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

/// Derive queue sections for the active board lens.
pub fn query_lens(
    tasks: &[Task],
    current_repo: Option<&Path>,
    lens: BoardLens<'_>,
    drawer_open: bool,
) -> QueueView {
    match lens {
        BoardLens::Home(BoardTab::Desk) => query_home_desk(tasks, drawer_open),
        BoardLens::Home(BoardTab::Projects) => {
            query_home_projects(tasks, current_repo, drawer_open)
        }
        BoardLens::Home(BoardTab::Threads) => query_home_threads(tasks, drawer_open),
        BoardLens::Project(path) => query_project_focus(tasks, path, drawer_open),
    }
}

/// Legacy entry retained for tests that still speak in deck-scope terms.
pub fn query(
    tasks: &[Task],
    current_repo: Option<&Path>,
    scope: DeckScope<'_>,
    drawer_open: bool,
) -> QueueView {
    match scope {
        DeckScope::Global => query_lens(
            tasks,
            current_repo,
            BoardLens::Home(BoardTab::Desk),
            drawer_open,
        ),
        DeckScope::Project(path) => {
            query_lens(tasks, current_repo, BoardLens::Project(path), drawer_open)
        }
    }
}

/// Task ids visible for selection, honoring home-tab collapse state.
pub fn visible_task_ids(
    view: &QueueView,
    lens: BoardLens<'_>,
    collapsed_projects: &HashSet<String>,
    collapsed_threads: &HashSet<String>,
    collapsed_thread_projects: &HashSet<ThreadProjectCollapseKey>,
) -> Vec<Uuid> {
    let mut out = Vec::new();
    for section in &view.sections {
        match lens {
            BoardLens::Home(BoardTab::Projects) => {
                let Some(path) = section.project_label.as_deref() else {
                    push_section_tasks(&mut out, section);
                    continue;
                };
                if collapsed_projects.contains(path) {
                    continue;
                }
                push_section_tasks(&mut out, section);
            }
            BoardLens::Home(BoardTab::Threads) => {
                let Some(thread) = section.thread_label.as_deref() else {
                    continue;
                };
                if collapsed_threads.contains(thread) {
                    continue;
                }
                if section.thread_subgroups.is_empty() {
                    push_section_tasks(&mut out, section);
                    continue;
                }
                for subgroup in &section.thread_subgroups {
                    let key = ThreadProjectCollapseKey {
                        thread: thread.to_string(),
                        project_path: subgroup.project_path.clone(),
                    };
                    if collapsed_thread_projects.contains(&key) {
                        continue;
                    }
                    out.extend(subgroup.task_ids.iter().copied());
                }
            }
            _ => push_section_tasks(&mut out, section),
        }
    }
    out
}

/// Session-only collapse identity for a project row under a thread group.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThreadProjectCollapseKey {
    pub thread: String,
    pub project_path: Option<String>,
}

fn push_section_tasks(out: &mut Vec<Uuid>, section: &QueueSection) {
    out.extend(section.task_ids.iter().copied());
}

fn query_home_desk(tasks: &[Task], drawer_open: bool) -> QueueView {
    let live: Vec<&Task> = tasks.iter().filter(|task| !task.soft_deleted).collect();

    let mut motion: Vec<&Task> = live
        .iter()
        .copied()
        .filter(|t| t.status == HumanStatus::Started)
        .collect();
    sort_by_updated_desc(&mut motion);

    let mut desk: Vec<&Task> = live
        .iter()
        .copied()
        .filter(|t| {
            matches!(
                t.status,
                HumanStatus::Ready | HumanStatus::Blocked | HumanStatus::Review
            ) && matches!(t.scope, TaskScope::Global)
        })
        .collect();
    sort_by_updated_desc(&mut desk);

    let mut sections = Vec::new();
    if !motion.is_empty() {
        sections.push(section_from(SectionKind::InMotion, None, None, &motion));
    }
    sections.push(deck_section(None, &desk));

    append_done(&mut sections, &live, drawer_open);

    QueueView {
        sections,
        counts: status_counts(&live, drawer_open),
    }
}

fn query_home_projects(
    tasks: &[Task],
    current_repo: Option<&Path>,
    drawer_open: bool,
) -> QueueView {
    let live: Vec<&Task> = tasks.iter().filter(|task| !task.soft_deleted).collect();
    let project_paths = open_project_paths(&live, current_repo);

    let mut sections = Vec::new();
    for path in project_paths {
        let group: Vec<&Task> = live
            .iter()
            .copied()
            .filter(|t| t.status != HumanStatus::Done)
            .filter(|t| matches!(&t.scope, TaskScope::Project { path: p } if p == &path))
            .collect();
        let mut ordered = group;
        sort_by_status_then_updated(&mut ordered);
        sections.push(project_group_section(path, &ordered));
    }

    append_done(&mut sections, &live, drawer_open);

    QueueView {
        sections,
        counts: status_counts(&live, drawer_open),
    }
}

fn query_home_threads(tasks: &[Task], drawer_open: bool) -> QueueView {
    let live: Vec<&Task> = tasks.iter().filter(|task| !task.soft_deleted).collect();

    let mut by_thread: BTreeMap<String, Vec<&Task>> = BTreeMap::new();
    for task in live
        .iter()
        .copied()
        .filter(|t| t.status != HumanStatus::Done)
    {
        if let Some(name) = task.thread.as_deref() {
            by_thread
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(task);
        }
    }

    let mut thread_names: Vec<String> = by_thread.keys().cloned().collect();
    thread_names.sort_by_key(|name| {
        Reverse(
            by_thread[name]
                .iter()
                .map(|task| task.updated_at)
                .max()
                .unwrap_or(SystemTime::UNIX_EPOCH),
        )
    });

    let mut sections = Vec::new();
    for name in thread_names {
        let group = &by_thread[&name];
        sections.push(thread_group_section(name, group));
    }

    append_done(&mut sections, &live, drawer_open);

    QueueView {
        sections,
        counts: status_counts(&live, drawer_open),
    }
}

fn query_project_focus(tasks: &[Task], path: &Path, drawer_open: bool) -> QueueView {
    let scope = DeckScope::Project(path);
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

    let mut open: Vec<&Task> = live
        .iter()
        .copied()
        .filter(|t| !matches!(t.status, HumanStatus::Started | HumanStatus::Done))
        .collect();
    sort_by_updated_desc(&mut open);

    let (label, _) = live
        .iter()
        .find_map(|task| match &task.scope {
            TaskScope::Project { path: stored } if Path::new(stored) == path => {
                Some(stored.clone())
            }
            _ => None,
        })
        .map(|stored| (stored, ()))
        .unwrap_or_else(|| (path.to_string_lossy().into_owned(), ()));

    let mut sections = Vec::new();
    if !motion.is_empty() {
        sections.push(section_from(SectionKind::InMotion, None, None, &motion));
    }
    sections.push(deck_section(Some(label), &open));
    append_done(&mut sections, &live, drawer_open);

    QueueView {
        sections,
        counts: status_counts(&live, drawer_open),
    }
}

fn open_project_paths(live: &[&Task], current_repo: Option<&Path>) -> Vec<String> {
    let mut paths: BTreeSet<String> = BTreeSet::new();
    for task in live
        .iter()
        .copied()
        .filter(|t| t.status != HumanStatus::Done)
    {
        if let TaskScope::Project { path } = &task.scope {
            paths.insert(path.clone());
        }
    }
    let mut ordered: Vec<String> = paths.into_iter().collect();
    if let Some(repo) = current_repo {
        if let Some(pos) = ordered.iter().position(|path| Path::new(path) == repo) {
            let current = ordered.remove(pos);
            ordered.insert(0, current);
        }
    }
    ordered
}

fn project_group_section(path: String, tasks: &[&Task]) -> QueueSection {
    let task_ids: Vec<Uuid> = tasks.iter().map(|t| t.id).collect();
    let count = task_ids.len();
    QueueSection {
        kind: SectionKind::OnDeck,
        project_label: Some(path),
        thread_label: None,
        thread_blocks: Vec::new(),
        thread_subgroups: Vec::new(),
        loose_task_ids: Vec::new(),
        task_ids,
        count,
        empty_hint: count == 0,
    }
}

fn thread_group_section(name: String, tasks: &[&Task]) -> QueueSection {
    let mut by_scope: BTreeMap<Option<String>, Vec<&Task>> = BTreeMap::new();
    for task in tasks {
        let key = match &task.scope {
            TaskScope::Global => None,
            TaskScope::Project { path } => Some(path.clone()),
        };
        by_scope.entry(key).or_default().push(*task);
    }

    let mut scope_keys: Vec<Option<String>> = by_scope.keys().cloned().collect();
    scope_keys.sort_by_key(|path| match path {
        None => (1u8, String::new()),
        Some(path) => (0u8, path.clone()),
    });

    let mut thread_subgroups = Vec::new();
    let mut task_ids = Vec::new();
    for path in scope_keys {
        let mut group = by_scope.remove(&path).unwrap_or_default();
        sort_by_status_then_updated(&mut group);
        let ids: Vec<Uuid> = group.iter().map(|t| t.id).collect();
        task_ids.extend(ids.iter().copied());
        thread_subgroups.push(ThreadProjectSubgroup {
            project_path: path,
            task_ids: ids,
        });
    }

    let count = task_ids.len();
    QueueSection {
        kind: SectionKind::OnDeck,
        project_label: None,
        thread_label: Some(name),
        thread_blocks: Vec::new(),
        thread_subgroups,
        loose_task_ids: Vec::new(),
        task_ids,
        count,
        empty_hint: count == 0,
    }
}

fn append_done(sections: &mut Vec<QueueSection>, live: &[&Task], drawer_open: bool) {
    if !drawer_open {
        return;
    }
    let mut done: Vec<&Task> = live
        .iter()
        .copied()
        .filter(|t| t.status == HumanStatus::Done)
        .collect();
    sort_by_updated_desc(&mut done);
    if !done.is_empty() {
        sections.push(section_from(SectionKind::Done, None, None, &done));
    }
}

fn status_counts(live: &[&Task], drawer_open: bool) -> StatusCounts {
    let in_motion = live
        .iter()
        .filter(|t| t.status == HumanStatus::Started)
        .count();
    let done = if drawer_open {
        live.iter()
            .filter(|t| t.status == HumanStatus::Done)
            .count()
    } else {
        live.iter()
            .filter(|t| t.status == HumanStatus::Done)
            .count()
    };
    StatusCounts {
        in_motion,
        done,
        need: 0,
    }
}

fn task_matches_scope(task: &Task, scope: DeckScope<'_>) -> bool {
    match scope {
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

fn status_rank(status: HumanStatus) -> u8 {
    match status {
        HumanStatus::Started => 0,
        HumanStatus::Review => 1,
        HumanStatus::Blocked => 2,
        HumanStatus::Ready => 3,
        HumanStatus::Done => 4,
    }
}

fn sort_by_status_then_updated(tasks: &mut [&Task]) {
    tasks.sort_by(|a, b| {
        status_rank(a.status)
            .cmp(&status_rank(b.status))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

fn section_from(
    kind: SectionKind,
    project_label: Option<String>,
    thread_label: Option<String>,
    tasks: &[&Task],
) -> QueueSection {
    let task_ids: Vec<Uuid> = tasks.iter().map(|t| t.id).collect();
    let count = task_ids.len();
    QueueSection {
        kind,
        project_label,
        thread_label,
        thread_blocks: Vec::new(),
        thread_subgroups: Vec::new(),
        loose_task_ids: Vec::new(),
        task_ids,
        count,
        empty_hint: false,
    }
}

/// ON DECK section for a scoped group: empty groups keep the header and set `empty_hint`.
fn deck_section(project_label: Option<String>, tasks: &[&Task]) -> QueueSection {
    let mut grouped: BTreeMap<String, Vec<&Task>> = BTreeMap::new();
    let mut loose = Vec::new();
    for task in tasks {
        if let Some(name) = task.thread.as_deref() {
            grouped
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push(*task);
        } else {
            loose.push(*task);
        }
    }

    let mut thread_blocks: Vec<ThreadBlock> = grouped
        .into_iter()
        .map(|(name, tasks)| ThreadBlock {
            name,
            open_count: tasks.len(),
            task_ids: tasks.iter().map(|task| task.id).collect(),
        })
        .collect();
    thread_blocks.sort_by_key(|block| {
        Reverse(
            tasks
                .iter()
                .find(|task| task.id == block.task_ids[0])
                .expect("thread block task comes from deck tasks")
                .updated_at,
        )
    });

    let loose_task_ids: Vec<Uuid> = loose.iter().map(|task| task.id).collect();
    let task_ids = thread_blocks
        .iter()
        .flat_map(|block| block.task_ids.iter().copied())
        .chain(loose_task_ids.iter().copied())
        .collect();
    let count = tasks.len();
    QueueSection {
        kind: SectionKind::OnDeck,
        project_label,
        thread_label: None,
        thread_blocks,
        thread_subgroups: Vec::new(),
        loose_task_ids,
        task_ids,
        count,
        empty_hint: count == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    use crate::domain::{HumanStatus, ProvenanceOrigin, TaskEvent, TaskEventKind, TaskScope};

    fn task(
        id: u128,
        status: HumanStatus,
        scope: TaskScope,
        soft_deleted: bool,
        updated_secs: u64,
    ) -> Task {
        task_with_thread(id, status, scope, soft_deleted, updated_secs, None)
    }

    fn task_with_thread(
        id: u128,
        status: HumanStatus,
        scope: TaskScope,
        soft_deleted: bool,
        updated_secs: u64,
        thread: Option<&str>,
    ) -> Task {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(updated_secs);
        Task {
            id: Uuid::from_u128(id),
            revision: Some(Uuid::from_u128(id)),
            merge_base_revision: None,
            title: format!("task-{id}"),
            notes: None,
            thread: thread.map(str::to_string),
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
            steps: Vec::new(),
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
    fn home_desk_in_motion_is_global_started_sorted_by_updated_desc() {
        let tasks = vec![
            task(1, HumanStatus::Started, TaskScope::Global, false, 10),
            task(2, HumanStatus::Started, project("/repos/a"), false, 30),
            task(3, HumanStatus::Started, TaskScope::Global, true, 40),
            task(4, HumanStatus::Ready, TaskScope::Global, false, 50),
            task(5, HumanStatus::Started, project("/repos/b"), false, 20),
        ];

        let view = query_lens(&tasks, None, BoardLens::Home(BoardTab::Desk), false);

        let motion: Vec<_> = view
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::InMotion)
            .collect();
        assert_eq!(motion.len(), 1);
        assert_eq!(
            ids(motion[0]),
            vec![Uuid::from_u128(2), Uuid::from_u128(5), Uuid::from_u128(1),]
        );

        let desk: Vec<_> = view
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::OnDeck && s.project_label.is_none())
            .collect();
        assert_eq!(desk.len(), 1);
        assert_eq!(ids(desk[0]), vec![Uuid::from_u128(4)]);
        assert!(section_ids(&view, SectionKind::InMotion).contains(&Uuid::from_u128(2)));
        assert!(!section_ids(&view, SectionKind::OnDeck).contains(&Uuid::from_u128(2)));
    }

    #[test]
    fn home_projects_keeps_started_under_project_ordered_by_status() {
        let tasks = vec![
            task(1, HumanStatus::Ready, project("/repos/a"), false, 10),
            task(2, HumanStatus::Started, project("/repos/a"), false, 20),
            task(3, HumanStatus::Blocked, project("/repos/a"), false, 30),
            task(4, HumanStatus::Review, project("/repos/a"), false, 40),
            task(5, HumanStatus::Started, project("/repos/b"), false, 50),
        ];

        let view = query_lens(
            &tasks,
            Some(Path::new("/repos/a")),
            BoardLens::Home(BoardTab::Projects),
            false,
        );

        assert!(view
            .sections
            .iter()
            .all(|section| section.kind != SectionKind::InMotion));

        let a = view
            .sections
            .iter()
            .find(|s| s.project_label.as_deref() == Some("/repos/a"))
            .expect("project a");
        assert_eq!(
            ids(a),
            vec![
                Uuid::from_u128(2),
                Uuid::from_u128(4),
                Uuid::from_u128(3),
                Uuid::from_u128(1),
            ]
        );
    }

    #[test]
    fn home_threads_groups_cross_project_by_name_with_project_subgroups() {
        let tasks = vec![
            task_with_thread(
                1,
                HumanStatus::Started,
                project("/repos/a"),
                false,
                30,
                Some("release"),
            ),
            task_with_thread(
                2,
                HumanStatus::Ready,
                TaskScope::Global,
                false,
                20,
                Some("release"),
            ),
            task_with_thread(
                3,
                HumanStatus::Review,
                project("/repos/b"),
                false,
                10,
                Some("release"),
            ),
            task_with_thread(
                4,
                HumanStatus::Ready,
                project("/repos/a"),
                false,
                5,
                Some("other"),
            ),
        ];

        let view = query_lens(&tasks, None, BoardLens::Home(BoardTab::Threads), false);
        let release = view
            .sections
            .iter()
            .find(|s| s.thread_label.as_deref() == Some("release"))
            .expect("release thread");
        assert_eq!(release.thread_subgroups.len(), 3);
        assert_eq!(
            ids(release),
            vec![Uuid::from_u128(1), Uuid::from_u128(3), Uuid::from_u128(2),]
        );
    }

    #[test]
    fn done_drawer_lists_non_deleted_done_updated_desc() {
        let tasks = vec![
            task(1, HumanStatus::Done, TaskScope::Global, false, 10),
            task(2, HumanStatus::Done, project("/repos/a"), false, 30),
            task(3, HumanStatus::Done, TaskScope::Global, true, 40),
            task(4, HumanStatus::Ready, TaskScope::Global, false, 50),
        ];

        let closed = query_lens(&tasks, None, BoardLens::Home(BoardTab::Desk), false);
        assert!(closed.sections.iter().all(|s| s.kind != SectionKind::Done));

        let open = query_lens(&tasks, None, BoardLens::Home(BoardTab::Desk), true);
        let done: Vec<_> = open
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::Done)
            .collect();
        assert_eq!(done.len(), 1);
        assert_eq!(ids(done[0]), vec![Uuid::from_u128(2), Uuid::from_u128(1),]);
    }

    #[test]
    fn project_focus_filters_sections_to_matching_project() {
        let tasks = vec![
            task(1, HumanStatus::Started, project("/repos/a"), false, 100),
            task(2, HumanStatus::Started, project("/repos/b"), false, 90),
            task(10, HumanStatus::Ready, project("/repos/a"), false, 70),
            task(20, HumanStatus::Done, project("/repos/a"), false, 40),
        ];

        let view = query_lens(
            &tasks,
            None,
            BoardLens::Project(Path::new("/repos/a")),
            true,
        );

        assert_eq!(
            section_ids(&view, SectionKind::InMotion),
            vec![Uuid::from_u128(1)]
        );
        assert_eq!(
            section_ids(&view, SectionKind::Done),
            vec![Uuid::from_u128(20)]
        );
        assert!(!all_listed_ids(&view).contains(&Uuid::from_u128(2)));
    }

    #[test]
    fn visible_task_ids_honors_project_and_thread_collapse() {
        let tasks = vec![
            task(1, HumanStatus::Ready, project("/repos/a"), false, 10),
            task(2, HumanStatus::Ready, project("/repos/b"), false, 20),
            task_with_thread(
                3,
                HumanStatus::Ready,
                project("/repos/a"),
                false,
                30,
                Some("release"),
            ),
        ];
        let view = query_lens(&tasks, None, BoardLens::Home(BoardTab::Projects), false);
        let mut collapsed_projects = HashSet::new();
        collapsed_projects.insert("/repos/a".to_string());
        let visible = visible_task_ids(
            &view,
            BoardLens::Home(BoardTab::Projects),
            &collapsed_projects,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(visible, vec![Uuid::from_u128(2)]);

        let threads = query_lens(&tasks, None, BoardLens::Home(BoardTab::Threads), false);
        let mut collapsed_threads = HashSet::new();
        collapsed_threads.insert("release".to_string());
        let visible_threads = visible_task_ids(
            &threads,
            BoardLens::Home(BoardTab::Threads),
            &HashSet::new(),
            &collapsed_threads,
            &HashSet::new(),
        );
        assert!(visible_threads.is_empty());
    }
}
