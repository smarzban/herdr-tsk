//! Attention Reactor: pure refresh of linked-task observations.
//!
//! Maps host pane/agent observations onto domain `apply_agent_observation` for
//! linked tasks only. Host list failure is handled by the caller (skip cycle).

use std::collections::HashMap;
use std::time::Duration;

use uuid::Uuid;

use crate::domain::{is_linked, AgentSessionIdentity, DomainState, ObservedStatus};
use crate::host::{observations_from_panes, HostPorts};

/// Default poll interval while the board is focused.
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Host observation keyed by exact pane id or exact stable session identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservationMap {
    by_pane: HashMap<String, ObservedStatus>,
    by_session: HashMap<(String, String), ObservedStatus>,
}

impl ObservationMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_pane(&mut self, pane_id: impl Into<String>, status: ObservedStatus) {
        self.by_pane.insert(pane_id.into(), status);
    }

    pub fn insert_session(&mut self, session: &AgentSessionIdentity, status: ObservedStatus) {
        self.by_session
            .insert((session.source.clone(), session.value.clone()), status);
    }

    /// Resolve only by stable session identity, or exact pane identity for legacy links.
    pub fn lookup(
        &self,
        pane_id: Option<&str>,
        session: Option<&AgentSessionIdentity>,
    ) -> Option<ObservedStatus> {
        if let Some(session) = session {
            return self
                .by_session
                .get(&(session.source.clone(), session.value.clone()))
                .copied();
        }
        pane_id.and_then(|pid| self.by_pane.get(pid).copied())
    }
}

/// Classification of a linked task during one host snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionClassification {
    Matched,
    Stale,
    Unavailable,
}

/// Result of one pure reactor refresh cycle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RefreshResult {
    /// Task ids whose human status changed.
    pub status_changed: Vec<Uuid>,
    /// Task ids that received a new/updated last_observed (including no status change).
    pub observed: Vec<Uuid>,
    /// Classification for every linked task considered during a successful snapshot.
    pub classifications: Vec<(Uuid, AttentionClassification)>,
}

impl RefreshResult {
    pub fn unavailable(state: &DomainState) -> Self {
        let classifications = state
            .tasks()
            .iter()
            .filter(|task| !task.soft_deleted && is_linked(task))
            .map(|task| (task.id, AttentionClassification::Unavailable))
            .collect();
        Self {
            classifications,
            ..Self::default()
        }
    }

    /// True when any domain mutation occurred (status and/or last_observed).
    pub fn any_change(&self) -> bool {
        !self.status_changed.is_empty() || !self.observed.is_empty()
    }

    /// True when human status changed for at least one task.
    pub fn status_changed(&self) -> bool {
        !self.status_changed.is_empty()
    }

    pub fn stale_ids(&self) -> Vec<Uuid> {
        self.classifications
            .iter()
            .filter_map(|(id, classification)| {
                (*classification == AttentionClassification::Stale).then_some(*id)
            })
            .collect()
    }
}

/// One poll cycle: list panes via host, map observations, apply domain refresh.
///
/// On host list failure returns `Err` and leaves `state` unchanged (caller skips cycle).
pub fn poll_host(state: &mut DomainState, host: &dyn HostPorts) -> Result<RefreshResult, String> {
    let panes = host.list_panes()?;
    let map = observations_from_panes(&panes);
    Ok(refresh(state, &map))
}

/// Apply host observations to all linked, non-soft-deleted tasks.
///
/// Unlinked and soft-deleted tasks are never mutated. Missing observations for a linked
/// task are skipped (keep last known). Pure domain work: no I/O.
pub fn refresh(state: &mut DomainState, observations: &ObservationMap) -> RefreshResult {
    let mut result = RefreshResult::default();
    let linked_ids: Vec<Uuid> = state
        .tasks()
        .iter()
        .filter(|t| !t.soft_deleted && is_linked(t))
        .map(|t| t.id)
        .collect();

    for id in linked_ids {
        let Some(task) = state.get(id) else {
            continue;
        };
        let meta = task.agent_meta.as_ref();
        let pane = meta.and_then(|m| m.pane_id.as_deref());
        let session = meta.and_then(|m| m.agent_session.as_ref());
        let Some(observed) = observations.lookup(pane, session) else {
            result
                .classifications
                .push((id, AttentionClassification::Stale));
            continue;
        };
        result
            .classifications
            .push((id, AttentionClassification::Matched));
        if let Ok(true) = state.apply_agent_observation(id, observed) {
            result.status_changed.push(id);
            result.observed.push(id);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AgentMeta, AgentSessionIdentity, HumanStatus, ProvenanceOrigin, TaskScope,
    };

    fn linked(state: &mut DomainState, title: &str, pane: &str, agent: Option<&str>) -> Uuid {
        let id = state
            .create(
                title,
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create");
        state
            .link_agent(
                id,
                AgentMeta {
                    agent_id: agent.map(str::to_string),
                    pane_id: Some(pane.into()),
                    agent_session: None,
                },
            )
            .expect("link");
        id
    }

    #[test]
    fn refresh_done_sets_review_on_linked_only() {
        let mut state = DomainState::new();
        let linked_id = linked(&mut state, "linked", "w0:p1", Some("a1"));
        let unlinked = state
            .create(
                "unlinked",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create");

        let mut map = ObservationMap::new();
        map.insert_pane("w0:p1", ObservedStatus::Done);
        // Noise for some other pane: must not affect unlinked.
        map.insert_pane("w0:other", ObservedStatus::Done);

        let result = refresh(&mut state, &map);
        assert!(result.status_changed.contains(&linked_id));
        assert_eq!(state.get(linked_id).expect("t").status, HumanStatus::Review);
        assert_eq!(state.get(unlinked).expect("t").status, HumanStatus::Ready);
        assert!(state.get(unlinked).expect("t").last_observed.is_none());
    }

    #[test]
    fn refresh_blocked_sets_blocked() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "work", "w0:p2", None);
        let mut map = ObservationMap::new();
        map.insert_pane("w0:p2", ObservedStatus::Blocked);
        let result = refresh(&mut state, &map);
        assert!(result.status_changed.contains(&id));
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Blocked);
    }

    #[test]
    fn refresh_unlinked_ignores_host_activity() {
        let mut state = DomainState::new();
        let id = state
            .create(
                "plain",
                None,
                TaskScope::Global,
                None,
                None,
                ProvenanceOrigin::Manual,
            )
            .expect("create");
        let mut map = ObservationMap::new();
        map.insert_pane("w0:p1", ObservedStatus::Done);
        let result = refresh(&mut state, &map);
        assert!(!result.any_change());
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Ready);
    }

    #[test]
    fn refresh_prefers_pane_id_then_agent_id() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "both", "w0:p9", Some("agent-x"));
        let mut map = ObservationMap::new();
        // Pane Idle would no-op; agent Done would set review. Pane wins → no status change.
        map.insert_pane("w0:p9", ObservedStatus::Idle);
        let result = refresh(&mut state, &map);
        assert!(!result.any_change());
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Ready);
        assert!(state.get(id).expect("t").last_observed.is_none());
    }

    #[test]
    fn refresh_does_not_fallback_to_agent_kind_when_pane_is_missing() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "agent-only", "w0:gone", Some("agent-y"));
        let map = ObservationMap::new();
        let result = refresh(&mut state, &map);
        assert!(result.stale_ids().contains(&id));
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Ready);
    }

    #[test]
    fn session_bearing_link_never_falls_back_to_its_matching_pane_id() {
        let session = AgentSessionIdentity {
            source: "herdr:pi".into(),
            value: "recorded-session".into(),
        };
        let mut state = DomainState::new();
        let id = state
            .create(
                "stable identity wins",
                None,
                TaskScope::Global,
                None,
                Some(AgentMeta {
                    agent_id: Some("pi".into()),
                    pane_id: Some("w0:p1".into()),
                    agent_session: Some(session),
                }),
                ProvenanceOrigin::Manual,
            )
            .expect("create");
        let mut observations = ObservationMap::new();
        observations.insert_pane("w0:p1", ObservedStatus::Done);

        let before = state.get(id).unwrap().clone();
        let result = refresh(&mut state, &observations);
        assert!(result.stale_ids().contains(&id));
        assert_eq!(state.get(id).unwrap(), &before);
    }

    #[test]
    fn exact_session_identity_does_not_collide_by_agent_kind() {
        let first = AgentSessionIdentity {
            source: "herdr:pi".into(),
            value: "session-1".into(),
        };
        let second = AgentSessionIdentity {
            source: "herdr:pi".into(),
            value: "session-2".into(),
        };
        let mut map = ObservationMap::new();
        map.insert_session(&first, ObservedStatus::Done);
        map.insert_session(&second, ObservedStatus::Blocked);

        assert_eq!(map.lookup(None, Some(&first)), Some(ObservedStatus::Done));
        assert_eq!(
            map.lookup(None, Some(&second)),
            Some(ObservedStatus::Blocked)
        );
    }

    #[test]
    fn refresh_marks_missing_link_stale_without_domain_mutation() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "stale", "w0:p1", None);
        let before = state.get(id).expect("task").clone();
        let result = refresh(&mut state, &ObservationMap::new());
        assert!(result.stale_ids().contains(&id));
        let after = state.get(id).expect("task");
        assert_eq!(after.status, before.status);
        assert_eq!(after.last_observed, before.last_observed);
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.history, before.history);
    }

    #[test]
    fn refresh_missing_observation_keeps_last_known() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "keep", "w0:p1", None);
        let mut map = ObservationMap::new();
        map.insert_pane("w0:p1", ObservedStatus::Done);
        refresh(&mut state, &map);
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Review);
        assert_eq!(
            state.get(id).expect("t").last_observed,
            Some(ObservedStatus::Done)
        );

        // Next cycle: no observation for this pane; prior apply result stays, link is stale.
        let empty = ObservationMap::new();
        let result = refresh(&mut state, &empty);
        assert!(!result.any_change());
        assert!(result.stale_ids().contains(&id));
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Review);
        assert_eq!(
            state.get(id).expect("t").last_observed,
            Some(ObservedStatus::Done)
        );
    }

    #[test]
    fn refresh_skips_soft_deleted_linked_tasks() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "gone", "w0:p1", None);
        state.soft_delete(id).expect("del");
        let before = state.get(id).expect("t").clone();
        let mut map = ObservationMap::new();
        map.insert_pane("w0:p1", ObservedStatus::Done);
        let result = refresh(&mut state, &map);
        assert!(!result.any_change());
        let after = state.get(id).expect("t");
        assert_eq!(after.status, before.status);
        assert_eq!(after.last_observed, before.last_observed);
        assert_eq!(after.updated_at, before.updated_at);
    }

    #[test]
    fn refresh_idle_or_working_is_noop() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "idle", "w0:p1", None);
        let before = state.get(id).expect("t").clone();
        let mut map = ObservationMap::new();
        map.insert_pane("w0:p1", ObservedStatus::Working);
        let result = refresh(&mut state, &map);
        assert!(!result.any_change());
        let after = state.get(id).expect("t");
        assert_eq!(after.updated_at, before.updated_at);
        assert!(after.last_observed.is_none());
    }

    #[test]
    fn refresh_never_sets_done() {
        let mut state = DomainState::new();
        let id = linked(&mut state, "gate", "w0:p1", None);
        let mut map = ObservationMap::new();
        map.insert_pane("w0:p1", ObservedStatus::Done);
        refresh(&mut state, &map);
        assert_ne!(state.get(id).expect("t").status, HumanStatus::Done);
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Review);
    }

    #[test]
    fn poll_host_applies_pane_list_observations() {
        use crate::host::PaneInfo;

        struct Fake {
            panes: Vec<PaneInfo>,
            fail: bool,
        }
        impl HostPorts for Fake {
            fn list_pane_ids(&self) -> Result<Vec<String>, String> {
                Ok(self.list_panes()?.into_iter().map(|p| p.pane_id).collect())
            }
            fn list_panes(&self) -> Result<Vec<PaneInfo>, String> {
                if self.fail {
                    return Err("list failed".into());
                }
                Ok(self.panes.clone())
            }
            fn focus_pane(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
            fn open_path(&self, _: &str) -> Result<(), String> {
                Ok(())
            }
        }

        let mut state = DomainState::new();
        let id = linked(&mut state, "poll", "w0:p1", Some("grok"));
        let host = Fake {
            panes: vec![PaneInfo {
                pane_id: "w0:p1".into(),
                agent: Some("grok".into()),
                observed: Some(ObservedStatus::Done),
                ..PaneInfo::default()
            }],
            fail: false,
        };
        let result = poll_host(&mut state, &host).expect("poll");
        assert!(result.status_changed.contains(&id));
        assert_eq!(state.get(id).expect("t").status, HumanStatus::Review);

        // Host failure skips cycle (error returned; state unchanged).
        let status_before = state.get(id).expect("t").status;
        let host_fail = Fake {
            panes: vec![],
            fail: true,
        };
        assert!(poll_host(&mut state, &host_fail).is_err());
        assert_eq!(state.get(id).expect("t").status, status_before);
    }
}
