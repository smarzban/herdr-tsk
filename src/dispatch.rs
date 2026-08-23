//! Dispatch Orchestrator: resolve cwd, plan host steps, execute.
//!
//! Pure resolve/plan have no host I/O. Execute calls host ports then domain
//! [`crate::domain::DomainState::after_dispatch_success`] only on full success.
//! Worktree create runs only for [`DispatchMode::NewWorktree`]. Never auto-dispatches
//! without a caller intent.

use std::env;
use std::path::Path;

use uuid::Uuid;

use crate::domain::{
    AgentMeta, AgentSessionIdentity, ContextCapsule, DispatchAttemptMode, DispatchAttemptPhase,
    DispatchAttemptStep, DispatchAttemptTransition, DomainState, OwnedResourceReceipt, Task,
    TaskScope,
};
use crate::host::HostPorts;
use crate::store::TaskStateStore;
use crate::text::non_empty;

/// Env override for dispatch agent kind (default `grok` when unset/empty).
pub const DISPATCH_AGENT_ENV: &str = "TSK_DISPATCH_AGENT";

/// Default agent kind when [`DISPATCH_AGENT_ENV`] is unset or blank.
pub const DEFAULT_DISPATCH_AGENT: &str = "grok";

/// Fixed supported agent kinds for board cycle (subset of herdr `agent start --kind`).
pub const SUPPORTED_DISPATCH_KINDS: &[&str] = &["grok", "claude", "pi", "codex", "opencode"];

/// Cycle to the next entry in [`SUPPORTED_DISPATCH_KINDS`] (wraps).
///
/// Unknown/current-not-in-list starts after `grok` (index 0 → next is `claude`).
pub fn cycle_dispatch_kind(current: &str) -> String {
    let idx = SUPPORTED_DISPATCH_KINDS
        .iter()
        .position(|&k| k.eq_ignore_ascii_case(current.trim()))
        .unwrap_or(0);
    let next = (idx + 1) % SUPPORTED_DISPATCH_KINDS.len();
    SUPPORTED_DISPATCH_KINDS[next].to_string()
}

/// Where to start the agent relative to the task's project tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    /// Open shell at existing task cwd (no worktree create).
    Here,
    /// Create a new git worktree, then open shell at the checkout path.
    NewWorktree,
}

/// Host steps for a dispatch plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchStep {
    /// Host `worktree create` (only for [`DispatchMode::NewWorktree`]).
    CreateWorktree { repo_cwd: String, branch: String },
    /// Split/open a shell pane rooted at `cwd`.
    OpenShell { cwd: String },
    /// `herdr agent start` with kind on the new pane.
    StartAgent { kind: String, name: String },
    /// Optional first prompt (title ± notes).
    Prompt { text: String },
}

/// Outcome of dispatch execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchResult {
    Dispatched {
        pane_id: String,
        agent: Option<String>,
    },
    Failed {
        message: String,
    },
}

impl DispatchResult {
    pub fn is_success(&self) -> bool {
        matches!(self, DispatchResult::Dispatched { .. })
    }

    pub fn message(&self) -> String {
        match self {
            DispatchResult::Dispatched { pane_id, agent } => match agent {
                Some(a) => format!("dispatched · agent {a} · pane {pane_id}"),
                None => format!("dispatched · pane {pane_id}"),
            },
            DispatchResult::Failed { message } => message.clone(),
        }
    }
}

/// Resolve dispatch agent kind from env. Non-empty `TSK_DISPATCH_AGENT`
/// wins; otherwise [`DEFAULT_DISPATCH_AGENT`].
pub fn dispatch_agent_kind_from_env() -> String {
    dispatch_agent_kind(env::var(DISPATCH_AGENT_ENV).ok())
}

/// Pure kind resolution (tests inject env value).
pub fn dispatch_agent_kind(env_value: Option<String>) -> String {
    env_value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_DISPATCH_AGENT.to_string())
}

/// Resolve working directory for dispatch (AC dispatch cwd).
///
/// Order: project scope path if [`TaskScope::Project`], else capsule
/// `worktree_path` → `repo_path` → `cwd`. First usable (existing directory) wins.
/// Fails with a non-empty message when none is usable.
pub fn resolve_dispatch_cwd(
    task: &Task,
    mut path_usable: impl FnMut(&str) -> bool,
) -> Result<String, String> {
    if let TaskScope::Project { path } = &task.scope {
        if let Some(p) = non_empty(Some(path.as_str())) {
            if path_usable(p) {
                return Ok(p.to_string());
            }
        }
    }
    path_from_capsule(task.capsule.as_ref(), &mut path_usable)
}

fn path_from_capsule(
    capsule: Option<&ContextCapsule>,
    path_usable: &mut impl FnMut(&str) -> bool,
) -> Result<String, String> {
    let Some(capsule) = capsule else {
        return Err("no usable dispatch cwd (no project path or capsule)".into());
    };
    for candidate in [
        capsule.worktree_path.as_deref(),
        capsule.repo_path.as_deref(),
        capsule.cwd.as_deref(),
    ] {
        if let Some(path) = non_empty(candidate) {
            if path_usable(path) {
                return Ok(path.to_string());
            }
        }
    }
    Err("no usable dispatch cwd (project/capsule paths missing or not directories)".into())
}

/// Whether a path is a usable dispatch cwd (existing directory).
pub fn default_path_usable(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// Branch name for a new dispatch worktree: `tasks/<8hex>`.
pub fn worktree_branch_for_task(id: Uuid) -> String {
    let hex = id.as_simple().to_string();
    format!("tasks/{}", &hex[..8.min(hex.len())])
}

/// Build ordered host steps for dispatch.
///
/// `Here`: OpenShell at `cwd` → StartAgent → optional Prompt.
/// `NewWorktree`: CreateWorktree at `cwd` with `branch` → OpenShell at placeholder
/// (resolved at execute from create result) → StartAgent → optional Prompt.
///
/// For `NewWorktree`, `OpenShell.cwd` is a plan placeholder (`cwd` / repo root); execute
/// replaces it with the host-returned checkout path.
pub fn plan_dispatch(
    mode: DispatchMode,
    cwd: &str,
    kind: &str,
    agent_name: &str,
    branch: Option<&str>,
    prompt: Option<String>,
) -> Vec<DispatchStep> {
    let mut steps = Vec::new();
    if mode == DispatchMode::NewWorktree {
        let branch = branch.unwrap_or("tasks/unknown").to_string();
        steps.push(DispatchStep::CreateWorktree {
            repo_cwd: cwd.to_string(),
            branch,
        });
    }
    steps.push(DispatchStep::OpenShell {
        cwd: cwd.to_string(),
    });
    steps.push(DispatchStep::StartAgent {
        kind: kind.to_string(),
        name: agent_name.to_string(),
    });
    if let Some(text) = prompt
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        steps.push(DispatchStep::Prompt { text });
    }
    steps
}

/// Build optional initial prompt from task title and notes.
pub fn initial_prompt(task: &Task) -> String {
    match task
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(notes) => format!("{}\n\n{}", task.title, notes),
        None => task.title.clone(),
    }
}

/// Stable agent name for `herdr agent start` (lowercase letters/digits/hyphen).
pub fn agent_name_for_task(id: Uuid) -> String {
    let hex = id.as_simple().to_string();
    format!("t{}", &hex[..8.min(hex.len())])
}

/// True when the plan includes an explicit [`DispatchStep::CreateWorktree`].
pub fn plan_contains_worktree_create(steps: &[DispatchStep]) -> bool {
    steps
        .iter()
        .any(|s| matches!(s, DispatchStep::CreateWorktree { .. }))
}

/// Host-only dispatch outcome (no domain mutation). Safe to produce on a worker thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchHostSuccess {
    pub pane_id: String,
    pub agent: Option<String>,
    pub agent_session: Option<AgentSessionIdentity>,
    /// Present when agent start succeeded but the optional first prompt failed.
    pub prompt_warning: Option<String>,
}

/// Run host steps for dispatch without touching domain state.
///
/// Domain link/status must be applied on the UI thread via
/// [`DomainState::after_dispatch_success`] after this returns `Ok`.
pub fn run_dispatch_host(
    task: &Task,
    mode: DispatchMode,
    kind: &str,
    host: &(impl HostPorts + ?Sized),
) -> Result<DispatchHostSuccess, String> {
    if task.soft_deleted {
        return Err("cannot dispatch a deleted task".into());
    }

    let repo_cwd = resolve_dispatch_cwd(task, |p| host.path_usable(p) && Path::new(p).is_dir())?;

    let agent_name = agent_name_for_task(task.id);
    let branch = worktree_branch_for_task(task.id);
    let prompt = initial_prompt(task);
    let steps = plan_dispatch(
        mode,
        &repo_cwd,
        kind,
        &agent_name,
        Some(&branch),
        Some(prompt),
    );
    if mode == DispatchMode::Here {
        debug_assert!(!plan_contains_worktree_create(&steps));
    } else {
        debug_assert!(plan_contains_worktree_create(&steps));
    }

    let shell_cwd = match mode {
        DispatchMode::Here => repo_cwd.clone(),
        DispatchMode::NewWorktree => match host.create_worktree(&repo_cwd, &branch) {
            Ok(path) => path,
            Err(e) => {
                return Err(format!(
                    "dispatch failed (worktree create at {repo_cwd}): {e}"
                ));
            }
        },
    };

    let pane_id = match host.open_shell_at_cwd(&shell_cwd) {
        Ok(id) => id,
        Err(e) => {
            return Err(format!("dispatch failed (open shell at {shell_cwd}): {e}"));
        }
    };

    let started = match host.start_agent(&agent_name, kind, &pane_id) {
        Ok(s) => s,
        Err(e) => {
            return Err(format!("dispatch failed (agent start): {e}"));
        }
    };

    let pane_id = if started.pane_id.is_empty() {
        pane_id
    } else {
        started.pane_id.clone()
    };
    let agent = started
        .agent
        .or(Some(kind.to_string()))
        .filter(|s| !s.is_empty());
    let agent_session = started.agent_session;

    let mut prompt_warning: Option<String> = None;
    for step in &steps {
        if let DispatchStep::Prompt { text } = step {
            if let Err(e) = host.agent_prompt(&pane_id, text) {
                prompt_warning = Some(e);
            }
        }
    }

    Ok(DispatchHostSuccess {
        pane_id,
        agent,
        agent_session,
        prompt_warning,
    })
}

/// Execute dispatch against host ports; domain updates only on success.
///
/// On host failure: no link mutation. Prompt failure after a successful start still
/// counts as dispatched (agent is live and linked); message notes prompt issue.
///
/// Defaults to [`DispatchMode::Here`] for thin compatibility.
pub fn execute_dispatch(
    domain: &mut DomainState,
    task_id: Uuid,
    kind: &str,
    host: &(impl HostPorts + ?Sized),
) -> DispatchResult {
    execute_dispatch_mode(domain, task_id, DispatchMode::Here, kind, host)
}

/// Execute dispatch with an explicit mode (here vs new worktree).
pub fn execute_dispatch_mode(
    domain: &mut DomainState,
    task_id: Uuid,
    mode: DispatchMode,
    kind: &str,
    host: &(impl HostPorts + ?Sized),
) -> DispatchResult {
    let Some(task) = domain.get(task_id).cloned() else {
        return DispatchResult::Failed {
            message: "select a task to dispatch".into(),
        };
    };

    let host_ok = match run_dispatch_host(&task, mode, kind, host) {
        Ok(s) => s,
        Err(message) => return DispatchResult::Failed { message },
    };

    let meta = AgentMeta {
        agent_id: host_ok.agent.clone(),
        pane_id: Some(host_ok.pane_id.clone()),
        agent_session: host_ok.agent_session.clone(),
    };
    if let Err(e) = domain.after_dispatch_success(task_id, meta) {
        return DispatchResult::Failed {
            message: format!("dispatch host ok but domain update failed: {e}"),
        };
    }

    if let Some(e) = host_ok.prompt_warning {
        return DispatchResult::Dispatched {
            pane_id: format!("{} (prompt failed: {e})", host_ok.pane_id),
            agent: host_ok.agent,
        };
    }

    DispatchResult::Dispatched {
        pane_id: host_ok.pane_id,
        agent: host_ok.agent,
    }
}

/// An explicit recovery action available for an active dispatch attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchRecoveryAction {
    Resume,
    Cleanup,
}

/// Outcome of a durable dispatch recovery operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchRecoveryResult {
    Dispatched {
        pane_id: String,
        agent: Option<String>,
        /// The agent exists and was linked, but its optional initial prompt failed.
        prompt_warning: Option<String>,
    },
    /// Start was refused because this task already has a durable active attempt.
    RecoveryRequired {
        attempt_id: Uuid,
        actions: Vec<DispatchRecoveryAction>,
    },
    Failed {
        attempt_id: Uuid,
        message: String,
    },
    Cleaned {
        attempt_id: Uuid,
    },
    /// The host may have completed a step, but its receipt could not be saved. No later
    /// host operation was attempted, so the caller can surface this uncertainty safely.
    PersistenceUncertain {
        attempt_id: Uuid,
        message: String,
    },
    Error {
        message: String,
    },
}

/// Persist an empty attempt, then execute its host steps one at a time.
///
/// `TaskStore` is owned so callers can clone it into the board's existing worker thread.
/// Each receipt is saved before the next resource-creating host call.
pub fn start_dispatch_attempt<S: TaskStateStore>(
    store: S,
    task_id: Uuid,
    mode: DispatchMode,
    kind: &str,
    host: &(impl HostPorts + ?Sized),
) -> DispatchRecoveryResult {
    let mode = match mode {
        DispatchMode::Here => DispatchAttemptMode::Here,
        DispatchMode::NewWorktree => DispatchAttemptMode::NewWorktree,
    };
    enum StartOutcome {
        Started(Uuid),
        Existing(Uuid),
    }
    let outcome = match store.locked_transition(|state| {
        if let Some(attempt) = state.active_attempt_for_task(task_id) {
            return Ok(StartOutcome::Existing(attempt.id()));
        }
        state
            .start_dispatch_attempt(task_id, mode, kind)
            .map(StartOutcome::Started)
            .map_err(|error| error.to_string())
    }) {
        Ok(outcome) => outcome,
        Err(error) => {
            return DispatchRecoveryResult::Error {
                message: format!("could not persist dispatch attempt before host work: {error}"),
            }
        }
    };
    let attempt_id = match outcome {
        StartOutcome::Started(attempt_id) => attempt_id,
        StartOutcome::Existing(attempt_id) => {
            return DispatchRecoveryResult::RecoveryRequired {
                attempt_id,
                actions: vec![
                    DispatchRecoveryAction::Resume,
                    DispatchRecoveryAction::Cleanup,
                ],
            }
        }
    };
    let state = match store.load() {
        Ok(state) => state,
        Err(error) => {
            return DispatchRecoveryResult::Error {
                message: error.to_string(),
            }
        }
    };
    execute_attempt(&store, state, attempt_id, host)
}

/// Confirm durable receipts and continue a failed attempt at its first incomplete or
/// unconfirmed step. Confirmed resources are never recreated.
pub fn resume_dispatch_attempt<S: TaskStateStore>(
    store: S,
    attempt_id: Uuid,
    host: &(impl HostPorts + ?Sized),
) -> DispatchRecoveryResult {
    let mut state = match store.load() {
        Ok(state) => state,
        Err(error) => {
            return DispatchRecoveryResult::Error {
                message: error.to_string(),
            }
        }
    };
    let Some(attempt) = state.active_attempt(attempt_id).cloned() else {
        return DispatchRecoveryResult::Error {
            message: "dispatch attempt no longer exists".into(),
        };
    };
    if attempt.phase() == DispatchAttemptPhase::Dispatching {
        // A process may stop after the durable attempt was saved but before it records a
        // failure. Convert that interrupted state to the same explicit recovery boundary.
        if let Err(error) = persist_transition(
            &store,
            &mut state,
            attempt_id,
            DispatchAttemptTransition::Fail {
                message: "dispatch interrupted before completion".into(),
            },
        ) {
            return DispatchRecoveryResult::PersistenceUncertain {
                attempt_id,
                message: error,
            };
        }
        return resume_dispatch_attempt(store, attempt_id, host);
    }
    if attempt.phase() != DispatchAttemptPhase::Failed {
        return DispatchRecoveryResult::Error {
            message: "dispatch attempt is not ready to resume".into(),
        };
    }

    let mut restart_from = None;
    for step in attempt.steps() {
        if !step.completed {
            break;
        }
        let confirmed = match step.step {
            DispatchAttemptStep::CreateWorktree => {
                attempt
                    .owned_resources()
                    .iter()
                    .find_map(|receipt| match receipt {
                        OwnedResourceReceipt::Worktree(receipt) => {
                            Some(host.confirm_worktree_receipt(receipt))
                        }
                        _ => None,
                    })
            }
            DispatchAttemptStep::OpenPane => {
                attempt
                    .owned_resources()
                    .iter()
                    .find_map(|receipt| match receipt {
                        OwnedResourceReceipt::Pane(receipt) => {
                            Some(host.confirm_pane_receipt(receipt))
                        }
                        _ => None,
                    })
            }
            DispatchAttemptStep::StartAgent => {
                attempt
                    .owned_resources()
                    .iter()
                    .find_map(|receipt| match receipt {
                        OwnedResourceReceipt::Agent(receipt) => {
                            Some(host.confirm_agent_receipt(receipt))
                        }
                        _ => None,
                    })
            }
            DispatchAttemptStep::SendPrompt => None,
        };
        if let Some(confirmed) = confirmed {
            match confirmed {
                Ok(true) => continue,
                Ok(false) => restart_from = Some(step.step),
                Err(error) => {
                    return DispatchRecoveryResult::Error {
                        message: format!("could not confirm dispatch receipt: {error}"),
                    };
                }
            }
        } else if step.step != DispatchAttemptStep::SendPrompt {
            restart_from = Some(step.step);
        }
        if restart_from.is_some() {
            break;
        }
    }

    let transition = restart_from
        .map(DispatchAttemptTransition::RestartFrom)
        .unwrap_or(DispatchAttemptTransition::Resume);
    if let Err(error) = persist_transition(&store, &mut state, attempt_id, transition) {
        return DispatchRecoveryResult::Error { message: error };
    }
    execute_attempt(&store, state, attempt_id, host)
}

/// Explicitly clean a failed attempt. Only its exact owned receipts are considered.
/// Panes are removed before worktrees; an agent receipt is retired with its owned pane.
pub fn cleanup_dispatch_attempt<S: TaskStateStore>(
    store: S,
    attempt_id: Uuid,
    host: &(impl HostPorts + ?Sized),
) -> DispatchRecoveryResult {
    let mut state = match store.load() {
        Ok(state) => state,
        Err(error) => {
            return DispatchRecoveryResult::Error {
                message: error.to_string(),
            }
        }
    };
    let Some(attempt) = state.active_attempt(attempt_id).cloned() else {
        return DispatchRecoveryResult::Error {
            message: "dispatch attempt no longer exists".into(),
        };
    };
    if attempt.phase() == DispatchAttemptPhase::Dispatching {
        // See resume: an interrupted worker leaves a durable Dispatching phase that must
        // remain explicitly recoverable, never silently discarded.
        if let Err(error) = persist_transition(
            &store,
            &mut state,
            attempt_id,
            DispatchAttemptTransition::Fail {
                message: "dispatch interrupted before cleanup".into(),
            },
        ) {
            return DispatchRecoveryResult::PersistenceUncertain {
                attempt_id,
                message: error,
            };
        }
        return cleanup_dispatch_attempt(store, attempt_id, host);
    }
    if !matches!(
        attempt.phase(),
        DispatchAttemptPhase::Failed | DispatchAttemptPhase::Cleaning
    ) {
        return DispatchRecoveryResult::Error {
            message: "dispatch attempt is not ready to clean".into(),
        };
    }
    if attempt.phase() == DispatchAttemptPhase::Failed {
        if let Err(error) = persist_transition(
            &store,
            &mut state,
            attempt_id,
            DispatchAttemptTransition::BeginCleanup,
        ) {
            return DispatchRecoveryResult::Error { message: error };
        }
    }

    loop {
        let Some(attempt) = state.active_attempt(attempt_id).cloned() else {
            return DispatchRecoveryResult::Error {
                message: "dispatch attempt disappeared during cleanup".into(),
            };
        };
        let pane = attempt
            .owned_resources()
            .iter()
            .find_map(|receipt| match receipt {
                OwnedResourceReceipt::Pane(receipt) => Some(receipt.clone()),
                _ => None,
            });
        if let Some(pane) = pane {
            let confirmed = match host.confirm_pane_receipt(&pane) {
                Ok(confirmed) => confirmed,
                Err(error) => return cleanup_failed(&store, &mut state, attempt_id, error),
            };
            if confirmed {
                if let Err(error) = host.remove_pane_receipt(&pane) {
                    return cleanup_failed(&store, &mut state, attempt_id, error);
                }
            }
            let mut removed = vec![OwnedResourceReceipt::Pane(pane.clone())];
            removed.extend(
                attempt
                    .owned_resources()
                    .iter()
                    .filter_map(|receipt| match receipt {
                        OwnedResourceReceipt::Agent(agent) if agent.pane_id == pane.pane_id => {
                            Some(receipt.clone())
                        }
                        _ => None,
                    }),
            );
            if let Err(error) = persist_transition(
                &store,
                &mut state,
                attempt_id,
                DispatchAttemptTransition::RemoveOwnedReceipts(removed),
            ) {
                return DispatchRecoveryResult::PersistenceUncertain {
                    attempt_id,
                    message: error,
                };
            }
            continue;
        }

        let worktree = attempt
            .owned_resources()
            .iter()
            .find_map(|receipt| match receipt {
                OwnedResourceReceipt::Worktree(receipt) => Some(receipt.clone()),
                _ => None,
            });
        if let Some(worktree) = worktree {
            let confirmed = match host.confirm_worktree_receipt(&worktree) {
                Ok(confirmed) => confirmed,
                Err(error) => return cleanup_failed(&store, &mut state, attempt_id, error),
            };
            if confirmed {
                if let Err(error) = host.remove_worktree_receipt(&worktree) {
                    return cleanup_failed(&store, &mut state, attempt_id, error);
                }
            }
            if let Err(error) = persist_transition(
                &store,
                &mut state,
                attempt_id,
                DispatchAttemptTransition::RemoveOwnedReceipt(OwnedResourceReceipt::Worktree(
                    worktree,
                )),
            ) {
                return DispatchRecoveryResult::PersistenceUncertain {
                    attempt_id,
                    message: error,
                };
            }
            continue;
        }

        // An agent receipt without a pane cannot be safely removed by the available exact
        // host ports. Keep it visible and recoverable rather than inferring a target.
        if attempt
            .owned_resources()
            .iter()
            .any(|receipt| matches!(receipt, OwnedResourceReceipt::Agent(_)))
        {
            return cleanup_failed(
                &store,
                &mut state,
                attempt_id,
                "owned agent has no owned pane".into(),
            );
        }
        let result = store.locked_transition(|fresh| {
            let revision = fresh
                .active_attempt(attempt_id)
                .ok_or_else(|| "dispatch attempt no longer exists".to_string())?
                .revision();
            fresh
                .remove_dispatch_attempt(attempt_id, revision)
                .map_err(|error| error.to_string())
        });
        return match result {
            Ok(()) => DispatchRecoveryResult::Cleaned { attempt_id },
            Err(message) => DispatchRecoveryResult::PersistenceUncertain {
                attempt_id,
                message,
            },
        };
    }
}

fn execute_attempt<S: TaskStateStore>(
    store: &S,
    mut state: DomainState,
    attempt_id: Uuid,
    host: &(impl HostPorts + ?Sized),
) -> DispatchRecoveryResult {
    loop {
        let Some(attempt) = state.active_attempt(attempt_id).cloned() else {
            return DispatchRecoveryResult::Error {
                message: "dispatch attempt no longer exists".into(),
            };
        };
        let Some(task) = state.get(attempt.task_id()).cloned() else {
            return failed_attempt(
                store,
                &mut state,
                attempt_id,
                "task no longer exists".into(),
            );
        };
        let next = attempt
            .steps()
            .iter()
            .find(|step| !step.completed)
            .map(|step| step.step);
        if next == Some(DispatchAttemptStep::SendPrompt) {
            let (pane_id, agent) = match finalize_agent_dispatch(store, attempt_id) {
                Ok(result) => result,
                Err(result) => return result,
            };
            return match host.agent_prompt(&pane_id, &initial_prompt(&task)) {
                Ok(()) => DispatchRecoveryResult::Dispatched {
                    pane_id,
                    agent: Some(agent),
                    prompt_warning: None,
                },
                Err(error) => DispatchRecoveryResult::Dispatched {
                    pane_id,
                    agent: Some(agent),
                    prompt_warning: Some(format!("initial prompt failed: {error}")),
                },
            };
        }
        // A confirmed worktree receipt supplies the later pane cwd. Do not require the
        // original repository to remain usable when resuming its remaining steps.
        let needs_repo_cwd = matches!(next, Some(DispatchAttemptStep::CreateWorktree))
            || (matches!(next, Some(DispatchAttemptStep::OpenPane))
                && !attempt
                    .owned_resources()
                    .iter()
                    .any(|receipt| matches!(receipt, OwnedResourceReceipt::Worktree(_))));
        let repo_cwd = if needs_repo_cwd {
            match resolve_dispatch_cwd(&task, |path| {
                host.path_usable(path) && Path::new(path).is_dir()
            }) {
                Ok(cwd) => Some(cwd),
                Err(error) => return failed_attempt(store, &mut state, attempt_id, error),
            }
        } else {
            None
        };
        let Some(next) = next else {
            return match finalize_agent_dispatch(store, attempt_id) {
                Ok((pane_id, agent)) => DispatchRecoveryResult::Dispatched {
                    pane_id,
                    agent: Some(agent),
                    prompt_warning: None,
                },
                Err(result) => result,
            };
        };

        let outcome = match next {
            DispatchAttemptStep::CreateWorktree => host
                .create_worktree_receipt(
                    repo_cwd.as_deref().expect("create requires repository cwd"),
                    &worktree_branch_for_task(task.id),
                )
                .map(OwnedResourceReceipt::Worktree)
                .map(DispatchAttemptTransition::RecordReceipt),
            DispatchAttemptStep::OpenPane => {
                let cwd = attempt
                    .owned_resources()
                    .iter()
                    .find_map(|receipt| match receipt {
                        OwnedResourceReceipt::Worktree(receipt) => Some(receipt.path.as_str()),
                        _ => None,
                    })
                    .or(repo_cwd.as_deref())
                    .expect("open pane requires worktree receipt or repository cwd");
                host.open_shell_at_cwd_receipt(cwd)
                    .map(OwnedResourceReceipt::Pane)
                    .map(DispatchAttemptTransition::RecordReceipt)
            }
            DispatchAttemptStep::StartAgent => {
                let Some(pane) =
                    attempt
                        .owned_resources()
                        .iter()
                        .find_map(|receipt| match receipt {
                            OwnedResourceReceipt::Pane(receipt) => Some(receipt.pane_id.as_str()),
                            _ => None,
                        })
                else {
                    return failed_attempt(
                        store,
                        &mut state,
                        attempt_id,
                        "dispatch is missing its pane receipt".into(),
                    );
                };
                host.start_agent_receipt(&agent_name_for_task(task.id), attempt.kind(), pane)
                    .map(OwnedResourceReceipt::Agent)
                    .map(DispatchAttemptTransition::RecordReceipt)
            }
            DispatchAttemptStep::SendPrompt => unreachable!("prompt is handled after finalizing"),
        };
        match outcome {
            Ok(transition) => {
                if let Err(error) = persist_transition(store, &mut state, attempt_id, transition) {
                    return DispatchRecoveryResult::PersistenceUncertain {
                        attempt_id,
                        message: error,
                    };
                }
            }
            Err(error) => return failed_attempt(store, &mut state, attempt_id, error),
        }
    }
}

/// Atomically commit the agent link, existing successful-dispatch semantics, and
/// attempt removal. This happens before the optional prompt, so a prompt failure never
/// turns a live agent into a recovery record.
fn finalize_agent_dispatch<S: TaskStateStore>(
    store: &S,
    attempt_id: Uuid,
) -> Result<(String, String), DispatchRecoveryResult> {
    store
        .locked_transition(|state| {
            let attempt = state
                .active_attempt(attempt_id)
                .cloned()
                .ok_or_else(|| "dispatch attempt no longer exists".to_string())?;
            let agent = attempt
                .owned_resources()
                .iter()
                .find_map(|receipt| match receipt {
                    OwnedResourceReceipt::Agent(receipt) => Some(receipt.clone()),
                    _ => None,
                })
                .ok_or_else(|| "dispatch completed without an agent receipt".to_string())?;
            let kind = attempt.kind().to_string();
            let display_name = agent.display_name.clone().unwrap_or_else(|| kind.clone());
            state
                .after_dispatch_success(
                    attempt.task_id(),
                    AgentMeta {
                        agent_id: Some(display_name),
                        pane_id: Some(agent.pane_id.clone()),
                        agent_session: agent.agent_session,
                    },
                )
                .map_err(|error| error.to_string())?;
            state
                .remove_dispatch_attempt(attempt_id, attempt.revision())
                .map_err(|error| error.to_string())?;
            Ok((agent.pane_id, kind))
        })
        .map_err(|message| DispatchRecoveryResult::PersistenceUncertain {
            attempt_id,
            message,
        })
}

fn persist_transition<S: TaskStateStore>(
    store: &S,
    state: &mut DomainState,
    attempt_id: Uuid,
    transition: DispatchAttemptTransition,
) -> Result<(), String> {
    // Never save the worker's long-lived snapshot. Load, validate its current attempt
    // revision, transition, and replace under one lock so attention and Board mutations
    // committed during host I/O survive and concurrent recovery actions are rejected.
    let refreshed = store.locked_transition(|fresh| {
        let revision = fresh
            .active_attempt(attempt_id)
            .ok_or_else(|| "dispatch attempt no longer exists".to_string())?
            .revision();
        fresh
            .transition_dispatch_attempt(attempt_id, revision, transition)
            .map_err(|error| error.to_string())?;
        Ok(fresh.clone())
    })?;
    *state = refreshed;
    Ok(())
}

fn failed_attempt<S: TaskStateStore>(
    store: &S,
    state: &mut DomainState,
    attempt_id: Uuid,
    message: String,
) -> DispatchRecoveryResult {
    match persist_transition(
        store,
        state,
        attempt_id,
        DispatchAttemptTransition::Fail {
            message: message.clone(),
        },
    ) {
        Ok(()) => DispatchRecoveryResult::Failed {
            attempt_id,
            message,
        },
        Err(error) => DispatchRecoveryResult::PersistenceUncertain {
            attempt_id,
            message: error,
        },
    }
}

fn cleanup_failed<S: TaskStateStore>(
    store: &S,
    state: &mut DomainState,
    attempt_id: Uuid,
    message: String,
) -> DispatchRecoveryResult {
    match persist_transition(
        store,
        state,
        attempt_id,
        DispatchAttemptTransition::CleanupFailed {
            message: message.clone(),
        },
    ) {
        Ok(()) => DispatchRecoveryResult::Failed {
            attempt_id,
            message,
        },
        Err(error) => DispatchRecoveryResult::PersistenceUncertain {
            attempt_id,
            message: error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{HumanStatus, ProvenanceOrigin};
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    };

    fn task_with(
        scope: TaskScope,
        capsule: Option<ContextCapsule>,
        title: &str,
        notes: Option<&str>,
    ) -> (DomainState, Uuid) {
        let mut state = DomainState::new();
        let id = state
            .create(
                title,
                notes.map(str::to_string),
                scope,
                capsule,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create");
        (state, id)
    }

    #[test]
    fn resolve_cwd_prefers_project_scope_path() {
        let (state, id) = task_with(
            TaskScope::Project {
                path: "/proj/app".into(),
            },
            Some(ContextCapsule {
                worktree_path: Some("/wt".into()),
                cwd: Some("/cwd".into()),
                ..ContextCapsule::default()
            }),
            "T",
            None,
        );
        let task = state.get(id).expect("t");
        let cwd = resolve_dispatch_cwd(task, |p| p == "/proj/app" || p == "/wt" || p == "/cwd")
            .expect("cwd");
        assert_eq!(cwd, "/proj/app");
    }

    #[test]
    fn resolve_cwd_capsule_order_worktree_repo_cwd() {
        let (state, id) = task_with(
            TaskScope::Global,
            Some(ContextCapsule {
                worktree_path: Some("/missing-wt".into()),
                repo_path: Some("/repo".into()),
                cwd: Some("/cwd".into()),
                ..ContextCapsule::default()
            }),
            "T",
            None,
        );
        let task = state.get(id).expect("t");
        let cwd = resolve_dispatch_cwd(task, |p| p == "/repo" || p == "/cwd").expect("cwd");
        assert_eq!(cwd, "/repo");

        let (state, id) = task_with(
            TaskScope::Global,
            Some(ContextCapsule {
                cwd: Some("/only-cwd".into()),
                ..ContextCapsule::default()
            }),
            "T",
            None,
        );
        let task = state.get(id).expect("t");
        let cwd = resolve_dispatch_cwd(task, |p| p == "/only-cwd").expect("cwd");
        assert_eq!(cwd, "/only-cwd");
    }

    #[test]
    fn resolve_cwd_fails_when_none_usable() {
        let (state, id) = task_with(TaskScope::Global, None, "T", None);
        let task = state.get(id).expect("t");
        let err = resolve_dispatch_cwd(task, |_| true).expect_err("no paths");
        assert!(!err.is_empty());

        let (state, id) = task_with(
            TaskScope::Project {
                path: "/gone".into(),
            },
            Some(ContextCapsule {
                cwd: Some("/also-gone".into()),
                ..ContextCapsule::default()
            }),
            "T",
            None,
        );
        let task = state.get(id).expect("t");
        let err = resolve_dispatch_cwd(task, |_| false).expect_err("unusable");
        assert!(err.contains("usable") || err.contains("cwd"));
    }

    #[test]
    fn plan_here_has_open_start_prompt_never_worktree_create() {
        let steps = plan_dispatch(
            DispatchMode::Here,
            "/work",
            "grok",
            "t12345678",
            None,
            Some("Fix the bug".into()),
        );
        assert_eq!(
            steps,
            vec![
                DispatchStep::OpenShell {
                    cwd: "/work".into()
                },
                DispatchStep::StartAgent {
                    kind: "grok".into(),
                    name: "t12345678".into()
                },
                DispatchStep::Prompt {
                    text: "Fix the bug".into()
                },
            ]
        );
        assert!(!plan_contains_worktree_create(&steps));
        for s in &steps {
            assert!(
                !matches!(s, DispatchStep::CreateWorktree { .. }),
                "Here plan must not create worktree: {s:?}"
            );
        }
    }

    #[test]
    fn plan_new_worktree_includes_create_then_open_start() {
        let steps = plan_dispatch(
            DispatchMode::NewWorktree,
            "/repos/app",
            "codex",
            "tdeadbeef",
            Some("tasks/deadbeef"),
            Some("Do it".into()),
        );
        assert!(plan_contains_worktree_create(&steps));
        assert_eq!(
            steps,
            vec![
                DispatchStep::CreateWorktree {
                    repo_cwd: "/repos/app".into(),
                    branch: "tasks/deadbeef".into(),
                },
                DispatchStep::OpenShell {
                    cwd: "/repos/app".into()
                },
                DispatchStep::StartAgent {
                    kind: "codex".into(),
                    name: "tdeadbeef".into()
                },
                DispatchStep::Prompt {
                    text: "Do it".into()
                },
            ]
        );
    }

    #[test]
    fn plan_omits_empty_prompt() {
        let steps = plan_dispatch(
            DispatchMode::Here,
            "/w",
            "grok",
            "n1",
            None,
            Some("   ".into()),
        );
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0], DispatchStep::OpenShell { .. }));
        assert!(matches!(steps[1], DispatchStep::StartAgent { .. }));
    }

    #[test]
    fn worktree_branch_uses_tasks_prefix_and_8hex() {
        let id = Uuid::from_u128(0xdead_beef_cafe_babe_u128);
        let branch = worktree_branch_for_task(id);
        assert!(branch.starts_with("tasks/"), "branch={branch}");
        assert_eq!(branch.len(), "tasks/".len() + 8);
    }

    #[test]
    fn initial_prompt_includes_title_and_notes() {
        let (state, id) = task_with(TaskScope::Global, None, "Title only", None);
        assert_eq!(initial_prompt(state.get(id).expect("t")), "Title only");
        let (state, id) = task_with(TaskScope::Global, None, "With notes", Some("extra context"));
        let p = initial_prompt(state.get(id).expect("t"));
        assert!(p.contains("With notes"));
        assert!(p.contains("extra context"));
    }

    #[test]
    fn dispatch_agent_kind_default_and_override() {
        assert_eq!(dispatch_agent_kind(None), "grok");
        assert_eq!(dispatch_agent_kind(Some(String::new())), "grok");
        assert_eq!(dispatch_agent_kind(Some("  ".into())), "grok");
        assert_eq!(dispatch_agent_kind(Some("codex".into())), "codex");
        assert_eq!(dispatch_agent_kind(Some("  claude  ".into())), "claude");
    }

    #[test]
    fn cycle_dispatch_kind_wraps_supported_list() {
        assert_eq!(cycle_dispatch_kind("grok"), "claude");
        assert_eq!(cycle_dispatch_kind("claude"), "pi");
        assert_eq!(cycle_dispatch_kind("opencode"), "grok");
        // Unknown falls through as if at grok, then advances to claude.
        assert_eq!(cycle_dispatch_kind("unknown-agent"), "claude");
    }

    #[test]
    fn dispatch_agent_kind_from_env_reads_override() {
        // Isolation: set and restore so parallel tests do not see a sticky value.
        let key = DISPATCH_AGENT_ENV;
        let prev = env::var(key).ok();
        env::set_var(key, "codex");
        assert_eq!(dispatch_agent_kind_from_env(), "codex");
        env::set_var(key, "  ");
        assert_eq!(dispatch_agent_kind_from_env(), DEFAULT_DISPATCH_AGENT);
        env::remove_var(key);
        assert_eq!(dispatch_agent_kind_from_env(), "grok");
        match prev {
            Some(v) => env::set_var(key, v),
            None => env::remove_var(key),
        }
    }

    /// Recording host for execute tests.
    #[derive(Debug, Default)]
    struct FakeDispatchHost {
        usable: Vec<String>,
        calls: Arc<Mutex<Vec<Vec<String>>>>,
        fail_open: bool,
        fail_start: bool,
        fail_prompt: bool,
        fail_create: bool,
        next_pane: String,
        /// Checkout path returned by create_worktree (default: `/tmp/fake-wt`).
        checkout_path: String,
        start_agent_name: Option<String>,
    }

    impl HostPorts for FakeDispatchHost {
        fn list_pane_ids(&self) -> Result<Vec<String>, String> {
            Ok(vec![])
        }
        fn focus_pane(&self, _pane_id: &str) -> Result<(), String> {
            Ok(())
        }
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }
        fn path_usable(&self, path: &str) -> bool {
            self.usable.iter().any(|p| p == path)
        }
        fn open_shell_at_cwd(&self, cwd: &str) -> Result<String, String> {
            self.calls.lock().unwrap().push(vec![
                "pane".into(),
                "split".into(),
                "--cwd".into(),
                cwd.into(),
                "--direction".into(),
                "right".into(),
                "--no-focus".into(),
            ]);
            if self.fail_open {
                return Err("open shell failed".into());
            }
            let pane = if self.next_pane.is_empty() {
                "w0:p_new".into()
            } else {
                self.next_pane.clone()
            };
            Ok(pane)
        }
        fn start_agent(
            &self,
            name: &str,
            kind: &str,
            pane_id: &str,
        ) -> Result<crate::host::StartedAgent, String> {
            self.calls.lock().unwrap().push(vec![
                "agent".into(),
                "start".into(),
                name.into(),
                "--kind".into(),
                kind.into(),
                "--pane".into(),
                pane_id.into(),
            ]);
            if self.fail_start {
                return Err("agent start failed".into());
            }
            Ok(crate::host::StartedAgent {
                pane_id: pane_id.to_string(),
                agent: self
                    .start_agent_name
                    .clone()
                    .or_else(|| Some(kind.to_string())),
                agent_session: None,
            })
        }
        fn agent_prompt(&self, target: &str, text: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(vec![
                "agent".into(),
                "prompt".into(),
                target.into(),
                text.into(),
            ]);
            if self.fail_prompt {
                return Err("prompt failed".into());
            }
            Ok(())
        }
        fn create_worktree(&self, repo_cwd: &str, branch: &str) -> Result<String, String> {
            self.calls.lock().unwrap().push(vec![
                "worktree".into(),
                "create".into(),
                "--cwd".into(),
                repo_cwd.into(),
                "--branch".into(),
                branch.into(),
                "--no-focus".into(),
            ]);
            if self.fail_create {
                return Err("worktree create failed".into());
            }
            let path = if self.checkout_path.is_empty() {
                "/tmp/fake-wt".into()
            } else {
                self.checkout_path.clone()
            };
            Ok(path)
        }
    }

    #[test]
    fn execute_success_links_sets_doing_and_prompts() {
        let dir = tempfile_dir();
        let (mut state, id) = task_with(
            TaskScope::Project {
                path: dir.to_string_lossy().into(),
            },
            None,
            "Ship dispatch",
            Some("notes here"),
        );
        let host = FakeDispatchHost {
            usable: vec![dir.to_string_lossy().into()],
            next_pane: "w0:p42".into(),
            start_agent_name: Some("grok".into()),
            ..FakeDispatchHost::default()
        };
        // path_usable true but is_dir check needs real dir
        let result = execute_dispatch(&mut state, id, "grok", &host);
        assert!(result.is_success(), "expected success, got {:?}", result);
        let task = state.get(id).expect("t");
        assert!(crate::domain::is_linked(task));
        assert_eq!(task.status, HumanStatus::Started);
        assert_eq!(
            task.agent_meta.as_ref().and_then(|m| m.pane_id.as_deref()),
            Some("w0:p42")
        );
        let calls = host.calls.lock().unwrap().clone();
        assert!(
            calls
                .iter()
                .any(|c| c.windows(2).any(|w| w == ["pane", "split"])),
            "open shell: {calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|c| c.windows(2).any(|w| w == ["agent", "start"])),
            "start: {calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|c| c.windows(2).any(|w| w == ["agent", "prompt"])),
            "prompt: {calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|c| c.windows(2).any(|w| w == ["worktree", "create"])),
            "must not create worktree: {calls:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_start_failure_does_not_link() {
        let dir = tempfile_dir();
        let (mut state, id) = task_with(
            TaskScope::Project {
                path: dir.to_string_lossy().into(),
            },
            None,
            "Will fail",
            None,
        );
        let host = FakeDispatchHost {
            usable: vec![dir.to_string_lossy().into()],
            fail_start: true,
            ..FakeDispatchHost::default()
        };
        let result = execute_dispatch(&mut state, id, "grok", &host);
        assert!(!result.is_success());
        let task = state.get(id).expect("t");
        assert!(task.agent_meta.is_none());
        assert_eq!(task.status, HumanStatus::Ready);
        assert!(!task
            .history
            .iter()
            .any(|e| e.kind == crate::domain::TaskEventKind::Dispatched));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_unusable_cwd_no_host_start() {
        let (mut state, id) = task_with(TaskScope::Global, None, "No cwd", None);
        let host = FakeDispatchHost::default();
        let result = execute_dispatch(&mut state, id, "grok", &host);
        assert!(!result.is_success());
        assert!(host.calls.lock().unwrap().is_empty());
        assert!(state.get(id).expect("t").agent_meta.is_none());
    }

    #[test]
    fn execute_new_worktree_success_creates_then_links() {
        let dir = tempfile_dir();
        let checkout = dir.join("wt-checkout");
        std::fs::create_dir_all(&checkout).expect("mkdir checkout");
        let (mut state, id) = task_with(
            TaskScope::Project {
                path: dir.to_string_lossy().into(),
            },
            None,
            "Worktree dispatch",
            Some("notes"),
        );
        let host = FakeDispatchHost {
            usable: vec![dir.to_string_lossy().into()],
            next_pane: "w0:p_wt".into(),
            checkout_path: checkout.to_string_lossy().into(),
            start_agent_name: Some("grok".into()),
            ..FakeDispatchHost::default()
        };
        let result =
            execute_dispatch_mode(&mut state, id, DispatchMode::NewWorktree, "grok", &host);
        assert!(result.is_success(), "expected success, got {:?}", result);
        let task = state.get(id).expect("t");
        assert!(crate::domain::is_linked(task));
        assert_eq!(task.status, HumanStatus::Started);
        assert_eq!(
            task.agent_meta.as_ref().and_then(|m| m.pane_id.as_deref()),
            Some("w0:p_wt")
        );

        let calls = host.calls.lock().unwrap().clone();
        assert!(
            calls
                .iter()
                .any(|c| c.windows(2).any(|w| w == ["worktree", "create"])),
            "must create worktree: {calls:?}"
        );
        let expected_branch = worktree_branch_for_task(id);
        assert!(
            calls.iter().any(|c| {
                c.windows(2)
                    .any(|w| w == ["--branch", expected_branch.as_str()])
            }),
            "branch {expected_branch} missing: {calls:?}"
        );
        // Shell opens at checkout path, not the original repo cwd alone.
        let checkout_s = checkout.to_string_lossy().to_string();
        assert!(
            calls.iter().any(|c| {
                c.windows(2).any(|w| w == ["--cwd", checkout_s.as_str()])
                    && c.windows(2).any(|w| w == ["pane", "split"])
            }),
            "open shell at checkout: {calls:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_new_worktree_create_fail_does_not_link() {
        let dir = tempfile_dir();
        let (mut state, id) = task_with(
            TaskScope::Project {
                path: dir.to_string_lossy().into(),
            },
            None,
            "Create fails",
            None,
        );
        let host = FakeDispatchHost {
            usable: vec![dir.to_string_lossy().into()],
            fail_create: true,
            ..FakeDispatchHost::default()
        };
        let result =
            execute_dispatch_mode(&mut state, id, DispatchMode::NewWorktree, "grok", &host);
        assert!(!result.is_success());
        assert!(
            result.message().contains("worktree create") || result.message().contains("failed"),
            "msg={}",
            result.message()
        );
        let task = state.get(id).expect("t");
        assert!(task.agent_meta.is_none());
        assert_eq!(task.status, HumanStatus::Ready);
        // Must not open shell or start agent after create failure.
        let calls = host.calls.lock().unwrap().clone();
        assert!(
            !calls
                .iter()
                .any(|c| c.windows(2).any(|w| w == ["pane", "split"])
                    || c.windows(2).any(|w| w == ["agent", "start"])),
            "no split/start after create fail: {calls:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_here_never_calls_create_worktree() {
        let dir = tempfile_dir();
        let (mut state, id) = task_with(
            TaskScope::Project {
                path: dir.to_string_lossy().into(),
            },
            None,
            "Here only",
            None,
        );
        let host = FakeDispatchHost {
            usable: vec![dir.to_string_lossy().into()],
            next_pane: "w0:p1".into(),
            ..FakeDispatchHost::default()
        };
        let result = execute_dispatch_mode(&mut state, id, DispatchMode::Here, "grok", &host);
        assert!(result.is_success());
        let calls = host.calls.lock().unwrap().clone();
        assert!(
            !calls
                .iter()
                .any(|c| c.windows(2).any(|w| w == ["worktree", "create"])),
            "Here must not create: {calls:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let sequence = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tsk-dispatch-{nanos}-{sequence}"));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }
}
