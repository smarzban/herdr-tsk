//! Task model and lifecycle commands.
//!
//! Capsule and agent identity fields stay on [`Task`] for store serde compatibility.
//! Human status remains the source of truth; observation fields are retained data only.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    DispatchAttempt, DispatchAttemptError, DispatchAttemptMode, DispatchAttemptTransition,
    ProvenanceOrigin, TaskEvent, TaskEventKind, UndoEntry,
};

/// Human-facing task progress. Source of truth for board state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanStatus {
    /// Previously serialized as `todo`. Old stores still load via the alias.
    #[serde(alias = "todo")]
    Ready,
    /// Previously serialized as `doing`. Old stores still load via the alias.
    #[serde(alias = "doing")]
    Started,
    Blocked,
    Review,
    Done,
}

/// Where a task belongs: global or a project identified by stable path scope key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskScope {
    Global,
    Project { path: String },
}

/// Frozen capture context. Missing fields stay `None`; never invent values.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContextCapsule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_text: Option<String>,
    /// Optional file path for a file/line reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Optional 1-based line for a file/line reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl ContextCapsule {
    /// True when every capsule field is absent.
    pub fn is_empty(&self) -> bool {
        self.repo_path.is_none()
            && self.worktree_path.is_none()
            && self.branch.is_none()
            && self.cwd.is_none()
            && self.source_pane_id.is_none()
            && self.selected_text.is_none()
            && self.file.is_none()
            && self.line.is_none()
    }
}

/// Exact stable identity reported by the host for one agent session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionIdentity {
    pub source: String,
    pub value: String,
}

/// Agent/session/pane identity only. No lifecycle field; status lives on [`ObservedStatus`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<AgentSessionIdentity>,
}

impl AgentMeta {
    /// True when no agent, pane, or stable session identity is present.
    pub fn is_empty(&self) -> bool {
        self.agent_id.is_none() && self.pane_id.is_none() && self.agent_session.is_none()
    }
}

/// Host-observed agent lifecycle retained for store serde compatibility.
///
/// V1 board code does not apply observations to human status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedStatus {
    Working,
    Blocked,
    Idle,
    Done,
    Unknown,
}

/// One step in a task's flat, ordered steps collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// Stable identity, addressable by id prefix like a task.
    pub id: Uuid,
    /// One line of text, trimmed at the boundaries.
    pub text: String,
    pub done: bool,
}

/// One unit of intended work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    /// Store-global human task number. Absent until the locked persistence boundary assigns it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    /// Opaque semantic revision used to guard concurrent operations. Legacy tasks
    /// decode without one and receive a revision on their next mutation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<Uuid>,
    /// Revision observed before an in-memory mutation. It is transient save intent carried to
    /// the locked store merge, never retained after a successful durable write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_base_revision: Option<Uuid>,
    pub title: String,
    pub notes: Option<String>,
    /// Optional normalized thread name. Missing fields in older stores decode as unthreaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    pub status: HumanStatus,
    pub scope: TaskScope,
    pub capsule: Option<ContextCapsule>,
    pub agent_meta: Option<AgentMeta>,
    /// Last host observation retained for store serde. Absent when never observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed: Option<ObservedStatus>,
    pub provenance: ProvenanceOrigin,
    /// Append-only domain event history.
    pub history: Vec<TaskEvent>,
    /// Flat, ordered steps. Absent on pre-steps stores; never reordered by a verb.
    /// Old stores name this field `checklist`; new writes use `steps` only.
    #[serde(default, alias = "checklist", skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
    pub soft_deleted: bool,
    #[serde(with = "super::time_serde")]
    pub created_at: SystemTime,
    #[serde(with = "super::time_serde")]
    pub updated_at: SystemTime,
}

/// Domain command failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Title was empty or whitespace-only after trim.
    EmptyTitle,
    /// No task with this id exists in the domain state.
    UnknownId(Uuid),
    /// Command refused because the task is soft-deleted.
    SoftDeleted(Uuid),
    /// Undo target changed after the undoable action was recorded.
    StaleUndo(Uuid),
    /// No active dispatch attempt with this id exists in the domain state.
    UnknownDispatchAttempt(Uuid),
    /// Step text was empty or whitespace-only after trim.
    EmptyStepText,
    /// No step with this id exists on the task.
    UnknownStep(Uuid),
    /// A task already owns an active dispatch attempt and cannot start another.
    ActiveDispatchAttempt { task_id: Uuid, attempt_id: Uuid },
    /// Dispatch-attempt transition was refused without changing the journal.
    DispatchAttempt(DispatchAttemptError),
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainError::EmptyTitle => write!(f, "title must be non-empty after trim"),
            DomainError::UnknownId(id) => write!(f, "unknown task id {id}"),
            DomainError::SoftDeleted(id) => write!(f, "task {id} is soft-deleted"),
            DomainError::StaleUndo(id) => {
                write!(f, "task {id} changed since the undoable action")
            }
            DomainError::UnknownDispatchAttempt(id) => write!(f, "unknown dispatch attempt {id}"),
            DomainError::EmptyStepText => write!(f, "step text must be non-empty after trim"),
            DomainError::UnknownStep(id) => write!(f, "unknown step id {id}"),
            DomainError::ActiveDispatchAttempt {
                task_id,
                attempt_id,
            } => write!(
                f,
                "task {task_id} already has active dispatch attempt {attempt_id}"
            ),
            DomainError::DispatchAttempt(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DomainError {}

impl From<DispatchAttemptError> for DomainError {
    fn from(value: DispatchAttemptError) -> Self {
        Self::DispatchAttempt(value)
    }
}

/// Sentinel carried only in memory to represent a legacy task's absent revision as a save base.
/// UUID v4 revisions can never equal nil.
const LEGACY_MERGE_BASE_REVISION: Uuid = Uuid::nil();

fn record_mutation(task: &mut Task, kind: TaskEventKind) {
    let now = SystemTime::now();
    task.merge_base_revision = Some(task.revision.unwrap_or(LEGACY_MERGE_BASE_REVISION));
    task.revision = Some(Uuid::new_v4());
    task.updated_at = now;
    task.history.push(TaskEvent { kind, at: now });
}

/// Document version written by this binary.
/// Bump when an older writer cannot round-trip a newly persisted field.
pub const STORE_FORMAT_VERSION: u32 = 2;

/// Version of documents written before `format_version` existed.
/// Stay on 1 when [`STORE_FORMAT_VERSION`] is bumped.
pub const LEGACY_STORE_FORMAT_VERSION: u32 = 1;

fn default_store_format_version() -> u32 {
    LEGACY_STORE_FORMAT_VERSION
}

fn default_next_task_number() -> u64 {
    1
}

/// In-memory task set. Persistence is Task Store.
///
/// SHORTCUT: Vec scan by id; fine until store loads many tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainState {
    /// Store document version. Missing on older files loads as [`LEGACY_STORE_FORMAT_VERSION`].
    #[serde(default = "default_store_format_version")]
    format_version: u32,
    /// The next store-global task number, allocated only while holding the store lock.
    #[serde(default = "default_next_task_number")]
    pub next_task_number: u64,
    tasks: Vec<Task>,
    /// Active recovery records sharing the task store's lock and atomic replacement boundary.
    #[serde(default)]
    active_attempts: Vec<DispatchAttempt>,
    /// LIFO undo records for soft-delete and complete.
    undo_stack: Vec<UndoEntry>,
}

impl Default for DomainState {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainState {
    pub fn new() -> Self {
        Self {
            format_version: STORE_FORMAT_VERSION,
            next_task_number: 1,
            tasks: Vec::new(),
            active_attempts: Vec::new(),
            undo_stack: Vec::new(),
        }
    }

    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Stamp this binary's format version before a durable write.
    pub(crate) fn stamp_format_version(&mut self) {
        self.format_version = STORE_FORMAT_VERSION;
    }

    /// Inspect the top undo entry without consuming it.
    pub(crate) fn last_undo(&self) -> Option<&UndoEntry> {
        self.undo_stack.last()
    }

    /// Pop the top undo entry after its revision guard has passed.
    pub(crate) fn pop_undo(&mut self) -> Option<UndoEntry> {
        self.undo_stack.pop()
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Active durable dispatch attempts. Entries remain until a later explicit success or
    /// fully-completed cleanup removes them.
    pub fn active_attempts(&self) -> &[DispatchAttempt] {
        &self.active_attempts
    }

    pub fn active_attempt(&self, id: Uuid) -> Option<&DispatchAttempt> {
        self.active_attempts
            .iter()
            .find(|attempt| attempt.id() == id)
    }

    /// Active attempt for one task, if any. A task can own only one until completion
    /// or full cleanup removes it.
    pub fn active_attempt_for_task(&self, task_id: Uuid) -> Option<&DispatchAttempt> {
        self.active_attempts
            .iter()
            .find(|attempt| attempt.task_id() == task_id)
    }

    /// Record an empty durable attempt before starting host dispatch work. This never
    /// mutates the task's human status.
    pub fn start_dispatch_attempt(
        &mut self,
        task_id: Uuid,
        mode: DispatchAttemptMode,
        kind: impl Into<String>,
    ) -> Result<Uuid, DomainError> {
        let task = self.get(task_id).ok_or(DomainError::UnknownId(task_id))?;
        if task.soft_deleted {
            return Err(DomainError::SoftDeleted(task_id));
        }
        if let Some(existing) = self.active_attempt_for_task(task_id) {
            return Err(DomainError::ActiveDispatchAttempt {
                task_id,
                attempt_id: existing.id(),
            });
        }
        let attempt = DispatchAttempt::start(task_id, mode, kind);
        let id = attempt.id();
        self.active_attempts.push(attempt);
        Ok(id)
    }

    /// Replace one attempt through its revision-guarded journal transition.
    pub fn transition_dispatch_attempt(
        &mut self,
        attempt_id: Uuid,
        expected_revision: Uuid,
        transition: DispatchAttemptTransition,
    ) -> Result<(), DomainError> {
        let attempt = self
            .active_attempts
            .iter_mut()
            .find(|attempt| attempt.id() == attempt_id)
            .ok_or(DomainError::UnknownDispatchAttempt(attempt_id))?;
        *attempt = attempt.transition(expected_revision, transition)?;
        Ok(())
    }

    /// Remove an active attempt only when the caller still holds its current revision.
    /// Completion and fully-cleaned removal are explicit higher-level operations.
    pub fn remove_dispatch_attempt(
        &mut self,
        attempt_id: Uuid,
        expected_revision: Uuid,
    ) -> Result<(), DomainError> {
        let index = self
            .active_attempts
            .iter()
            .position(|attempt| attempt.id() == attempt_id)
            .ok_or(DomainError::UnknownDispatchAttempt(attempt_id))?;
        let current = self.active_attempts[index].revision();
        if current != expected_revision {
            return Err(DispatchAttemptError::StaleRevision {
                expected: expected_revision,
                current,
            }
            .into());
        }
        self.active_attempts.remove(index);
        Ok(())
    }

    /// Lookup by id. Soft-deleted tasks remain findable.
    pub fn get(&self, id: Uuid) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Create a task with human status `ready` and the given provenance origin.
    ///
    /// Rejects empty/whitespace-only titles. On success, stores exactly one
    /// task with the trimmed title and optional notes, and appends a
    /// `Created` event.
    ///
    /// `agent_meta` is stored as-is. It does not affect `status`.
    pub fn create(
        &mut self,
        title: impl AsRef<str>,
        notes: Option<String>,
        scope: TaskScope,
        capsule: Option<ContextCapsule>,
        agent_meta: Option<AgentMeta>,
        provenance: ProvenanceOrigin,
    ) -> Result<Uuid, DomainError> {
        self.create_with_thread(title, notes, scope, capsule, agent_meta, provenance, None)
    }

    /// Create a task with an already-normalized optional thread in its initial mutation.
    ///
    /// [`Self::create`] remains the unthreaded compatibility path for existing callers.
    #[allow(clippy::too_many_arguments)] // Mirrors the stable `create` field list plus thread.
    pub fn create_with_thread(
        &mut self,
        title: impl AsRef<str>,
        notes: Option<String>,
        scope: TaskScope,
        capsule: Option<ContextCapsule>,
        agent_meta: Option<AgentMeta>,
        provenance: ProvenanceOrigin,
        thread: Option<String>,
    ) -> Result<Uuid, DomainError> {
        let title = title.as_ref().trim();
        if title.is_empty() {
            return Err(DomainError::EmptyTitle);
        }

        let now = SystemTime::now();
        let id = Uuid::new_v4();
        // Soft link: store non-empty meta only (missing omitted, not invented).
        let agent_meta = agent_meta.filter(|m| !m.is_empty());
        self.tasks.push(Task {
            id,
            number: None,
            revision: Some(Uuid::new_v4()),
            merge_base_revision: None,
            title: title.to_string(),
            notes,
            thread,
            status: HumanStatus::Ready,
            scope,
            capsule,
            agent_meta,
            last_observed: None,
            provenance,
            history: vec![TaskEvent {
                kind: TaskEventKind::Created,
                at: now,
            }],
            steps: Vec::new(),
            soft_deleted: false,
            created_at: now,
            updated_at: now,
        });
        Ok(id)
    }

    /// Set human status. Human status is truth; callers are user commands only.
    pub fn set_status(&mut self, id: Uuid, status: HumanStatus) -> Result<(), DomainError> {
        self.apply_status(id, status, TaskEventKind::StatusSet)
    }

    /// Complete: set human status to `done`. Pushes an undo entry.
    pub fn complete(&mut self, id: Uuid) -> Result<(), DomainError> {
        self.apply_status(id, HumanStatus::Done, TaskEventKind::Completed)?;
        let expected_revision = self.get(id).and_then(|task| task.revision);
        self.undo_stack.push(UndoEntry::Complete {
            id,
            expected_revision,
        });
        Ok(())
    }

    /// Reopen a `done` task to `ready`.
    pub fn reopen(&mut self, id: Uuid) -> Result<(), DomainError> {
        self.apply_status(id, HumanStatus::Ready, TaskEventKind::Reopened)
    }

    /// Soft-delete: mark excluded from board views until restore. Stays in store.
    /// Pushes an undo entry so `undo` can restore.
    pub fn soft_delete(&mut self, id: Uuid) -> Result<(), DomainError> {
        let expected_revision = {
            let task = self.task_mut(id)?;
            task.soft_deleted = true;
            record_mutation(task, TaskEventKind::SoftDeleted);
            task.revision
        };
        self.undo_stack.push(UndoEntry::SoftDelete {
            id,
            expected_revision,
        });
        Ok(())
    }

    /// Clear soft-delete so the task can reappear in views.
    pub fn restore(&mut self, id: Uuid) -> Result<(), DomainError> {
        let task = self.task_mut(id)?;
        task.soft_deleted = false;
        record_mutation(task, TaskEventKind::Restored);
        Ok(())
    }

    /// Edit title, notes, scope, and thread together. Title uses the same non-empty trim rule as create.
    pub fn edit(
        &mut self,
        id: Uuid,
        title: impl AsRef<str>,
        notes: Option<String>,
        scope: TaskScope,
        thread: Option<String>,
    ) -> Result<(), DomainError> {
        let title = title.as_ref().trim();
        if title.is_empty() {
            return Err(DomainError::EmptyTitle);
        }
        let task = self.task_mut(id)?;
        task.title = title.to_string();
        task.notes = notes;
        task.scope = scope;
        task.thread = thread;
        record_mutation(task, TaskEventKind::Edited);
        Ok(())
    }

    /// Add one step at the end of the task's steps.
    ///
    /// Trims text and refuses empty-after-trim. Returns the new step's id.
    /// Never touches status, scope, or notes.
    pub fn add_step(&mut self, task_id: Uuid, text: impl AsRef<str>) -> Result<Uuid, DomainError> {
        let text = text.as_ref().trim();
        if text.is_empty() {
            return Err(DomainError::EmptyStepText);
        }
        let task = self.task_mut(task_id)?;
        let step = Step {
            id: Uuid::new_v4(),
            text: text.to_string(),
            done: false,
        };
        let step_id = step.id;
        task.steps.push(step);
        record_mutation(task, TaskEventKind::StepAdded);
        Ok(step_id)
    }

    /// Flip one step's done flag.
    ///
    /// Journals `StepChecked` when the step turns done and `StepUnchecked`
    /// when it turns open. Never touches status, scope, or notes; completing
    /// the steps never completes the task.
    pub fn toggle_step(&mut self, task_id: Uuid, step_id: Uuid) -> Result<(), DomainError> {
        let task = self.task_mut(task_id)?;
        let step = task
            .steps
            .iter_mut()
            .find(|step| step.id == step_id)
            .ok_or(DomainError::UnknownStep(step_id))?;
        step.done = !step.done;
        let kind = if step.done {
            TaskEventKind::StepChecked
        } else {
            TaskEventKind::StepUnchecked
        };
        record_mutation(task, kind);
        Ok(())
    }

    /// Rename one step. Trims text and refuses empty-after-trim.
    /// Never touches status, scope, or notes.
    pub fn rename_step(
        &mut self,
        task_id: Uuid,
        step_id: Uuid,
        text: impl AsRef<str>,
    ) -> Result<(), DomainError> {
        let text = text.as_ref().trim();
        if text.is_empty() {
            return Err(DomainError::EmptyStepText);
        }
        let task = self.task_mut(task_id)?;
        let step = task
            .steps
            .iter_mut()
            .find(|step| step.id == step_id)
            .ok_or(DomainError::UnknownStep(step_id))?;
        step.text = text.to_string();
        record_mutation(task, TaskEventKind::StepRenamed);
        Ok(())
    }

    /// Remove one step by id. Never touches status, scope, or notes.
    pub fn remove_step(&mut self, task_id: Uuid, step_id: Uuid) -> Result<(), DomainError> {
        let task = self.task_mut(task_id)?;
        let index = task
            .steps
            .iter()
            .position(|step| step.id == step_id)
            .ok_or(DomainError::UnknownStep(step_id))?;
        task.steps.remove(index);
        record_mutation(task, TaskEventKind::StepRemoved);
        Ok(())
    }

    fn apply_status(
        &mut self,
        id: Uuid,
        status: HumanStatus,
        kind: TaskEventKind,
    ) -> Result<(), DomainError> {
        let task = self.task_mut(id)?;
        task.status = status;
        record_mutation(task, kind);
        Ok(())
    }

    fn task_mut(&mut self, id: Uuid) -> Result<&mut Task, DomainError> {
        self.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(DomainError::UnknownId(id))
    }

    /// Merge a fresh disk snapshot into local presentation state. Disk wins divergent tasks
    /// because this method has no mutation baseline and must not guess with wall-clock
    /// timestamps.
    pub fn merge_tasks_from_disk(&mut self, other: &DomainState) {
        for incoming in &other.tasks {
            match self.tasks.iter_mut().find(|task| task.id == incoming.id) {
                Some(existing) if existing.revision != incoming.revision => {
                    *existing = incoming.clone()
                }
                Some(_) => {}
                None => self.tasks.push(incoming.clone()),
            }
        }
        self.merge_undo_entries(other);
        self.merge_attempts_from_disk(other);
    }

    /// Merge one local save intent under the store lock without using wall-clock ordering.
    /// A locally changed task is accepted only when the disk still has the revision it changed
    /// from; otherwise the caller receives a typed save failure instead of overwriting a newer
    /// concurrent mutation.
    pub(crate) fn merge_for_save(&mut self, disk: &DomainState) -> Result<(), String> {
        for incoming in &disk.tasks {
            let Some(local) = self.tasks.iter_mut().find(|task| task.id == incoming.id) else {
                self.tasks.push(incoming.clone());
                continue;
            };
            if local.revision == incoming.revision || local.merge_base_revision.is_none() {
                // This local copy did not mutate the task, so fresh disk state wins.
                *local = incoming.clone();
            } else if local.merge_base_revision == incoming.revision
                || (local.merge_base_revision == Some(LEGACY_MERGE_BASE_REVISION)
                    && incoming.revision.is_none())
            {
                // The disk still holds exactly the version this local mutation was based on.
            } else {
                return Err(format!("task {} changed during save", local.id));
            }
        }
        self.next_task_number = self.next_task_number.max(disk.next_task_number);
        self.merge_undo_entries(disk);
        self.merge_attempts_for_save(disk);
        Ok(())
    }

    /// Assign missing task numbers in deterministic creation order. The store calls this only
    /// under its exclusive lock, immediately before the durable replacement.
    pub(crate) fn assign_numbers_for_persistence(&mut self) {
        let next_after_existing = self
            .tasks
            .iter()
            .filter_map(|task| task.number)
            .max()
            .and_then(|number| number.checked_add(1))
            .unwrap_or(1);
        self.next_task_number = self.next_task_number.max(next_after_existing);
        let mut missing: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| task.number.is_none().then_some(index))
            .collect();
        missing.sort_by_key(|&index| (self.tasks[index].created_at, self.tasks[index].id));
        for index in missing {
            self.tasks[index].number = Some(self.next_task_number);
            self.next_task_number = self
                .next_task_number
                .checked_add(1)
                .expect("task number exhausted");
        }
    }

    pub(crate) fn sync_numbers_from_persisted(&mut self, persisted: &DomainState) {
        self.next_task_number = persisted.next_task_number;
        for task in &mut self.tasks {
            task.number = persisted
                .get(task.id)
                .and_then(|persisted_task| persisted_task.number);
        }
    }

    pub(crate) fn clear_merge_bases(&mut self) {
        for task in &mut self.tasks {
            task.merge_base_revision = None;
        }
    }

    fn merge_undo_entries(&mut self, other: &DomainState) {
        for incoming in &other.undo_stack {
            if !self.undo_stack.contains(incoming) {
                self.undo_stack.push(incoming.clone());
            }
        }
    }

    /// Disk owns dispatch-attempt lifecycle under the store lock. Unlike tasks, attempts are
    /// physically removed on completion or cleanup, so a stale board copy must not recreate one
    /// that no longer exists on disk.
    fn merge_attempts_for_save(&mut self, other: &DomainState) {
        self.active_attempts
            .retain(|local| other.active_attempt(local.id()).is_some());
        self.merge_attempts_from_disk(other);
    }

    fn merge_attempts_from_disk(&mut self, other: &DomainState) {
        for incoming in &other.active_attempts {
            match self
                .active_attempts
                .iter_mut()
                .find(|attempt| attempt.id() == incoming.id())
            {
                Some(existing) if existing.revision() != incoming.revision() => {
                    *existing = incoming.clone();
                }
                Some(_) => {}
                None => self.active_attempts.push(incoming.clone()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_sample(state: &mut DomainState) -> Uuid {
        state
            .create(
                "Fix flake",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("valid title creates a task")
    }

    #[test]
    fn merge_prefers_higher_attempt_revision_when_timestamps_are_equal() {
        let task_id = Uuid::new_v4();
        let base = DispatchAttempt::start(task_id, DispatchAttemptMode::Here, "grok");
        let existing = base
            .transition(
                base.revision(),
                DispatchAttemptTransition::Fail {
                    message: "older transition".into(),
                },
            )
            .expect("first transition");
        let incoming = existing
            .transition(existing.revision(), DispatchAttemptTransition::Resume)
            .expect("newer transition");
        let equal_timestamp = serde_json::json!([1_700_000_000_u64, 0_u32]);
        let with_timestamp = |attempt: DispatchAttempt| {
            let mut json = serde_json::to_value(attempt).expect("serialize attempt");
            json["updated_at"] = equal_timestamp.clone();
            serde_json::from_value(json).expect("deserialize attempt")
        };
        let existing = with_timestamp(existing);
        let incoming = with_timestamp(incoming);

        let mut local = DomainState::new();
        local.active_attempts.push(existing);
        let mut disk = DomainState::new();
        disk.active_attempts.push(incoming.clone());

        local.merge_tasks_from_disk(&disk);

        assert_eq!(local.active_attempts(), &[incoming]);
    }

    #[test]
    fn missing_format_version_loads_as_legacy() {
        let state: DomainState = serde_json::from_value(serde_json::json!({
            "tasks": [],
            "undo_stack": []
        }))
        .expect("legacy document");
        assert_eq!(state.format_version(), LEGACY_STORE_FORMAT_VERSION);
        assert_eq!(state.format_version(), 1);
    }

    #[test]
    fn create_rejects_whitespace_only_title_and_adds_no_task() {
        let mut state = DomainState::new();
        let err = state
            .create(
                "   \t\n  ",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect_err("whitespace-only title must fail");
        assert_eq!(err, DomainError::EmptyTitle);
        assert!(state.tasks().is_empty());
    }

    #[test]
    fn create_with_title_yields_one_todo_task_without_notes() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        assert_eq!(state.tasks().len(), 1);
        let task = &state.tasks()[0];
        assert_eq!(task.id, id);
        assert_eq!(task.title, "Fix flake");
        assert_eq!(task.status, HumanStatus::Ready);
        assert_eq!(task.notes, None);
        assert!(!task.soft_deleted);
        assert_eq!(task.scope, TaskScope::Global);
    }

    #[test]
    fn create_with_notes_stores_notes() {
        let mut state = DomainState::new();
        state
            .create(
                "Fix flake",
                Some("flaky under load".into()),
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("valid title creates a task");
        assert_eq!(state.tasks()[0].notes.as_deref(), Some("flaky under load"));
    }

    #[test]
    fn create_with_capture_origin_stores_capture() {
        let mut state = DomainState::new();
        let id = state
            .create(
                "From capture",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Capture,
            )
            .expect("valid title creates a task");
        assert_eq!(
            state.get(id).expect("task exists").provenance,
            ProvenanceOrigin::Capture
        );
    }

    #[test]
    fn create_with_selection_origin_stores_selection() {
        let mut state = DomainState::new();
        let id = state
            .create(
                "From selection",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Selection,
            )
            .expect("valid title creates a task");
        assert_eq!(
            state.get(id).expect("task exists").provenance,
            ProvenanceOrigin::Selection
        );
    }

    #[test]
    fn mutations_append_matching_events() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);

        let kinds = |state: &DomainState| -> Vec<TaskEventKind> {
            state
                .get(id)
                .expect("task exists")
                .history
                .iter()
                .map(|e| e.kind)
                .collect()
        };

        assert_eq!(kinds(&state), vec![TaskEventKind::Created]);

        state
            .edit(id, "Edited title", None, TaskScope::Global, None)
            .expect("edit");
        assert!(kinds(&state).contains(&TaskEventKind::Edited));

        state
            .set_status(id, HumanStatus::Started)
            .expect("set_status");
        assert!(kinds(&state).contains(&TaskEventKind::StatusSet));

        state.complete(id).expect("complete");
        assert!(kinds(&state).contains(&TaskEventKind::Completed));

        state.reopen(id).expect("reopen");
        assert!(kinds(&state).contains(&TaskEventKind::Reopened));

        state.soft_delete(id).expect("soft_delete");
        assert!(kinds(&state).contains(&TaskEventKind::SoftDeleted));

        state.restore(id).expect("restore");
        assert!(kinds(&state).contains(&TaskEventKind::Restored));

        // Full ordered history for the sequence above.
        assert_eq!(
            kinds(&state),
            vec![
                TaskEventKind::Created,
                TaskEventKind::Edited,
                TaskEventKind::StatusSet,
                TaskEventKind::Completed,
                TaskEventKind::Reopened,
                TaskEventKind::SoftDeleted,
                TaskEventKind::Restored,
            ]
        );
    }

    #[test]
    fn every_semantic_mutation_refreshes_the_opaque_revision() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        let mut previous = state.get(id).expect("task").revision.expect("revision");

        let mut assert_refreshed = |state: &DomainState| {
            let current = state.get(id).expect("task").revision.expect("revision");
            assert_ne!(current, previous);
            previous = current;
        };

        state
            .edit(id, "Edited", None, TaskScope::Global, None)
            .expect("edit");
        assert_refreshed(&state);
        state.set_status(id, HumanStatus::Started).expect("status");
        assert_refreshed(&state);
        state.complete(id).expect("complete");
        assert_refreshed(&state);
        state.reopen(id).expect("reopen");
        assert_refreshed(&state);
        state.soft_delete(id).expect("soft delete");
        assert_refreshed(&state);
        state.restore(id).expect("restore");
        assert_refreshed(&state);
    }

    #[test]
    fn restore_clears_soft_deleted() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.soft_delete(id).expect("soft_delete");
        assert!(state.get(id).expect("task exists").soft_deleted);
        state.restore(id).expect("restore");
        assert!(!state.get(id).expect("task exists").soft_deleted);
    }

    #[test]
    fn set_status_accepts_each_human_status() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        for status in [
            HumanStatus::Ready,
            HumanStatus::Started,
            HumanStatus::Blocked,
            HumanStatus::Review,
            HumanStatus::Done,
        ] {
            state
                .set_status(id, status)
                .expect("set_status must accept every human status");
            assert_eq!(state.get(id).expect("task exists").status, status);
        }
    }

    #[test]
    fn complete_sets_status_done() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.complete(id).expect("complete known id");
        assert_eq!(
            state.get(id).expect("task exists").status,
            HumanStatus::Done
        );
    }

    #[test]
    fn reopen_on_done_sets_status_todo() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.complete(id).expect("complete known id");
        state.reopen(id).expect("reopen known id");
        assert_eq!(
            state.get(id).expect("task exists").status,
            HumanStatus::Ready
        );
    }

    #[test]
    fn soft_delete_flags_task_but_keeps_it_in_store() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.soft_delete(id).expect("soft_delete known id");
        assert_eq!(state.tasks().len(), 1);
        let task = state.get(id).expect("soft-deleted task still gettable");
        assert!(task.soft_deleted);
    }

    #[test]
    fn edit_updates_title_notes_and_scope() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        let project = TaskScope::Project {
            path: "/home/me/proj".into(),
        };
        state
            .edit(
                id,
                "  New title  ",
                Some("updated notes".into()),
                project.clone(),
                None,
            )
            .expect("edit known id");
        let task = state.get(id).expect("task exists");
        assert_eq!(task.title, "New title");
        assert_eq!(task.notes.as_deref(), Some("updated notes"));
        assert_eq!(task.scope, project);
    }

    #[test]
    fn edit_carrying_thread_journals_one_event_and_bumps_revision_once() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        let before = state.get(id).expect("task").clone();

        state
            .edit(
                id,
                "Edited title",
                Some("edited notes".into()),
                TaskScope::Project {
                    path: "/repos/threads".into(),
                },
                Some("release-2026".into()),
            )
            .expect("edit carrying thread");

        let task = state.get(id).expect("task");
        assert_eq!(task.title, "Edited title");
        assert_eq!(task.notes.as_deref(), Some("edited notes"));
        assert_eq!(task.thread.as_deref(), Some("release-2026"));
        assert_eq!(
            task.scope,
            TaskScope::Project {
                path: "/repos/threads".into(),
            }
        );
        assert_eq!(task.history.len(), before.history.len() + 1);
        assert_eq!(
            task.history.last().map(|event| event.kind),
            Some(TaskEventKind::Edited)
        );
        assert_ne!(task.revision, before.revision);
    }

    #[test]
    fn commands_reject_unknown_id() {
        let mut state = DomainState::new();
        let missing = Uuid::new_v4();
        assert_eq!(
            state.set_status(missing, HumanStatus::Started),
            Err(DomainError::UnknownId(missing))
        );
        assert_eq!(
            state.complete(missing),
            Err(DomainError::UnknownId(missing))
        );
        assert_eq!(state.reopen(missing), Err(DomainError::UnknownId(missing)));
        assert_eq!(
            state.soft_delete(missing),
            Err(DomainError::UnknownId(missing))
        );
        assert_eq!(state.restore(missing), Err(DomainError::UnknownId(missing)));
        assert_eq!(
            state.edit(missing, "x", None, TaskScope::Global, None),
            Err(DomainError::UnknownId(missing))
        );
    }

    #[test]
    fn steps_mutations_journal_events_and_bump_revision() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        let mut previous_revision = state.get(id).expect("task").revision.expect("revision");
        let mut previous_len = state.get(id).expect("task").history.len();

        let mut assert_journaled = |state: &DomainState, kind: TaskEventKind| {
            let task = state.get(id).expect("task exists");
            let revision = task.revision.expect("revision");
            assert_ne!(revision, previous_revision, "revision must change");
            previous_revision = revision;
            assert_eq!(
                task.history.len(),
                previous_len + 1,
                "exactly one history event must be appended"
            );
            previous_len = task.history.len();
            assert_eq!(task.history.last().expect("event").kind, kind);
        };

        let step = state.add_step(id, "  First step  ").expect("add step");
        assert_journaled(&state, TaskEventKind::StepAdded);
        assert_eq!(
            state.get(id).expect("task").steps,
            vec![Step {
                id: step,
                text: "First step".into(),
                done: false,
            }],
            "add must trim and append at the end"
        );

        state.toggle_step(id, step).expect("toggle on");
        assert_journaled(&state, TaskEventKind::StepChecked);
        assert!(state.get(id).expect("task").steps[0].done);

        state.toggle_step(id, step).expect("toggle off");
        assert_journaled(&state, TaskEventKind::StepUnchecked);
        assert!(!state.get(id).expect("task").steps[0].done);

        state
            .rename_step(id, step, "  Renamed step  ")
            .expect("rename steps step");
        assert_journaled(&state, TaskEventKind::StepRenamed);
        assert_eq!(
            state.get(id).expect("task").steps[0].text,
            "Renamed step",
            "rename must trim"
        );

        state.remove_step(id, step).expect("remove step");
        assert_journaled(&state, TaskEventKind::StepRemoved);
        assert!(state.get(id).expect("task").steps.is_empty());
    }

    #[test]
    fn toggle_step_keeps_human_status_including_completing_last_step() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state
            .set_status(id, HumanStatus::Started)
            .expect("set_status");
        let step = state.add_step(id, "Only step").expect("add step");
        state
            .toggle_step(id, step)
            .expect("toggle the only step done");
        let task = state.get(id).expect("task exists");
        assert!(task.steps[0].done, "the only step is now done");
        assert_eq!(
            task.status,
            HumanStatus::Started,
            "completing the steps must never change human status"
        );
    }

    #[test]
    fn step_commands_reject_unknown_ids_and_empty_text() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        let step = state.add_step(id, "Step").expect("add step");
        let missing_task = Uuid::new_v4();
        let missing_item = Uuid::new_v4();

        assert_eq!(
            state.add_step(missing_task, "x"),
            Err(DomainError::UnknownId(missing_task))
        );
        assert_eq!(
            state.toggle_step(missing_task, step),
            Err(DomainError::UnknownId(missing_task))
        );
        assert_eq!(
            state.rename_step(missing_task, step, "x"),
            Err(DomainError::UnknownId(missing_task))
        );
        assert_eq!(
            state.remove_step(missing_task, step),
            Err(DomainError::UnknownId(missing_task))
        );
        assert_eq!(
            state.toggle_step(id, missing_item),
            Err(DomainError::UnknownStep(missing_item))
        );
        assert_eq!(
            state.rename_step(id, missing_item, "x"),
            Err(DomainError::UnknownStep(missing_item))
        );
        assert_eq!(
            state.remove_step(id, missing_item),
            Err(DomainError::UnknownStep(missing_item))
        );
        assert_eq!(state.add_step(id, "   "), Err(DomainError::EmptyStepText));
        assert_eq!(
            state.rename_step(id, step, "  "),
            Err(DomainError::EmptyStepText)
        );

        let task = state.get(id).expect("task exists");
        assert_eq!(
            task.steps,
            vec![Step {
                id: step,
                text: "Step".into(),
                done: false,
            }],
            "refused commands must not mutate the steps"
        );
        assert_eq!(
            task.history.len(),
            2,
            "no journal writes for refused commands"
        );
    }

    #[test]
    fn locked_create_assigns_number_one_then_two() {
        let dir = std::env::temp_dir().join(format!("tsk-number-domain-{}", Uuid::new_v4()));
        let store = crate::store::TaskStore::new(&dir);
        let first = store
            .locked_transition(|state| {
                state
                    .create(
                        "first",
                        None,
                        TaskScope::Global,
                        None,
                        None,
                        ProvenanceOrigin::Manual,
                    )
                    .map_err(|error| error.to_string())
            })
            .expect("persist first");
        let second = store
            .locked_transition(|state| {
                state
                    .create(
                        "second",
                        None,
                        TaskScope::Global,
                        None,
                        None,
                        ProvenanceOrigin::Manual,
                    )
                    .map_err(|error| error.to_string())
            })
            .expect("persist second");
        let state = store.load().expect("load");
        assert_eq!(state.get(first).and_then(|task| task.number), Some(1));
        assert_eq!(state.get(second).and_then(|task| task.number), Some(2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn edit_status_scope_thread_complete_soft_delete_leave_number_unchanged() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.assign_numbers_for_persistence();
        let number = state.get(id).and_then(|task| task.number);
        state
            .edit(
                id,
                "edited",
                None,
                TaskScope::Project {
                    path: "/project".into(),
                },
                Some("thread".into()),
            )
            .expect("edit");
        state.set_status(id, HumanStatus::Started).expect("status");
        state.complete(id).expect("complete");
        state.soft_delete(id).expect("delete");
        assert_eq!(state.get(id).and_then(|task| task.number), number);
    }

    #[test]
    fn undo_of_complete_and_of_soft_delete_keeps_the_same_number() {
        let mut state = DomainState::new();
        let id = create_sample(&mut state);
        state.assign_numbers_for_persistence();
        let number = state.get(id).and_then(|task| task.number);
        state.complete(id).expect("complete");
        state.undo().expect("undo complete");
        state.soft_delete(id).expect("delete");
        state.undo().expect("undo delete");
        assert_eq!(state.get(id).and_then(|task| task.number), number);
    }

    #[test]
    fn ready_and_started_load_legacy_todo_and_doing_and_write_new_names() {
        assert_eq!(
            serde_json::from_str::<HumanStatus>("\"todo\"").expect("todo"),
            HumanStatus::Ready
        );
        assert_eq!(
            serde_json::from_str::<HumanStatus>("\"doing\"").expect("doing"),
            HumanStatus::Started
        );
        assert_eq!(
            serde_json::to_string(&HumanStatus::Ready).expect("ready"),
            "\"ready\""
        );
        assert_eq!(
            serde_json::to_string(&HumanStatus::Started).expect("started"),
            "\"started\""
        );
    }
}
