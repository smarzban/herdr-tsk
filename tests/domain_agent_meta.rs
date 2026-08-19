//! Domain: agent metadata + observation gate (soft link, never auto-done).
//!
//! AgentMeta is identity-only. Soft link via create/park retains meta. Human `done` is
//! only via `DomainState::complete` / human `set_status`.
//! `apply_agent_observation` may set `review`/`blocked` for linked tasks only.

use std::fs;
use std::path::{Path, PathBuf};

use herdr_tasks::attention::ObservationMap;
use herdr_tasks::domain::{
    AgentMeta, AgentSessionIdentity, ContextCapsule, DomainState, HumanStatus, ObservedStatus,
    ProvenanceOrigin, TaskEventKind, TaskScope,
};

fn sample_meta() -> AgentMeta {
    AgentMeta {
        agent_id: Some("agent-abc".into()),
        pane_id: Some("w1:p2".into()),
        agent_session: Some(AgentSessionIdentity {
            source: "herdr:pi".into(),
            value: "session-domain".into(),
        }),
    }
}

#[test]
fn legacy_agent_meta_without_session_deserializes() {
    let meta: AgentMeta = serde_json::from_str(r#"{"agent_id":"agent-abc","pane_id":"w1:p2"}"#)
        .expect("legacy agent metadata");

    assert_eq!(meta.agent_id.as_deref(), Some("agent-abc"));
    assert_eq!(meta.pane_id.as_deref(), Some("w1:p2"));
    assert!(meta.agent_session.is_none());
}

#[test]
fn attention_identity_requires_exact_source_and_value() {
    let first = AgentSessionIdentity {
        source: "herdr:pi".into(),
        value: "same-kind-1".into(),
    };
    let other = AgentSessionIdentity {
        source: "other-host".into(),
        value: "same-kind-1".into(),
    };
    let mut observations = ObservationMap::new();
    observations.insert_session(&first, ObservedStatus::Done);

    assert_eq!(
        observations.lookup(None, Some(&first)),
        Some(ObservedStatus::Done)
    );
    assert_eq!(observations.lookup(None, Some(&other)), None);
}

#[test]
fn create_with_agent_meta_retains_meta() {
    let mut state = DomainState::new();
    let meta = sample_meta();
    let id = state
        .create(
            "From agent pane",
            None,
            TaskScope::Global,
            None,
            Some(meta.clone()),
            ProvenanceOrigin::Capture,
        )
        .expect("create with agent_meta");

    let task = state.get(id).expect("task exists");
    assert_eq!(task.agent_meta.as_ref(), Some(&meta));
    assert_eq!(
        task.agent_meta.as_ref().and_then(|m| m.agent_id.as_deref()),
        Some("agent-abc")
    );
    assert_eq!(
        task.agent_meta.as_ref().and_then(|m| m.pane_id.as_deref()),
        Some("w1:p2")
    );
    // Passive: presence of agent meta does not change human status away from todo.
    assert_eq!(task.status, HumanStatus::Ready);
}

#[test]
fn complete_is_only_via_complete_agent_meta_stays_passive() {
    let mut state = DomainState::new();
    let meta = sample_meta();
    let id = state
        .create(
            "Still open despite agent",
            None,
            TaskScope::Global,
            None,
            Some(meta.clone()),
            ProvenanceOrigin::Capture,
        )
        .expect("create");

    // Agent meta alone never completes the task.
    assert_eq!(state.get(id).expect("task").status, HumanStatus::Ready);

    state.complete(id).expect("complete is the done path");
    let task = state.get(id).expect("task");
    assert_eq!(task.status, HumanStatus::Done);
    // Meta remains data after human completion; not consumed or cleared by complete.
    assert_eq!(task.agent_meta.as_ref(), Some(&meta));
}

#[test]
fn park_stores_agent_meta_passively_without_status_change() {
    // / /: park refreshes meta; human status and agent signal stay decoupled.
    let mut state = DomainState::new();
    let id = state
        .create(
            "Parked work",
            None,
            TaskScope::Global,
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create");
    state
        .set_status(id, HumanStatus::Blocked)
        .expect("set_status");

    let meta = sample_meta();
    state
        .park(
            id,
            Some(ContextCapsule {
                source_pane_id: Some("w1:p2".into()),
                ..ContextCapsule::default()
            }),
            Some(meta.clone()),
        )
        .expect("park");

    let task = state.get(id).expect("task");
    assert_eq!(task.status, HumanStatus::Blocked);
    assert_eq!(task.agent_meta.as_ref(), Some(&meta));
    assert!(
        task.history.iter().any(|e| e.kind == TaskEventKind::Parked),
        "park appends Parked history event"
    );
}

/// AgentMeta stays identity-only; auto-done from observation is forbidden.
///
/// allows `apply_agent_observation` → review/blocked for linked tasks. Scans domain
/// sources so AgentMeta never grows a status field and no helper returns Done from
/// observation alone.
#[test]
fn domain_agent_meta_identity_only_and_observation_never_auto_done() {
    let domain_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/domain");
    let sources = collect_rs_sources(&domain_dir);
    assert!(
        !sources.is_empty(),
        "expected domain .rs files under {}",
        domain_dir.display()
    );

    let mut combined = String::new();
    for (_path, text) in &sources {
        combined.push_str(text);
        combined.push('\n');
    }

    // AgentMeta must remain identity-only: no status-like field on the struct body.
    let agent_meta_def = extract_struct_body(&combined, "AgentMeta")
        .expect("AgentMeta struct must exist in domain sources");
    assert!(
        !agent_meta_def.contains("status"),
        "AgentMeta must not declare a status field; fields are identity only: {agent_meta_def}"
    );

    // Runtime: observation path never yields Done.
    let mut state = DomainState::new();
    let id = state
        .create(
            "obs",
            None,
            TaskScope::Global,
            None,
            Some(AgentMeta {
                agent_id: Some("a".into()),
                pane_id: None,
                agent_session: None,
            }),
            ProvenanceOrigin::Capture,
        )
        .expect("create linked");
    for observed in [
        ObservedStatus::Working,
        ObservedStatus::Blocked,
        ObservedStatus::Idle,
        ObservedStatus::Done,
        ObservedStatus::Unknown,
    ] {
        state.apply_agent_observation(id, observed).expect("apply");
        assert_ne!(
            state.get(id).expect("t").status,
            HumanStatus::Done,
            "observation {observed:?} must not set human done"
        );
    }
    // Only complete sets done.
    state.complete(id).expect("complete");
    assert_eq!(state.get(id).expect("t").status, HumanStatus::Done);
}

fn collect_rs_sources(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).unwrap_or_else(|e| {
        panic!("read_dir {}: {e}", dir.display());
    });
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_rs_sources(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let text = fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!("read {}: {e}", path.display());
            });
            out.push((path, text));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Best-effort: body between `{` after `struct Name` and its matching `}`.
fn extract_struct_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("struct {name}");
    let start = src.find(&marker)?;
    let after = &src[start..];
    let brace = after.find('{')?;
    let body_start = start + brace + 1;
    let mut depth = 1usize;
    let bytes = src.as_bytes();
    let mut i = body_start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[body_start..i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
