//! Provenance origin and append-only task event history.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// How a task was created. Allowed set includes at least these three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceOrigin {
    Manual,
    Capture,
    Selection,
}

/// Kind of domain mutation recorded on a task's event history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEventKind {
    Created,
    Edited,
    StatusSet,
    Completed,
    Reopened,
    SoftDeleted,
    Restored,
    /// Context capsule / agent meta refreshed on an existing task.
    Parked,
    /// Explicit agent/pane link set.
    AgentLinked,
    /// Explicit agent/pane link cleared.
    AgentUnlinked,
    /// Successful dispatch: agent started and linked.
    Dispatched,
    /// Step appended to the task. Old stores name this `checklist_item_added`.
    #[serde(alias = "checklist_item_added")]
    StepAdded,
    /// Step flipped to done. Old stores name this `checklist_item_checked`.
    #[serde(alias = "checklist_item_checked")]
    StepChecked,
    /// Step flipped back to open. Old stores name this `checklist_item_unchecked`.
    #[serde(alias = "checklist_item_unchecked")]
    StepUnchecked,
    /// Step text changed. Old stores name this `checklist_item_renamed`.
    #[serde(alias = "checklist_item_renamed")]
    StepRenamed,
    /// Step removed from the task. Old stores name this `checklist_item_removed`.
    #[serde(alias = "checklist_item_removed")]
    StepRemoved,
}

/// One append-only history record on a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEvent {
    pub kind: TaskEventKind,
    #[serde(with = "super::time_serde")]
    pub at: SystemTime,
}
