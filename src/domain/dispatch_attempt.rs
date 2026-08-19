//! Durable dispatch-attempt journal with explicit owned-resource receipts.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AgentSessionIdentity;

/// The dispatch path an attempt was created to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchAttemptMode {
    Here,
    NewWorktree,
}

/// One ordered dispatch step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchAttemptStep {
    CreateWorktree,
    OpenPane,
    StartAgent,
    SendPrompt,
}

/// Durable completion state for one ordered dispatch step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchAttemptStepState {
    pub step: DispatchAttemptStep,
    pub completed: bool,
}

/// Current recovery phase for an active attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchAttemptPhase {
    Dispatching,
    Failed,
    Cleaning,
}

/// Exact receipt for a worktree created by this plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeReceipt {
    /// Human-readable checkout path, retained for recovery presentation only.
    pub path: String,
    /// Opaque host workspace identity required for removal.
    pub workspace_id: String,
}

/// Exact receipt for a pane created by this plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneReceipt {
    /// Opaque host pane identity required for removal.
    pub pane_id: String,
}

/// Exact identity returned for an agent started in a plugin-owned pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReceipt {
    pub pane_id: String,
    /// Host-reported display name, retained separately from stable identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<AgentSessionIdentity>,
}

/// A resource that this attempt may later offer for explicit cleanup.
///
/// Values are receipts, never ambient paths or inferred host identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnedResourceReceipt {
    Worktree(WorktreeReceipt),
    Pane(PaneReceipt),
    Agent(AgentReceipt),
}

impl OwnedResourceReceipt {
    fn completing_step(&self) -> DispatchAttemptStep {
        match self {
            Self::Worktree(_) => DispatchAttemptStep::CreateWorktree,
            Self::Pane(_) => DispatchAttemptStep::OpenPane,
            Self::Agent(_) => DispatchAttemptStep::StartAgent,
        }
    }
}

/// A transition that produces one new attempt revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchAttemptTransition {
    RecordReceipt(OwnedResourceReceipt),
    CompleteStep(DispatchAttemptStep),
    Fail {
        message: String,
    },
    /// Resume a failed attempt from its first incomplete step.
    Resume,
    /// Resume a failed attempt from `step`, discarding this and later receipts.
    /// Used after a host confirmation shows a previously recorded resource is gone.
    RestartFrom(DispatchAttemptStep),
    BeginCleanup,
    /// Return a cleanup attempt to Failed while retaining every unremoved receipt.
    CleanupFailed {
        message: String,
    },
    RemoveOwnedReceipt(OwnedResourceReceipt),
    /// Remove receipts that one host operation disposed together (the agent is owned by
    /// its pane, so closing the pane removes both records in one durable write).
    RemoveOwnedReceipts(Vec<OwnedResourceReceipt>),
}

/// A durable active dispatch attempt. Completed and fully-cleaned attempts are removed
/// from the containing [`super::DomainState`] rather than retained as history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchAttempt {
    id: Uuid,
    task_id: Uuid,
    mode: DispatchAttemptMode,
    kind: String,
    steps: Vec<DispatchAttemptStepState>,
    owned_resources: Vec<OwnedResourceReceipt>,
    phase: DispatchAttemptPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
    revision: Uuid,
    #[serde(with = "super::time_serde")]
    updated_at: SystemTime,
}

impl DispatchAttempt {
    /// Start an empty attempt before any host side effect occurs.
    pub fn start(task_id: Uuid, mode: DispatchAttemptMode, kind: impl Into<String>) -> Self {
        let mut steps = Vec::new();
        if mode == DispatchAttemptMode::NewWorktree {
            steps.push(DispatchAttemptStepState {
                step: DispatchAttemptStep::CreateWorktree,
                completed: false,
            });
        }
        for step in [
            DispatchAttemptStep::OpenPane,
            DispatchAttemptStep::StartAgent,
            DispatchAttemptStep::SendPrompt,
        ] {
            steps.push(DispatchAttemptStepState {
                step,
                completed: false,
            });
        }
        Self {
            id: Uuid::new_v4(),
            task_id,
            mode,
            kind: kind.into(),
            steps,
            owned_resources: Vec::new(),
            phase: DispatchAttemptPhase::Dispatching,
            last_error: None,
            revision: Uuid::new_v4(),
            updated_at: SystemTime::now(),
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn task_id(&self) -> Uuid {
        self.task_id
    }

    pub fn mode(&self) -> DispatchAttemptMode {
        self.mode
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn steps(&self) -> &[DispatchAttemptStepState] {
        &self.steps
    }

    pub fn owned_resources(&self) -> &[OwnedResourceReceipt] {
        &self.owned_resources
    }

    pub fn phase(&self) -> DispatchAttemptPhase {
        self.phase
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn revision(&self) -> Uuid {
        self.revision
    }

    pub fn updated_at(&self) -> SystemTime {
        self.updated_at
    }

    /// Validate and apply one transition, returning a replacement attempt. Callers must
    /// persist the replacement before starting another resource-creating host step.
    pub fn transition(
        &self,
        expected_revision: Uuid,
        transition: DispatchAttemptTransition,
    ) -> Result<Self, DispatchAttemptError> {
        if expected_revision != self.revision {
            return Err(DispatchAttemptError::StaleRevision {
                expected: expected_revision,
                current: self.revision,
            });
        }

        let mut next = self.clone();
        match transition {
            DispatchAttemptTransition::RecordReceipt(receipt) => {
                next.require_dispatching()?;
                let step = receipt.completing_step();
                next.require_next_step(step)?;
                if next.owned_resources.contains(&receipt) {
                    return Err(DispatchAttemptError::DuplicateReceipt);
                }
                next.owned_resources.push(receipt);
                next.mark_completed(step);
            }
            DispatchAttemptTransition::CompleteStep(step) => {
                next.require_dispatching()?;
                next.require_next_step(step)?;
                if matches!(
                    step,
                    DispatchAttemptStep::CreateWorktree
                        | DispatchAttemptStep::OpenPane
                        | DispatchAttemptStep::StartAgent
                ) {
                    return Err(DispatchAttemptError::ReceiptRequired(step));
                }
                next.mark_completed(step);
            }
            DispatchAttemptTransition::Fail { message } => {
                next.require_dispatching()?;
                next.phase = DispatchAttemptPhase::Failed;
                next.last_error = Some(message);
            }
            DispatchAttemptTransition::Resume => {
                if next.phase != DispatchAttemptPhase::Failed {
                    return Err(DispatchAttemptError::InvalidPhase {
                        expected: DispatchAttemptPhase::Failed,
                        actual: next.phase,
                    });
                }
                next.phase = DispatchAttemptPhase::Dispatching;
                next.last_error = None;
            }
            DispatchAttemptTransition::RestartFrom(step) => {
                if next.phase != DispatchAttemptPhase::Failed {
                    return Err(DispatchAttemptError::InvalidPhase {
                        expected: DispatchAttemptPhase::Failed,
                        actual: next.phase,
                    });
                }
                let Some(index) = next.steps.iter().position(|state| state.step == step) else {
                    return Err(DispatchAttemptError::OutOfOrder {
                        expected: next
                            .steps
                            .iter()
                            .find(|state| !state.completed)
                            .map(|state| state.step),
                        actual: step,
                    });
                };
                next.steps[index..]
                    .iter_mut()
                    .for_each(|state| state.completed = false);
                let retained_steps = next.steps[..index]
                    .iter()
                    .map(|state| state.step)
                    .collect::<Vec<_>>();
                next.owned_resources
                    .retain(|receipt| retained_steps.contains(&receipt.completing_step()));
                next.phase = DispatchAttemptPhase::Dispatching;
                next.last_error = None;
            }
            DispatchAttemptTransition::BeginCleanup => {
                if next.phase != DispatchAttemptPhase::Failed {
                    return Err(DispatchAttemptError::InvalidPhase {
                        expected: DispatchAttemptPhase::Failed,
                        actual: next.phase,
                    });
                }
                next.phase = DispatchAttemptPhase::Cleaning;
            }
            DispatchAttemptTransition::CleanupFailed { message } => {
                if next.phase != DispatchAttemptPhase::Cleaning {
                    return Err(DispatchAttemptError::InvalidPhase {
                        expected: DispatchAttemptPhase::Cleaning,
                        actual: next.phase,
                    });
                }
                next.phase = DispatchAttemptPhase::Failed;
                next.last_error = Some(message);
            }
            DispatchAttemptTransition::RemoveOwnedReceipt(receipt) => {
                next.remove_owned_receipts(&[receipt])?;
            }
            DispatchAttemptTransition::RemoveOwnedReceipts(receipts) => {
                next.remove_owned_receipts(&receipts)?;
            }
        }
        next.revision = Uuid::new_v4();
        next.updated_at = SystemTime::now();
        Ok(next)
    }

    fn remove_owned_receipts(
        &mut self,
        receipts: &[OwnedResourceReceipt],
    ) -> Result<(), DispatchAttemptError> {
        if self.phase != DispatchAttemptPhase::Cleaning {
            return Err(DispatchAttemptError::InvalidPhase {
                expected: DispatchAttemptPhase::Cleaning,
                actual: self.phase,
            });
        }
        if receipts.is_empty()
            || receipts
                .iter()
                .any(|receipt| !self.owned_resources.contains(receipt))
        {
            return Err(DispatchAttemptError::UnownedReceipt);
        }
        self.owned_resources
            .retain(|owned| !receipts.contains(owned));
        Ok(())
    }

    fn require_dispatching(&self) -> Result<(), DispatchAttemptError> {
        if self.phase == DispatchAttemptPhase::Dispatching {
            Ok(())
        } else {
            Err(DispatchAttemptError::InvalidPhase {
                expected: DispatchAttemptPhase::Dispatching,
                actual: self.phase,
            })
        }
    }

    fn require_next_step(&self, step: DispatchAttemptStep) -> Result<(), DispatchAttemptError> {
        let next = self
            .steps
            .iter()
            .find(|state| !state.completed)
            .map(|state| state.step);
        if next == Some(step) {
            Ok(())
        } else {
            Err(DispatchAttemptError::OutOfOrder {
                expected: next,
                actual: step,
            })
        }
    }

    fn mark_completed(&mut self, step: DispatchAttemptStep) {
        self.steps
            .iter_mut()
            .find(|state| state.step == step)
            .expect("validated current step exists")
            .completed = true;
    }
}

/// Refusal to alter a dispatch attempt. Every error leaves the stored attempt unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchAttemptError {
    StaleRevision {
        expected: Uuid,
        current: Uuid,
    },
    InvalidPhase {
        expected: DispatchAttemptPhase,
        actual: DispatchAttemptPhase,
    },
    OutOfOrder {
        expected: Option<DispatchAttemptStep>,
        actual: DispatchAttemptStep,
    },
    DuplicateReceipt,
    ReceiptRequired(DispatchAttemptStep),
    UnownedReceipt,
}

impl std::fmt::Display for DispatchAttemptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { .. } => {
                write!(f, "dispatch attempt changed since this transition")
            }
            Self::InvalidPhase { .. } => write!(f, "dispatch attempt is not in the required phase"),
            Self::OutOfOrder { .. } => write!(f, "dispatch attempt transition is out of order"),
            Self::DuplicateReceipt => write!(f, "dispatch resource receipt is already owned"),
            Self::ReceiptRequired(_) => {
                write!(f, "resource-creating step requires an exact receipt")
            }
            Self::UnownedReceipt => {
                write!(f, "dispatch resource receipt is not owned by this attempt")
            }
        }
    }
}

impl std::error::Error for DispatchAttemptError {}
