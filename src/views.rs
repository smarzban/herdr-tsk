//! Pure board view filters for Focus, Projects, and Global.

use std::path::Path;

use uuid::Uuid;

use crate::domain::{is_linked, HumanStatus, ObservedStatus, Task, TaskScope};

/// Board primary lens identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewId {
    /// Work needing the human across every scope.
    Focus,
    /// Project-scoped work in the selected project, or all projects.
    Projects,
    /// Work without a project scope.
    Global,
}

/// Completion filter available within the Projects and Global lenses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CompletionFilter {
    /// Tasks whose human status is not done.
    #[default]
    Open,
    /// Tasks whose human status is done.
    Completed,
}

/// Pure view query over an in-memory task snapshot.
///
/// `selected_project` is an in-session Projects selection. `None` means all projects.
/// Focus ignores both it and `completion_filter`; it always excludes completed tasks.
/// Soft-deleted tasks are excluded from every lens. No I/O.
pub fn query(
    tasks: &[Task],
    view: ViewId,
    selected_project: Option<&Path>,
    completion_filter: CompletionFilter,
) -> Vec<Uuid> {
    tasks
        .iter()
        .filter(|task| !task.soft_deleted)
        .filter(|task| match view {
            ViewId::Focus => in_focus_set(task),
            ViewId::Projects => {
                is_completion_match(task, completion_filter)
                    && project_path_matches(task, selected_project)
            }
            ViewId::Global => {
                is_completion_match(task, completion_filter)
                    && matches!(task.scope, TaskScope::Global)
            }
        })
        .map(|task| task.id)
        .collect()
}

/// True when a non-soft-deleted, non-completed task belongs in Focus.
///
/// Linked observations are input only. This predicate never changes the human status,
/// link identity, persistence, or host lookup behavior.
pub fn in_focus_set(task: &Task) -> bool {
    !task.soft_deleted
        && task.status != HumanStatus::Done
        && (matches!(task.status, HumanStatus::Blocked | HumanStatus::Review)
            || (is_linked(task)
                && matches!(
                    task.last_observed,
                    Some(ObservedStatus::Blocked | ObservedStatus::Done)
                )))
}

fn is_completion_match(task: &Task, filter: CompletionFilter) -> bool {
    match filter {
        CompletionFilter::Open => task.status != HumanStatus::Done,
        CompletionFilter::Completed => task.status == HumanStatus::Done,
    }
}

fn project_path_matches(task: &Task, selected_project: Option<&Path>) -> bool {
    let TaskScope::Project { path } = &task.scope else {
        return false;
    };
    selected_project.is_none_or(|project| Path::new(path) == project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    use crate::domain::{AgentMeta, ProvenanceOrigin, TaskEvent, TaskEventKind};

    const THIS_REPO: &str = "/repos/this";
    const OTHER_REPO: &str = "/repos/other";

    fn task(id: u128, status: HumanStatus, scope: TaskScope, soft_deleted: bool) -> Task {
        let at = SystemTime::UNIX_EPOCH;
        Task {
            id: Uuid::from_u128(id),
            revision: Some(Uuid::new_v4()),
            merge_base_revision: None,
            title: format!("task-{id}"),
            notes: None,
            thread: None,
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

    #[test]
    fn focus_includes_human_and_exact_linked_observations_but_never_completed_or_deleted() {
        let mut linked_blocked = task(4, HumanStatus::Ready, TaskScope::Global, false);
        linked_blocked.agent_meta = Some(AgentMeta {
            agent_id: None,
            pane_id: Some("w0:p1".into()),
            agent_session: None,
        });
        linked_blocked.last_observed = Some(ObservedStatus::Blocked);

        let mut linked_done = task(5, HumanStatus::Ready, TaskScope::Global, false);
        linked_done.agent_meta = Some(AgentMeta {
            agent_id: None,
            pane_id: Some("w0:p2".into()),
            agent_session: None,
        });
        linked_done.last_observed = Some(ObservedStatus::Done);

        let mut linked_done_but_completed = task(6, HumanStatus::Done, TaskScope::Global, false);
        linked_done_but_completed.agent_meta = Some(AgentMeta {
            agent_id: None,
            pane_id: Some("w0:p3".into()),
            agent_session: None,
        });
        linked_done_but_completed.last_observed = Some(ObservedStatus::Done);

        let tasks = vec![
            task(1, HumanStatus::Blocked, TaskScope::Global, false),
            task(
                2,
                HumanStatus::Review,
                TaskScope::Project {
                    path: THIS_REPO.into(),
                },
                false,
            ),
            task(3, HumanStatus::Blocked, TaskScope::Global, true),
            linked_blocked,
            linked_done,
            linked_done_but_completed,
        ];

        assert_eq!(
            query(&tasks, ViewId::Focus, None, CompletionFilter::Open),
            vec![
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                Uuid::from_u128(4),
                Uuid::from_u128(5),
            ]
        );
    }

    #[test]
    fn projects_filters_open_completed_and_all_projects_without_inventing_a_path() {
        let tasks = vec![
            task(
                1,
                HumanStatus::Ready,
                TaskScope::Project {
                    path: THIS_REPO.into(),
                },
                false,
            ),
            task(
                2,
                HumanStatus::Done,
                TaskScope::Project {
                    path: THIS_REPO.into(),
                },
                false,
            ),
            task(
                3,
                HumanStatus::Done,
                TaskScope::Project {
                    path: OTHER_REPO.into(),
                },
                false,
            ),
            task(4, HumanStatus::Ready, TaskScope::Global, false),
            task(
                5,
                HumanStatus::Ready,
                TaskScope::Project {
                    path: THIS_REPO.into(),
                },
                true,
            ),
        ];

        assert_eq!(
            query(
                &tasks,
                ViewId::Projects,
                Some(Path::new(THIS_REPO)),
                CompletionFilter::Open,
            ),
            vec![Uuid::from_u128(1)]
        );
        assert_eq!(
            query(
                &tasks,
                ViewId::Projects,
                Some(Path::new(THIS_REPO)),
                CompletionFilter::Completed,
            ),
            vec![Uuid::from_u128(2)]
        );
        assert_eq!(
            query(&tasks, ViewId::Projects, None, CompletionFilter::Completed,),
            vec![Uuid::from_u128(2), Uuid::from_u128(3)]
        );
    }

    #[test]
    fn global_filters_open_and_completed_and_excludes_project_and_deleted_tasks() {
        let tasks = vec![
            task(1, HumanStatus::Ready, TaskScope::Global, false),
            task(2, HumanStatus::Done, TaskScope::Global, false),
            task(
                3,
                HumanStatus::Ready,
                TaskScope::Project {
                    path: THIS_REPO.into(),
                },
                false,
            ),
            task(4, HumanStatus::Ready, TaskScope::Global, true),
        ];

        assert_eq!(
            query(&tasks, ViewId::Global, None, CompletionFilter::Open),
            vec![Uuid::from_u128(1)]
        );
        assert_eq!(
            query(&tasks, ViewId::Global, None, CompletionFilter::Completed),
            vec![Uuid::from_u128(2)]
        );
    }
}
