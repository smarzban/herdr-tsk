//! Host Adapter ports for resume, park, attention, and dispatch.
//!
//! Uses `HERDR_BIN_PATH` (fallback `herdr`). Resume/open path never issues worktree
//! create. Dispatch-here uses pane split + agent start + agent prompt. Dispatch-worktree
//! adds `worktree create` first, then uses the returned checkout path as agent cwd.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::attention::ObservationMap;
use crate::board_pane::BOARD_PANE_LABEL;
use crate::domain::{
    AgentReceipt, AgentSessionIdentity, ObservedStatus, PaneReceipt, WorktreeReceipt,
};
use crate::text::non_empty;

/// Env var for the herdr CLI binary (injected by host; tests set a fake).
pub const HERDR_BIN_ENV: &str = "HERDR_BIN_PATH";

/// One pane row from `herdr pane list` / `pane current`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaneInfo {
    pub pane_id: String,
    pub cwd: Option<String>,
    pub foreground_cwd: Option<String>,
    /// Agent kind/display name, distinct from stable session identity.
    pub agent: Option<String>,
    pub agent_session: Option<AgentSessionIdentity>,
    /// Host-reported agent lifecycle when present (`working`, `blocked`, `idle`, `done`, …).
    pub observed: Option<ObservedStatus>,
    pub label: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub focused: bool,
    pub workspace_id: Option<String>,
}

impl PaneInfo {
    /// Prefer foreground process cwd, then pane cwd (non-empty).
    pub fn effective_cwd(&self) -> Option<&str> {
        non_empty(self.foreground_cwd.as_deref()).or_else(|| non_empty(self.cwd.as_deref()))
    }
}

/// Result of `herdr agent start` (subset used by dispatch).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartedAgent {
    pub pane_id: String,
    pub agent: Option<String>,
    pub agent_session: Option<AgentSessionIdentity>,
}

/// Host operations needed by resume, park, attention, and dispatch.
pub trait HostPorts {
    /// Pane ids currently present in the session (`pane list`).
    fn list_pane_ids(&self) -> Result<Vec<String>, String>;

    /// Full pane rows from `pane list` (for work-pane selection on park).
    fn list_panes(&self) -> Result<Vec<PaneInfo>, String> {
        Ok(self
            .list_pane_ids()?
            .into_iter()
            .map(|pane_id| PaneInfo {
                pane_id,
                ..PaneInfo::default()
            })
            .collect())
    }

    /// Current pane from `pane current`, if any.
    fn current_pane(&self) -> Result<Option<PaneInfo>, String> {
        Ok(None)
    }

    /// Focus an existing pane by id (`pane zoom <id> --on` then `--off`).
    fn focus_pane(&self, pane_id: &str) -> Result<(), String>;

    /// Open or focus a worktree/workspace path (`worktree open --path … --focus`).
    fn open_path(&self, path: &str) -> Result<(), String>;

    /// Whether a path is usable as a resume open target.
    fn path_usable(&self, path: &str) -> bool {
        let p = Path::new(path);
        p.is_dir() || p.is_file()
    }

    /// Open a shell pane rooted at `cwd` (`pane split --cwd …`). Returns new pane id.
    ///
    /// Default: unsupported (resume-only hosts). Dispatch requires a real implementation.
    fn open_shell_at_cwd(&self, _cwd: &str) -> Result<String, String> {
        Err("open_shell_at_cwd not supported on this host".into())
    }

    /// Start a supported agent in an existing pane (`agent start --kind --pane`).
    fn start_agent(
        &self,
        _name: &str,
        _kind: &str,
        _pane_id: &str,
    ) -> Result<StartedAgent, String> {
        Err("start_agent not supported on this host".into())
    }

    /// Submit a prompt to an agent/pane (`agent prompt`).
    fn agent_prompt(&self, _target: &str, _text: &str) -> Result<(), String> {
        Err("agent_prompt not supported on this host".into())
    }

    /// Create a git worktree via host (`worktree create --cwd --branch`).
    ///
    /// Returns the checkout path for the new worktree. Used only by dispatch-worktree
    /// mode (never by dispatch-here or resume). Default: unsupported.
    fn create_worktree(&self, _repo_cwd: &str, _branch: &str) -> Result<String, String> {
        Err("create_worktree not supported on this host".into())
    }

    /// Create a worktree and return its exact host removal identity.
    fn create_worktree_receipt(
        &self,
        _repo_cwd: &str,
        _branch: &str,
    ) -> Result<WorktreeReceipt, String> {
        Err("create_worktree_receipt not supported on this host".into())
    }

    /// Open a shell pane and return its exact host identity.
    fn open_shell_at_cwd_receipt(&self, _cwd: &str) -> Result<PaneReceipt, String> {
        Err("open_shell_at_cwd_receipt not supported on this host".into())
    }

    /// Start an agent in a receipt-owned pane and return its exact identity.
    fn start_agent_receipt(
        &self,
        _name: &str,
        _kind: &str,
        _pane_id: &str,
    ) -> Result<AgentReceipt, String> {
        Err("start_agent_receipt not supported on this host".into())
    }

    /// Confirm a worktree by its recorded opaque workspace id only.
    fn confirm_worktree_receipt(&self, _receipt: &WorktreeReceipt) -> Result<bool, String> {
        Err("confirm_worktree_receipt not supported on this host".into())
    }

    /// Confirm a pane by its recorded opaque pane id only.
    fn confirm_pane_receipt(&self, _receipt: &PaneReceipt) -> Result<bool, String> {
        Err("confirm_pane_receipt not supported on this host".into())
    }

    /// Confirm an agent by its recorded pane and optional stable session identity only.
    fn confirm_agent_receipt(&self, _receipt: &AgentReceipt) -> Result<bool, String> {
        Err("confirm_agent_receipt not supported on this host".into())
    }

    /// Remove only the recorded pane identity.
    fn remove_pane_receipt(&self, _receipt: &PaneReceipt) -> Result<(), String> {
        Err("remove_pane_receipt not supported on this host".into())
    }

    /// Remove only the recorded workspace identity.
    fn remove_worktree_receipt(&self, _receipt: &WorktreeReceipt) -> Result<(), String> {
        Err("remove_worktree_receipt not supported on this host".into())
    }
}

/// Real host: spawns herdr CLI.
#[derive(Debug, Clone)]
pub struct HerdrHost {
    bin: PathBuf,
}

impl HerdrHost {
    /// Resolve binary from `HERDR_BIN_PATH`, else `herdr` on PATH.
    pub fn from_env() -> Self {
        let bin = env::var(HERDR_BIN_ENV).unwrap_or_else(|_| "herdr".into());
        Self {
            bin: PathBuf::from(bin),
        }
    }

    /// Explicit binary path (tests / injection).
    pub fn with_bin(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    pub fn bin(&self) -> &Path {
        &self.bin
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.bin)
            .args(args)
            .output()
            .map_err(|e| format!("failed to spawn {}: {e}", self.bin.display()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "herdr {:?} failed (status {}): {}{}",
                args,
                output.status,
                stderr.trim(),
                if stdout.trim().is_empty() {
                    String::new()
                } else {
                    format!(" / {}", stdout.trim())
                }
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl HostPorts for HerdrHost {
    fn list_pane_ids(&self) -> Result<Vec<String>, String> {
        Ok(self.list_panes()?.into_iter().map(|p| p.pane_id).collect())
    }

    fn list_panes(&self) -> Result<Vec<PaneInfo>, String> {
        let stdout = self.run(&["pane", "list"])?;
        parse_pane_list_result(&stdout)
    }

    fn current_pane(&self) -> Result<Option<PaneInfo>, String> {
        let stdout = self.run(&["pane", "current"])?;
        parse_pane_current_result(&stdout)
    }

    fn focus_pane(&self, pane_id: &str) -> Result<(), String> {
        validate_host_id(pane_id, "pane")?;
        // Agent/shell panes are not plugin panes: `plugin pane focus` only works for
        // plugin-owned panes. Focus any pane by id via zoom on/off (herdr ≥ 0.7.x):
        // --on focuses (and may maximize); --off restores layout while keeping focus.
        self.run(&["pane", "zoom", pane_id, "--on"])?;
        let _ = self.run(&["pane", "zoom", pane_id, "--off"]);
        Ok(())
    }

    fn open_path(&self, path: &str) -> Result<(), String> {
        // Open/focus path only: never `worktree create`.
        self.run(&["worktree", "open", "--path", path, "--focus"])?;
        Ok(())
    }

    fn open_shell_at_cwd(&self, cwd: &str) -> Result<String, String> {
        // Sibling shell at cwd; never worktree create.
        let stdout = self.run(&[
            "pane",
            "split",
            "--current",
            "--direction",
            "right",
            "--cwd",
            cwd,
            "--no-focus",
        ])?;
        parse_split_pane_id(&stdout).ok_or_else(|| {
            format!(
                "pane split succeeded but no pane_id in response: {}",
                stdout.trim()
            )
        })
    }

    fn start_agent(&self, name: &str, kind: &str, pane_id: &str) -> Result<StartedAgent, String> {
        validate_host_id(pane_id, "pane")?;
        // Interactive agents can take >30s to become ready in live herdr.
        let stdout = self.run(&[
            "agent",
            "start",
            name,
            "--kind",
            kind,
            "--pane",
            pane_id,
            "--timeout",
            "90000",
        ])?;
        parse_started_agent(&stdout, pane_id, kind)
    }

    fn agent_prompt(&self, target: &str, text: &str) -> Result<(), String> {
        validate_host_id(target, "pane")?;
        // herdr 0.7.5 accepts `agent prompt <TARGET> <TEXT> [OPTIONS]`; pass both
        // positional values after Clap's option terminator so task text cannot be a flag.
        self.run(&["agent", "prompt", "--", target, text])?;
        Ok(())
    }

    fn create_worktree(&self, repo_cwd: &str, branch: &str) -> Result<String, String> {
        // Host chooses checkout path; we parse it from the JSON response.
        // --no-focus: keep the Tasks board focused while dispatch continues.
        let stdout = self.run(&[
            "worktree",
            "create",
            "--cwd",
            repo_cwd,
            "--branch",
            branch,
            "--no-focus",
        ])?;
        parse_worktree_checkout_path(&stdout).ok_or_else(|| {
            format!(
                "worktree create succeeded but no checkout path in response: {}",
                stdout.trim()
            )
        })
    }

    fn create_worktree_receipt(
        &self,
        repo_cwd: &str,
        branch: &str,
    ) -> Result<WorktreeReceipt, String> {
        let stdout = self.run(&[
            "worktree",
            "create",
            "--cwd",
            repo_cwd,
            "--branch",
            branch,
            "--no-focus",
        ])?;
        parse_worktree_receipt(&stdout).ok_or_else(|| {
            format!(
                "worktree create succeeded but no exact receipt in response: {}",
                stdout.trim()
            )
        })
    }

    fn open_shell_at_cwd_receipt(&self, cwd: &str) -> Result<PaneReceipt, String> {
        let stdout = self.run(&[
            "pane",
            "split",
            "--current",
            "--direction",
            "right",
            "--cwd",
            cwd,
            "--no-focus",
        ])?;
        parse_pane_receipt(&stdout).ok_or_else(|| {
            format!(
                "pane split succeeded but no exact receipt in response: {}",
                stdout.trim()
            )
        })
    }

    fn start_agent_receipt(
        &self,
        name: &str,
        kind: &str,
        pane_id: &str,
    ) -> Result<AgentReceipt, String> {
        validate_host_id(pane_id, "pane")?;
        let stdout = self.run(&[
            "agent",
            "start",
            name,
            "--kind",
            kind,
            "--pane",
            pane_id,
            "--timeout",
            "90000",
        ])?;
        parse_agent_receipt(&stdout, pane_id).ok_or_else(|| {
            format!(
                "agent start succeeded but no exact receipt in response: {}",
                stdout.trim()
            )
        })
    }

    fn confirm_worktree_receipt(&self, receipt: &WorktreeReceipt) -> Result<bool, String> {
        validate_host_id(&receipt.workspace_id, "workspace")?;
        let stdout = self.run(&["worktree", "list", "--workspace", &receipt.workspace_id])?;
        Ok(parse_worktree_workspace_ids(&stdout)
            .iter()
            .any(|workspace_id| workspace_id == &receipt.workspace_id))
    }

    fn confirm_pane_receipt(&self, receipt: &PaneReceipt) -> Result<bool, String> {
        validate_host_id(&receipt.pane_id, "pane")?;
        let stdout = self.run(&["pane", "get", &receipt.pane_id])?;
        Ok(parse_pane_current_result(&stdout)?.is_some_and(|pane| pane.pane_id == receipt.pane_id))
    }

    fn confirm_agent_receipt(&self, receipt: &AgentReceipt) -> Result<bool, String> {
        validate_host_id(&receipt.pane_id, "pane")?;
        let stdout = self.run(&["pane", "get", &receipt.pane_id])?;
        let Some(pane) = parse_pane_current_result(&stdout)? else {
            return Ok(false);
        };
        Ok(pane.pane_id == receipt.pane_id && pane.agent_session == receipt.agent_session)
    }

    fn remove_pane_receipt(&self, receipt: &PaneReceipt) -> Result<(), String> {
        validate_host_id(&receipt.pane_id, "pane")?;
        self.run(&["pane", "close", &receipt.pane_id])?;
        Ok(())
    }

    fn remove_worktree_receipt(&self, receipt: &WorktreeReceipt) -> Result<(), String> {
        validate_host_id(&receipt.workspace_id, "workspace")?;
        self.run(&[
            "worktree",
            "remove",
            "--workspace",
            &receipt.workspace_id,
            "--force",
            "--json",
        ])?;
        Ok(())
    }
}

/// Parse `result.pane.pane_id` from `pane split` JSON (skill: read result.pane.pane_id).
pub fn parse_split_pane_id(json: &str) -> Option<String> {
    let data = serde_json::from_str::<Value>(json).ok()?;
    let result = data.get("result")?;
    result
        .get("pane")
        .and_then(|p| p.get("pane_id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            // Some hosts may return pane_id at result top level.
            result
                .get("pane_id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

/// Parse `agent start` JSON (`type: agent_started` → `result.agent`).
///
/// Non-JSON success body uses `fallback_pane` / `fallback_kind` (CLI may print empty).
pub fn parse_started_agent(
    json: &str,
    fallback_pane: &str,
    fallback_kind: &str,
) -> Result<StartedAgent, String> {
    let Ok(data) = serde_json::from_str::<Value>(json) else {
        return Ok(StartedAgent {
            pane_id: fallback_pane.to_string(),
            agent: Some(fallback_kind.to_string()),
            agent_session: None,
        });
    };
    if let Some(err) = data.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("agent start failed");
        return Err(msg.to_string());
    }
    let agent = data.get("result").and_then(|r| r.get("agent"));
    let pane_id = agent
        .and_then(|a| a.get("pane_id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback_pane)
        .to_string();
    validate_host_id(&pane_id, "pane")?;
    let agent_name = agent
        .and_then(|a| a.get("agent").or_else(|| a.get("name")))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| Some(fallback_kind.to_string()));
    let agent_session = agent
        .and_then(|a| a.get("agent_session"))
        .and_then(parse_agent_session);
    Ok(StartedAgent {
        pane_id,
        agent: agent_name,
        agent_session,
    })
}

/// Argv fragments that must never appear on the dispatch-**here** host path.
///
/// Dispatch-worktree intentionally calls `worktree create`; use mode-aware checks.
pub const FORBIDDEN_DISPATCH_HERE_ARGS: &[&str] = &["worktree", "create"];

/// Deprecated alias: prefer [`FORBIDDEN_DISPATCH_HERE_ARGS`].
pub const FORBIDDEN_DISPATCH_ARGS: &[&str] = FORBIDDEN_DISPATCH_HERE_ARGS;

/// Parse the exact worktree receipt from `herdr worktree create` JSON.
///
/// The checkout path is display-only. Cleanup requires the host-issued workspace id.
pub fn parse_worktree_receipt(json: &str) -> Option<WorktreeReceipt> {
    let data = serde_json::from_str::<Value>(json).ok()?;
    if data.get("error").is_some() {
        return None;
    }
    let result = data.get("result")?;
    Some(WorktreeReceipt {
        path: exact_non_empty_string(result.get("worktree")?, "path")?,
        workspace_id: exact_non_empty_string(result.get("workspace")?, "workspace_id")?,
    })
}

/// Parse the exact pane receipt from `herdr pane split` JSON.
pub fn parse_pane_receipt(json: &str) -> Option<PaneReceipt> {
    Some(PaneReceipt {
        pane_id: parse_split_pane_id(json)?,
    })
}

/// Parse an exact agent receipt, rejecting a host response for a different pane.
pub fn parse_agent_receipt(json: &str, expected_pane_id: &str) -> Option<AgentReceipt> {
    let data = serde_json::from_str::<Value>(json).ok()?;
    if data.get("error").is_some() {
        return None;
    }
    let agent = data.get("result")?.get("agent")?;
    let pane_id = exact_non_empty_string(agent, "pane_id")?;
    if pane_id != expected_pane_id {
        return None;
    }
    Some(AgentReceipt {
        pane_id,
        display_name: agent
            .get("agent")
            .or_else(|| agent.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        agent_session: agent.get("agent_session").and_then(parse_agent_session),
    })
}

/// Extract only host-issued workspace identities from a `worktree list` response.
fn parse_worktree_workspace_ids(json: &str) -> Vec<String> {
    let Ok(data) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let Some(result) = data.get("result") else {
        return Vec::new();
    };
    let mut ids = result
        .get("worktrees")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|worktree| exact_non_empty_string(worktree, "open_workspace_id"))
        .collect::<Vec<_>>();
    if let Some(workspace_id) = result
        .get("source")
        .and_then(|source| exact_non_empty_string(source, "source_workspace_id"))
    {
        ids.push(workspace_id);
    }
    ids
}

/// Parse checkout path from `herdr worktree create` JSON (`result.worktree.path`).
///
/// Fallbacks: `result.workspace.worktree.checkout_path`, then `result.root_pane.cwd`.
pub fn parse_worktree_checkout_path(json: &str) -> Option<String> {
    let data = serde_json::from_str::<Value>(json).ok()?;
    if data.get("error").is_some() {
        return None;
    }
    let result = data.get("result")?;
    result
        .get("worktree")
        .and_then(|w| w.get("path"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            result
                .get("workspace")
                .and_then(|ws| ws.get("worktree"))
                .and_then(|w| w.get("checkout_path"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            result
                .get("root_pane")
                .and_then(|p| p.get("cwd"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

/// Collect non-empty `pane_id` strings from herdr `pane list` JSON.
pub fn parse_pane_ids(json: &str) -> Vec<String> {
    parse_pane_list(json)
        .into_iter()
        .map(|p| p.pane_id)
        .collect()
}

/// Parse full pane rows from herdr `pane list` JSON. This compatibility helper maps an
/// invalid response to empty for older pure callers; live host operations use the strict result.
pub fn parse_pane_list(json: &str) -> Vec<PaneInfo> {
    parse_pane_list_result(json).unwrap_or_default()
}

/// Strict pane-list parser. Malformed host output remains a host failure, never an empty
/// snapshot that could incorrectly become Park NoMatch or attention Stale.
pub fn parse_pane_list_result(json: &str) -> Result<Vec<PaneInfo>, String> {
    let data = serde_json::from_str::<Value>(json)
        .map_err(|error| format!("malformed pane list response: {error}"))?;
    let panes = data
        .get("result")
        .and_then(|result| result.get("panes"))
        .and_then(Value::as_array)
        .ok_or_else(|| "malformed pane list response: missing result.panes array".to_string())?;
    panes
        .iter()
        .map(|pane| {
            pane_info_from_value(pane)
                .ok_or_else(|| "malformed pane list response: pane is missing pane_id".to_string())
        })
        .collect()
}

/// Parse the pane object from herdr `pane current` JSON.
pub fn parse_pane_current(json: &str) -> Option<PaneInfo> {
    parse_pane_current_result(json).ok().flatten()
}

/// Strict current-pane parser. An explicit null pane means no current pane; absent or malformed
/// result shapes are errors so callers do not silently substitute a different discovery result.
pub fn parse_pane_current_result(json: &str) -> Result<Option<PaneInfo>, String> {
    let data = serde_json::from_str::<Value>(json)
        .map_err(|error| format!("malformed pane current response: {error}"))?;
    let pane = data
        .get("result")
        .and_then(|result| result.get("pane"))
        .ok_or_else(|| "malformed pane current response: missing result.pane".to_string())?;
    if pane.is_null() {
        return Ok(None);
    }
    pane_info_from_value(pane)
        .map(Some)
        .ok_or_else(|| "malformed pane current response: pane is missing pane_id".to_string())
}

fn pane_info_from_value(pane: &Value) -> Option<PaneInfo> {
    let pane_id = pane
        .get("pane_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    Some(PaneInfo {
        pane_id,
        cwd: opt_string(pane, "cwd"),
        foreground_cwd: opt_string(pane, "foreground_cwd"),
        agent: opt_string(pane, "agent"),
        agent_session: pane.get("agent_session").and_then(parse_agent_session),
        observed: parse_observed_status(pane.get("agent_status").and_then(|v| v.as_str())),
        label: opt_string(pane, "label"),
        terminal_title_stripped: opt_string(pane, "terminal_title_stripped"),
        focused: pane
            .get("focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        workspace_id: opt_string(pane, "workspace_id"),
    })
}

/// Parse host `agent_status` strings into [`ObservedStatus`].
///
/// Unknown or empty values become [`ObservedStatus::Unknown`]. Missing → `None`.
pub fn parse_observed_status(raw: Option<&str>) -> Option<ObservedStatus> {
    let s = raw.map(str::trim).filter(|s| !s.is_empty())?;
    Some(match s.to_ascii_lowercase().as_str() {
        "working" | "running" | "active" => ObservedStatus::Working,
        "blocked" => ObservedStatus::Blocked,
        "idle" => ObservedStatus::Idle,
        "done" | "finished" | "completed" => ObservedStatus::Done,
        "unknown" => ObservedStatus::Unknown,
        _ => ObservedStatus::Unknown,
    })
}

/// Build an [`ObservationMap`] from pane list rows.
///
/// Inserts by exact pane id and, when supplied, exact stable session identity.
pub fn observations_from_panes(panes: &[PaneInfo]) -> ObservationMap {
    let mut map = ObservationMap::new();
    for pane in panes {
        let Some(status) = pane.observed else {
            continue;
        };
        map.insert_pane(&pane.pane_id, status);
        if let Some(session) = pane.agent_session.as_ref() {
            map.insert_session(session, status);
        }
    }
    map
}

fn opt_string(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_agent_session(value: &Value) -> Option<AgentSessionIdentity> {
    Some(AgentSessionIdentity {
        source: exact_non_empty_string(value, "source")?,
        value: exact_non_empty_string(value, "value")?,
    })
}

fn exact_non_empty_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

fn validate_host_id(value: &str, kind: &str) -> Result<(), String> {
    if value.starts_with('-')
        || value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | ':' | '-')
        })
    {
        return Err(format!("unsafe host {kind} identifier"));
    }
    Ok(())
}

/// True when the pane is the Tasks board (label or stripped title).
pub fn is_tasks_pane(pane: &PaneInfo) -> bool {
    pane.label.as_deref() == Some(BOARD_PANE_LABEL)
        || pane.terminal_title_stripped.as_deref() == Some(BOARD_PANE_LABEL)
}

/// Pick one work pane in the board workspace (not the Tasks board).
///
/// This legacy helper remains for the explicit Link action. Park uses the typed,
/// ambiguity-preserving resolver in `context` instead. Never cross a workspace boundary.
pub fn select_work_pane(
    panes: &[PaneInfo],
    current: Option<&PaneInfo>,
    board_workspace_id: Option<&str>,
) -> Option<PaneInfo> {
    let ws = non_empty(board_workspace_id)?;
    let eligible =
        |pane: &PaneInfo| !is_tasks_pane(pane) && pane.workspace_id.as_deref() == Some(ws);
    if let Some(p) = panes.iter().find(|p| p.focused && eligible(p)) {
        return Some(p.clone());
    }
    if let Some(p) = current.filter(|p| eligible(p)) {
        return Some(p.clone());
    }
    panes.iter().find(|p| eligible(p)).cloned()
}

/// Infer the invoking board's workspace id (current Tasks pane, then focused Tasks pane).
pub fn board_workspace_id(panes: &[PaneInfo], current: Option<&PaneInfo>) -> Option<String> {
    current
        .filter(|pane| is_tasks_pane(pane))
        .and_then(|pane| pane.workspace_id.clone())
        .or_else(|| {
            panes
                .iter()
                .find(|pane| pane.focused && is_tasks_pane(pane))
                .and_then(|pane| pane.workspace_id.clone())
        })
        .or_else(|| {
            panes
                .iter()
                .find(|pane| is_tasks_pane(pane))
                .and_then(|pane| pane.workspace_id.clone())
        })
}

/// Argv fragments that must never appear in host resume calls.
pub const FORBIDDEN_RESUME_ARGS: &[&str] = &["create", "agent", "dispatch", "start-agent"];

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Recording host for pure unit tests (no process spawn).
    #[derive(Debug, Default)]
    struct FakeHost {
        panes: Vec<String>,
        usable: Vec<String>,
        calls: Arc<Mutex<Vec<Vec<String>>>>,
        fail_focus: bool,
        fail_open: bool,
    }

    impl HostPorts for FakeHost {
        fn list_pane_ids(&self) -> Result<Vec<String>, String> {
            self.calls
                .lock()
                .unwrap()
                .push(vec!["pane".into(), "list".into()]);
            Ok(self.panes.clone())
        }

        fn focus_pane(&self, pane_id: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(vec![
                "plugin".into(),
                "pane".into(),
                "focus".into(),
                pane_id.into(),
            ]);
            if self.fail_focus {
                return Err("focus failed".into());
            }
            Ok(())
        }

        fn open_path(&self, path: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(vec![
                "worktree".into(),
                "open".into(),
                "--path".into(),
                path.into(),
                "--focus".into(),
            ]);
            if self.fail_open {
                return Err("open failed".into());
            }
            Ok(())
        }

        fn path_usable(&self, path: &str) -> bool {
            self.usable.iter().any(|p| p == path)
        }
    }

    #[test]
    fn parse_pane_ids_extracts_ids() {
        let json = r#"{
            "result": {
                "panes": [
                    {"pane_id": "w0:p1", "label": "Editor"},
                    {"pane_id": "w0:p2", "label": "Tasks"}
                ]
            }
        }"#;
        assert_eq!(parse_pane_ids(json), vec!["w0:p1", "w0:p2"]);
    }

    #[test]
    fn parse_pane_ids_empty_on_bad_json() {
        assert!(parse_pane_ids("nope").is_empty());
        assert!(parse_pane_ids("{}").is_empty());
    }

    #[test]
    fn malformed_live_pane_output_is_a_host_failure_not_an_empty_snapshot() {
        assert!(parse_pane_list_result("not json").is_err());
        assert!(parse_pane_list_result(r#"{"result":{}}"#).is_err());
        assert!(parse_pane_current_result("not json").is_err());
        assert!(parse_pane_current_result(r#"{"result":{}}"#).is_err());
        assert_eq!(
            parse_pane_current_result(r#"{"result":{"pane":null}}"#).unwrap(),
            None,
            "only an explicit null represents no current pane"
        );
    }

    #[test]
    fn receipt_operations_reject_unsafe_host_identifiers_before_running_cli() {
        let host = HerdrHost::with_bin("/definitely/not/a/herdr-binary");
        let pane = PaneReceipt {
            pane_id: "--workspace=other".into(),
        };
        let worktree = WorktreeReceipt {
            path: "/display-only".into(),
            workspace_id: "--force".into(),
        };
        assert!(host.focus_pane("--on").is_err());
        assert!(host.start_agent("task", "grok", "--pane=other").is_err());
        assert!(host.agent_prompt("--pane=other", "hello").is_err());
        assert!(host.confirm_pane_receipt(&pane).is_err());
        assert!(host.remove_pane_receipt(&pane).is_err());
        assert!(host.confirm_worktree_receipt(&worktree).is_err());
        assert!(host.remove_worktree_receipt(&worktree).is_err());
    }

    #[test]
    fn parse_pane_list_preserves_agent_session_source_value() {
        let json = r#"{
            "result": {
                "panes": [{
                    "pane_id": "w10:p1R",
                    "agent": "pi",
                    "agent_session": {
                        "source": "herdr:pi",
                        "value": "session-4f2c"
                    }
                }]
            }
        }"#;

        let panes = parse_pane_list(json);
        let session = panes[0].agent_session.as_ref().expect("agent session");
        assert_eq!(session.source, "herdr:pi");
        assert_eq!(session.value, "session-4f2c");
        assert_eq!(panes[0].agent.as_deref(), Some("pi"));
        assert_eq!(panes[0].pane_id, "w10:p1R");
    }

    #[test]
    fn parse_pane_list_and_current_fill_fields() {
        let list = r#"{
            "result": {
                "panes": [
                    {
                        "pane_id": "w0:p1",
                        "label": "Grok",
                        "cwd": "/work/repo",
                        "foreground_cwd": "/work/repo/src",
                        "agent": "grok",
                        "agent_status": "working",
                        "focused": true,
                        "workspace_id": "w0"
                    },
                    {
                        "pane_id": "w0:p2",
                        "label": "Tasks",
                        "cwd": "/plugin",
                        "focused": false,
                        "workspace_id": "w0"
                    }
                ]
            }
        }"#;
        let panes = parse_pane_list(list);
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].pane_id, "w0:p1");
        assert_eq!(panes[0].foreground_cwd.as_deref(), Some("/work/repo/src"));
        assert_eq!(panes[0].agent.as_deref(), Some("grok"));
        assert_eq!(panes[0].observed, Some(ObservedStatus::Working));
        assert!(panes[0].focused);
        assert!(is_tasks_pane(&panes[1]));
        assert!(panes[1].observed.is_none());

        let current_json = r#"{
            "result": {
                "pane": {
                    "pane_id": "w0:p2",
                    "label": "Tasks",
                    "cwd": "/plugin",
                    "workspace_id": "w0"
                }
            }
        }"#;
        let cur = parse_pane_current(current_json).expect("current");
        assert_eq!(cur.pane_id, "w0:p2");
        assert!(is_tasks_pane(&cur));
    }

    #[test]
    fn select_work_pane_prefers_focused_non_tasks() {
        let panes = vec![
            PaneInfo {
                pane_id: "w0:p1".into(),
                cwd: Some("/work".into()),
                focused: true,
                workspace_id: Some("w0".into()),
                ..PaneInfo::default()
            },
            PaneInfo {
                pane_id: "w0:p2".into(),
                label: Some("Tasks".into()),
                cwd: Some("/plugin".into()),
                focused: false,
                workspace_id: Some("w0".into()),
                ..PaneInfo::default()
            },
        ];
        let selected = select_work_pane(&panes, Some(&panes[1]), Some("w0")).expect("work");
        assert_eq!(selected.pane_id, "w0:p1");
    }

    #[test]
    fn park_source_excludes_focused_panes_outside_board_workspace() {
        let panes = vec![
            PaneInfo {
                pane_id: "w1:p1".into(),
                cwd: Some("/other/repo".into()),
                focused: true,
                workspace_id: Some("w1".into()),
                ..PaneInfo::default()
            },
            PaneInfo {
                pane_id: "w0:p2".into(),
                label: Some("Tasks".into()),
                focused: false,
                workspace_id: Some("w0".into()),
                ..PaneInfo::default()
            },
        ];

        assert!(
            select_work_pane(&panes, Some(&panes[0]), Some("w0")).is_none(),
            "a focused pane outside the Tasks workspace must not become the Park source"
        );
    }

    #[test]
    fn select_work_pane_uses_current_when_not_tasks() {
        let panes = vec![
            PaneInfo {
                pane_id: "w0:p1".into(),
                cwd: Some("/work".into()),
                focused: false,
                workspace_id: Some("w0".into()),
                ..PaneInfo::default()
            },
            PaneInfo {
                pane_id: "w0:p2".into(),
                label: Some("Tasks".into()),
                focused: false,
                workspace_id: Some("w0".into()),
                ..PaneInfo::default()
            },
        ];
        let current = panes[0].clone();
        let selected = select_work_pane(&panes, Some(&current), Some("w0")).expect("work");
        assert_eq!(selected.pane_id, "w0:p1");
    }

    #[test]
    fn select_work_pane_same_workspace_when_tasks_is_current() {
        let panes = vec![
            PaneInfo {
                pane_id: "w0:p1".into(),
                cwd: Some("/work/repo".into()),
                foreground_cwd: Some("/work/repo/src".into()),
                agent: Some("grok".into()),
                focused: false,
                workspace_id: Some("w0".into()),
                ..PaneInfo::default()
            },
            PaneInfo {
                pane_id: "w0:p2".into(),
                label: Some("Tasks".into()),
                terminal_title_stripped: Some("Tasks".into()),
                cwd: Some("/plugin-state".into()),
                focused: true,
                workspace_id: Some("w0".into()),
                ..PaneInfo::default()
            },
        ];
        let selected = select_work_pane(&panes, Some(&panes[1]), Some("w0")).expect("work");
        assert_eq!(selected.pane_id, "w0:p1");
        assert_eq!(selected.effective_cwd(), Some("/work/repo/src"));
    }

    #[test]
    fn select_work_pane_none_when_only_tasks() {
        let panes = vec![PaneInfo {
            pane_id: "w0:p2".into(),
            label: Some("Tasks".into()),
            focused: true,
            workspace_id: Some("w0".into()),
            ..PaneInfo::default()
        }];
        assert!(select_work_pane(&panes, Some(&panes[0]), Some("w0")).is_none());
    }

    #[test]
    fn parse_observed_status_maps_host_strings() {
        assert_eq!(
            parse_observed_status(Some("working")),
            Some(ObservedStatus::Working)
        );
        assert_eq!(
            parse_observed_status(Some("BLOCKED")),
            Some(ObservedStatus::Blocked)
        );
        assert_eq!(
            parse_observed_status(Some("idle")),
            Some(ObservedStatus::Idle)
        );
        assert_eq!(
            parse_observed_status(Some("done")),
            Some(ObservedStatus::Done)
        );
        assert_eq!(
            parse_observed_status(Some("finished")),
            Some(ObservedStatus::Done)
        );
        assert_eq!(
            parse_observed_status(Some("completed")),
            Some(ObservedStatus::Done)
        );
        assert_eq!(
            parse_observed_status(Some("unknown")),
            Some(ObservedStatus::Unknown)
        );
        assert_eq!(
            parse_observed_status(Some("weird-future-value")),
            Some(ObservedStatus::Unknown)
        );
        assert_eq!(parse_observed_status(None), None);
        assert_eq!(parse_observed_status(Some("  ")), None);
    }

    #[test]
    fn observations_from_panes_indexes_exact_pane_and_session() {
        let panes = vec![
            PaneInfo {
                pane_id: "w0:p1".into(),
                agent: Some("grok".into()),
                agent_session: Some(AgentSessionIdentity {
                    source: "herdr".into(),
                    value: "session-1".into(),
                }),
                observed: Some(ObservedStatus::Done),
                ..PaneInfo::default()
            },
            PaneInfo {
                pane_id: "w0:p2".into(),
                agent: Some("grok".into()),
                agent_session: Some(AgentSessionIdentity {
                    source: "herdr".into(),
                    value: "session-2".into(),
                }),
                observed: Some(ObservedStatus::Blocked),
                ..PaneInfo::default()
            },
            PaneInfo {
                pane_id: "w0:p3".into(),
                // no observed: skipped
                ..PaneInfo::default()
            },
        ];
        let map = observations_from_panes(&panes);
        assert_eq!(map.lookup(Some("w0:p1"), None), Some(ObservedStatus::Done));
        assert_eq!(
            map.lookup(
                None,
                Some(&AgentSessionIdentity {
                    source: "herdr".into(),
                    value: "session-1".into(),
                }),
            ),
            Some(ObservedStatus::Done)
        );
        assert_eq!(
            map.lookup(
                None,
                Some(&AgentSessionIdentity {
                    source: "herdr".into(),
                    value: "session-2".into(),
                }),
            ),
            Some(ObservedStatus::Blocked)
        );
        assert_eq!(map.lookup(None, None), None);
        assert_eq!(
            map.lookup(Some("w0:p2"), None),
            Some(ObservedStatus::Blocked)
        );
        assert_eq!(map.lookup(Some("w0:p3"), None), None);
    }

    #[test]
    fn parse_pane_list_agent_status_fixture() {
        let list = r#"{
            "result": {
                "panes": [
                    {
                        "pane_id": "w1:p0",
                        "agent": "codex",
                        "agent_status": "blocked"
                    },
                    {
                        "pane_id": "w1:p1",
                        "agent": "grok",
                        "agent_status": "done"
                    },
                    {
                        "pane_id": "w1:p2",
                        "agent_status": "idle"
                    }
                ]
            }
        }"#;
        let panes = parse_pane_list(list);
        assert_eq!(panes[0].observed, Some(ObservedStatus::Blocked));
        assert_eq!(panes[1].observed, Some(ObservedStatus::Done));
        assert_eq!(panes[2].observed, Some(ObservedStatus::Idle));
        let map = observations_from_panes(&panes);
        assert_eq!(map.lookup(Some("w1:p1"), None), Some(ObservedStatus::Done));
    }

    #[test]
    fn fake_host_focus_argv_has_pane_zoom_no_create() {
        let host = FakeHost {
            panes: vec!["w0:p1".into()],
            ..FakeHost::default()
        };
        host.focus_pane("w0:p1").expect("focus");
        let calls = host.calls.lock().unwrap().clone();
        // FakeHost records one logical call; HerdrHost issues zoom on/off.
        assert_eq!(calls.len(), 1);
        let argv = &calls[0];
        assert!(argv.contains(&"w0:p1".to_string()));
        for bad in FORBIDDEN_RESUME_ARGS {
            assert!(
                !argv
                    .iter()
                    .any(|a| a == bad || a.contains("worktree create")),
                "forbidden token {bad} in {argv:?}"
            );
        }
        assert!(!argv.windows(2).any(|w| w == ["worktree", "create"]));
    }

    #[test]
    fn fake_host_open_argv_has_worktree_open_path_focus_no_create() {
        let host = FakeHost::default();
        host.open_path("/repos/app").expect("open");
        let calls = host.calls.lock().unwrap().clone();
        let argv = &calls[0];
        assert!(argv.windows(2).any(|w| w == ["worktree", "open"]));
        assert!(argv.windows(2).any(|w| w == ["--path", "/repos/app"]));
        assert!(argv.iter().any(|a| a == "--focus"));
        assert!(!argv.windows(2).any(|w| w == ["worktree", "create"]));
        assert!(!argv.iter().any(|a| a.contains("agent")));
    }

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = env::temp_dir().join(format!("herdr-tasks-host-{nanos}"));
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// Fake herdr binary: logs argv, returns pane list JSON when asked.
    fn write_fake_herdr(dir: &Path, log_path: &Path, pane_ids: &[&str]) -> PathBuf {
        let bin = dir.join("fake-herdr");
        let panes_json: Vec<String> = pane_ids
            .iter()
            .map(|id| format!(r#"{{"pane_id":"{id}","label":"x"}}"#))
            .collect();
        let panes_joined = panes_json.join(",");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
if [ "$1" = "pane" ] && [ "$2" = "list" ]; then
  cat <<'EOF'
{{"result":{{"panes":[{panes}]}}}}
EOF
  exit 0
fi
if [ "$1" = "pane" ] && [ "$2" = "zoom" ]; then
  exit 0
fi
if [ "$1" = "worktree" ] && [ "$2" = "open" ]; then
  exit 0
fi
exit 0
"#,
            log = log_path.display(),
            panes = panes_joined
        );
        fs::write(&bin, script).expect("write fake herdr");
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();
        bin
    }

    #[test]
    fn herdr_host_focus_and_open_invoke_expected_argv_without_create_or_agent() {
        let dir = temp_dir();
        let log = dir.join("calls.log");
        let bin = write_fake_herdr(&dir, &log, &["w0:p9"]);
        let host = HerdrHost::with_bin(&bin);

        host.focus_pane("w0:p9").expect("focus");
        host.open_path("/tmp/resume-path").expect("open");
        let ids = host.list_pane_ids().expect("list");
        assert_eq!(ids, vec!["w0:p9".to_string()]);

        let log_text = fs::read_to_string(&log).expect("read log");
        assert!(
            log_text.contains("pane zoom w0:p9 --on") && log_text.contains("pane zoom w0:p9 --off"),
            "focus argv missing zoom on/off: {log_text}"
        );
        assert!(
            log_text.contains("worktree open --path /tmp/resume-path --focus"),
            "open argv missing: {log_text}"
        );
        assert!(
            log_text.contains("pane list"),
            "list argv missing: {log_text}"
        );
        assert!(
            !log_text.contains("worktree create"),
            "must not create worktree: {log_text}"
        );
        assert!(
            !log_text.contains("agent"),
            "must not start agent: {log_text}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Fake herdr for dispatch: pane split, agent start, agent prompt; never worktree create.
    fn write_fake_herdr_dispatch(dir: &Path, log_path: &Path) -> PathBuf {
        let bin = dir.join("fake-herdr-dispatch");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
# Refuse worktree create (contract).
if [ "$1" = "worktree" ] && [ "$2" = "create" ]; then
  echo '{{"error":{{"code":"forbidden","message":"worktree create forbidden"}}}}' >&2
  exit 2
fi
if [ "$1" = "pane" ] && [ "$2" = "split" ]; then
  cat <<'EOF'
{{"id":"cli:pane:split","result":{{"type":"pane_info","pane":{{"pane_id":"w0:p_split","cwd":"/work"}}}}}}
EOF
  exit 0
fi
if [ "$1" = "agent" ] && [ "$2" = "start" ]; then
  cat <<'EOF'
{{"id":"cli:agent:start","result":{{"type":"agent_started","agent":{{"pane_id":"w0:p_split","agent":"grok","name":"t123"}},"argv":["grok"]}}}}
EOF
  exit 0
fi
if [ "$1" = "agent" ] && [ "$2" = "prompt" ]; then
  cat <<'EOF'
{{"id":"cli:agent:prompt","result":{{"type":"agent_prompted","agent":{{"pane_id":"w0:p_split","agent":"grok"}}}}}}
EOF
  exit 0
fi
exit 0
"#,
            log = log_path.display()
        );
        fs::write(&bin, script).expect("write fake herdr dispatch");
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();
        bin
    }

    #[test]
    fn parse_split_pane_id_from_result_pane() {
        let json = r#"{"result":{"type":"pane_info","pane":{"pane_id":"w1:p2"}}}"#;
        assert_eq!(parse_split_pane_id(json).as_deref(), Some("w1:p2"));
        assert!(parse_split_pane_id("nope").is_none());
    }

    #[test]
    fn parse_started_agent_rejects_unsafe_host_pane_id() {
        let json = r#"{
            "result": {
                "agent": {"pane_id": "--pane=other", "agent": "grok"}
            }
        }"#;
        assert!(parse_started_agent(json, "w0:p1", "grok").is_err());
    }

    #[test]
    fn parse_started_agent_from_agent_started() {
        let json = r#"{
            "result": {
                "type": "agent_started",
                "agent": {
                    "pane_id": "w0:p3",
                    "agent": "grok",
                    "name": "t1",
                    "agent_session": {
                        "source": "herdr:grok",
                        "value": "session-dispatch"
                    }
                },
                "argv": ["grok"]
            }
        }"#;
        let s = parse_started_agent(json, "fallback", "kind").expect("parse");
        assert_eq!(s.pane_id, "w0:p3");
        assert_eq!(s.agent.as_deref(), Some("grok"));
        assert_eq!(
            s.agent_session
                .as_ref()
                .map(|session| (session.source.as_str(), session.value.as_str())),
            Some(("herdr:grok", "session-dispatch"))
        );
    }

    #[test]
    fn herdr_host_dispatch_argv_split_start_prompt_no_worktree_create() {
        let dir = temp_dir();
        let log = dir.join("dispatch.log");
        let bin = write_fake_herdr_dispatch(&dir, &log);
        let host = HerdrHost::with_bin(&bin);

        let pane = host.open_shell_at_cwd("/work/repo").expect("split");
        assert_eq!(pane, "w0:p_split");
        let started = host.start_agent("t12345678", "grok", &pane).expect("start");
        assert_eq!(started.pane_id, "w0:p_split");
        assert_eq!(started.agent.as_deref(), Some("grok"));
        host.agent_prompt(&pane, "--wait").expect("prompt");

        let log_text = fs::read_to_string(&log).expect("log");
        assert!(
            log_text.contains("pane split") && log_text.contains("--cwd /work/repo"),
            "split argv: {log_text}"
        );
        assert!(
            log_text.contains("agent start t12345678 --kind grok --pane w0:p_split")
                && log_text.contains("--timeout"),
            "start argv: {log_text}"
        );
        assert!(
            log_text.contains("agent prompt -- w0:p_split --wait"),
            "prompt positional values must follow the CLI option terminator: {log_text}"
        );
        assert!(
            !log_text.contains("worktree create"),
            "must not create worktree: {log_text}"
        );
        for bad in FORBIDDEN_DISPATCH_ARGS {
            // "worktree" alone is not required forbidden on open path; create pair is.
            let _ = bad;
        }
        assert!(!log_text.contains("worktree create"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_worktree_checkout_path_from_worktree_created() {
        let json = r#"{
            "id": "cli:worktree:create",
            "result": {
                "type": "worktree_created",
                "workspace": {
                    "workspace_id": "w_1",
                    "worktree": {
                        "checkout_path": "/worktrees/repo/tasks-abc",
                        "is_linked_worktree": true
                    }
                },
                "root_pane": {"pane_id": "w1:p0", "cwd": "/worktrees/repo/tasks-abc"},
                "worktree": {
                    "path": "/worktrees/repo/tasks-abc",
                    "branch": "tasks/abcdef12",
                    "is_bare": false,
                    "is_detached": false,
                    "is_prunable": false,
                    "is_linked_worktree": true,
                    "label": "tasks-abc"
                }
            }
        }"#;
        assert_eq!(
            parse_worktree_checkout_path(json).as_deref(),
            Some("/worktrees/repo/tasks-abc")
        );
        assert!(parse_worktree_checkout_path("nope").is_none());
        assert!(parse_worktree_checkout_path(r#"{"error":{"message":"fail"}}"#).is_none());
    }

    #[test]
    fn parse_worktree_checkout_path_fallbacks() {
        let via_workspace = r#"{
            "result": {
                "type": "worktree_created",
                "workspace": {
                    "worktree": {"checkout_path": "/fallback/ws"}
                }
            }
        }"#;
        assert_eq!(
            parse_worktree_checkout_path(via_workspace).as_deref(),
            Some("/fallback/ws")
        );
        let via_pane = r#"{
            "result": {
                "type": "worktree_created",
                "root_pane": {"cwd": "/fallback/pane"}
            }
        }"#;
        assert_eq!(
            parse_worktree_checkout_path(via_pane).as_deref(),
            Some("/fallback/pane")
        );
    }

    /// Fake herdr for worktree create: logs argv, returns checkout path JSON.
    fn write_fake_herdr_worktree_create(dir: &Path, log_path: &Path) -> PathBuf {
        let bin = dir.join("fake-herdr-wt-create");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
if [ "$1" = "worktree" ] && [ "$2" = "create" ]; then
  # Require --cwd and --branch for the contract under test.
  echo "$*" | grep -q -- '--cwd' || exit 3
  echo "$*" | grep -q -- '--branch' || exit 3
  cat <<'EOF'
{{"id":"cli:worktree:create","result":{{"type":"worktree_created","worktree":{{"path":"/tmp/fake-wt/tasks-deadbeef","branch":"tasks/deadbeef","is_bare":false,"is_detached":false,"is_prunable":false,"is_linked_worktree":true,"label":"tasks-deadbeef"}}}}}}
EOF
  exit 0
fi
exit 0
"#,
            log = log_path.display()
        );
        fs::write(&bin, script).expect("write fake herdr wt create");
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();
        bin
    }

    #[test]
    fn herdr_host_create_worktree_argv_and_parses_checkout_path() {
        let dir = temp_dir();
        let log = dir.join("wt-create.log");
        let bin = write_fake_herdr_worktree_create(&dir, &log);
        let host = HerdrHost::with_bin(&bin);

        let path = host
            .create_worktree("/repos/app", "tasks/deadbeef")
            .expect("create_worktree");
        assert_eq!(path, "/tmp/fake-wt/tasks-deadbeef");

        let log_text = fs::read_to_string(&log).expect("log");
        assert!(
            log_text.contains("worktree create")
                && log_text.contains("--cwd /repos/app")
                && log_text.contains("--branch tasks/deadbeef")
                && log_text.contains("--no-focus"),
            "create argv missing expected flags: {log_text}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn herdr_host_create_worktree_failure_propagates() {
        let dir = temp_dir();
        let log = dir.join("wt-fail.log");
        let bin = dir.join("fake-herdr-wt-fail");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
echo '{{"error":{{"code":"failed","message":"branch exists"}}}}' >&2
exit 1
"#,
            log = log.display()
        );
        fs::write(&bin, script).expect("write");
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();

        let host = HerdrHost::with_bin(&bin);
        let err = host
            .create_worktree("/repos/app", "tasks/x")
            .expect_err("must fail");
        assert!(
            err.contains("failed") || err.contains("branch") || err.contains("worktree"),
            "unexpected err: {err}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_receipts_preserves_only_exact_host_identities() {
        let worktree = parse_worktree_receipt(
            r#"{"result":{"workspace":{"workspace_id":"w_owned"},"worktree":{"path":"/worktrees/task"}}}"#,
        )
        .expect("worktree receipt");
        assert_eq!(
            worktree,
            WorktreeReceipt {
                path: "/worktrees/task".into(),
                workspace_id: "w_owned".into(),
            }
        );
        assert!(
            parse_worktree_receipt(r#"{"result":{"worktree":{"path":"/worktrees/task"}}}"#)
                .is_none()
        );

        let pane = parse_pane_receipt(r#"{"result":{"pane":{"pane_id":"w_owned:p_created"}}}"#)
            .expect("pane receipt");
        assert_eq!(pane.pane_id, "w_owned:p_created");

        let agent = parse_agent_receipt(
            r#"{"result":{"agent":{"pane_id":"w_owned:p_created","name":"host-normalized-agent","agent_session":{"source":"herdr:grok","value":"s_123"}}}}"#,
            "w_owned:p_created",
        )
        .expect("agent receipt");
        assert_eq!(agent.pane_id, "w_owned:p_created");
        assert_eq!(agent.display_name.as_deref(), Some("host-normalized-agent"));
        assert_eq!(
            agent.agent_session,
            Some(AgentSessionIdentity {
                source: "herdr:grok".into(),
                value: "s_123".into(),
            })
        );
    }

    fn write_fake_herdr_receipts(dir: &Path, log_path: &Path) -> PathBuf {
        let bin = dir.join("fake-herdr-receipts");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{log}"
if [ "$1" = "worktree" ] && [ "$2" = "create" ]; then
  cat <<'EOF'
{{"result":{{"workspace":{{"workspace_id":"w_owned"}},"worktree":{{"path":"/worktrees/task"}}}}}}
EOF
  exit 0
fi
if [ "$1" = "pane" ] && [ "$2" = "split" ]; then
  echo '{{"result":{{"pane":{{"pane_id":"w_owned:p_created"}}}}}}'
  exit 0
fi
if [ "$1" = "agent" ] && [ "$2" = "start" ]; then
  echo '{{"result":{{"agent":{{"pane_id":"w_owned:p_created","agent_session":{{"source":"herdr:grok","value":"s_123"}}}}}}}}'
  exit 0
fi
if [ "$1" = "worktree" ] && [ "$2" = "list" ] && [ "$3" = "--workspace" ] && [ "$4" = "w_owned" ]; then
  echo '{{"result":{{"worktrees":[{{"open_workspace_id":"w_owned","path":"/worktrees/task"}}]}}}}'
  exit 0
fi
if [ "$1" = "pane" ] && [ "$2" = "get" ] && [ "$3" = "w_owned:p_created" ]; then
  echo '{{"result":{{"pane":{{"pane_id":"w_owned:p_created","agent_session":{{"source":"herdr:grok","value":"s_123"}}}}}}}}'
  exit 0
fi
exit 0
"#,
            log = log_path.display()
        );
        fs::write(&bin, script).expect("write fake herdr receipts");
        let mut perms = fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).unwrap();
        bin
    }

    #[test]
    fn herdr_host_receipts_confirm_and_remove_by_exact_recorded_identity() {
        let dir = temp_dir();
        let log = dir.join("receipts.log");
        let host = HerdrHost::with_bin(write_fake_herdr_receipts(&dir, &log));

        let worktree = host
            .create_worktree_receipt("/repos/app", "tasks/deadbeef")
            .expect("worktree receipt");
        let pane = host
            .open_shell_at_cwd_receipt("/worktrees/task")
            .expect("pane receipt");
        let agent = host
            .start_agent_receipt("t123", "grok", &pane.pane_id)
            .expect("agent receipt");
        let display_only_worktree = WorktreeReceipt {
            path: "/ambient/display-only".into(),
            workspace_id: worktree.workspace_id.clone(),
        };
        assert!(host
            .confirm_worktree_receipt(&display_only_worktree)
            .expect("worktree confirm"));
        assert!(host.confirm_pane_receipt(&pane).expect("pane confirm"));
        assert!(host.confirm_agent_receipt(&agent).expect("agent confirm"));
        assert!(!host
            .confirm_agent_receipt(&AgentReceipt {
                pane_id: agent.pane_id.clone(),
                display_name: None,
                agent_session: Some(AgentSessionIdentity {
                    source: "herdr:grok".into(),
                    value: "other-session".into(),
                }),
            })
            .expect("mismatched session is not confirmed"));

        host.remove_pane_receipt(&pane).expect("close exact pane");
        host.remove_worktree_receipt(&worktree)
            .expect("remove exact workspace");

        let log_text = fs::read_to_string(&log).expect("read log");
        assert!(
            log_text.contains("pane close w_owned:p_created"),
            "close argv: {log_text}"
        );
        assert!(
            log_text.contains("worktree remove --workspace w_owned --force --json"),
            "worktree removal argv: {log_text}"
        );
        assert!(
            log_text.contains("worktree list --workspace w_owned")
                && log_text.contains("pane get w_owned:p_created"),
            "confirmation must use only receipt identities: {log_text}"
        );
        assert!(
            !log_text.contains("/ambient/display-only"),
            "confirmation/removal must not use an ambient display path: {log_text}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn receipt_ports_are_safely_unsupported_by_legacy_test_hosts() {
        let host = FakeHost::default();
        assert!(host
            .confirm_worktree_receipt(&WorktreeReceipt {
                path: "/ambient/display-only".into(),
                workspace_id: "w_owned".into(),
            })
            .is_err());
        assert!(host
            .remove_pane_receipt(&PaneReceipt {
                pane_id: "w_owned:p_created".into(),
            })
            .is_err());
    }

    #[test]
    fn plugin_manifest_requires_herdr_0_7_5() {
        let manifest =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/herdr-plugin.toml"))
                .expect("read manifest");
        assert!(manifest.contains("min_herdr_version = \"0.7.5\""));
    }
}
