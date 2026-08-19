//! Resume Navigator: pure decision order pane → path → fail.
//!
//! No host I/O in [`decide_resume`]. Never invents targets; never creates worktrees
//! or starts agents. Host execution: [`execute_resume`].

use crate::domain::ContextCapsule;
use crate::host::HostPorts;
use crate::text::non_empty;

/// Outcome of resume attempt ordering (decision only; host execute is separate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeDecision {
    /// Capsule source pane still exists: focus it.
    FocusPane { pane_id: String },
    /// Pane missing/absent; open or focus a usable worktree/workspace path.
    OpenPath { path: String },
    /// No usable resume target (or empty capsule).
    Failed { message: String },
}

/// Pure resume decision from a capsule and host probes.
///
/// Order (AC resume attempt order):
/// 1. source pane id present and `pane_exists` → [`ResumeDecision::FocusPane`]
/// 2. else first usable worktree/workspace path → [`ResumeDecision::OpenPath`]
/// 3. else [`ResumeDecision::Failed`] with a non-empty message
///
/// Path candidates (first usable wins): `worktree_path`, then `repo_path`, then `cwd`.
/// Does not invent pane ids or paths. Has no CreateWorktree / StartAgent variants.
pub fn decide_resume(
    capsule: Option<&ContextCapsule>,
    mut pane_exists: impl FnMut(&str) -> bool,
    mut path_usable: impl FnMut(&str) -> bool,
) -> ResumeDecision {
    let Some(capsule) = capsule else {
        return ResumeDecision::Failed {
            message: "no context capsule to resume".into(),
        };
    };

    if let Some(pane_id) = non_empty(capsule.source_pane_id.as_deref()) {
        if pane_exists(pane_id) {
            return ResumeDecision::FocusPane {
                pane_id: pane_id.to_string(),
            };
        }
    }

    path_decision(capsule, &mut path_usable)
}

/// First usable path among worktree → repo → cwd, or Failed.
fn path_decision(
    capsule: &ContextCapsule,
    path_usable: &mut impl FnMut(&str) -> bool,
) -> ResumeDecision {
    for candidate in [
        capsule.worktree_path.as_deref(),
        capsule.repo_path.as_deref(),
        capsule.cwd.as_deref(),
    ] {
        if let Some(path) = non_empty(candidate) {
            if path_usable(path) {
                return ResumeDecision::OpenPath {
                    path: path.to_string(),
                };
            }
        }
    }

    ResumeDecision::Failed {
        message: "no usable resume target (pane missing and no openable path)".into(),
    }
}

/// Decide resume order using host pane list / path probes, then execute focus or open.
///
/// On focus failure with a usable capsule path, falls back to OpenPath.
/// On other host I/O failure, returns [`ResumeDecision::Failed`] with a non-empty message.
/// Does not mutate task domain state.
pub fn execute_resume(
    capsule: Option<&ContextCapsule>,
    host: &(impl HostPorts + ?Sized),
) -> ResumeDecision {
    let pane_ids = match host.list_pane_ids() {
        Ok(ids) => ids,
        Err(e) => {
            return ResumeDecision::Failed {
                message: format!("could not list panes: {e}"),
            };
        }
    };
    let decision = decide_resume(
        capsule,
        |id| pane_ids.iter().any(|p| p == id),
        |path| host.path_usable(path),
    );
    run_decision(decision, host, capsule)
}

/// Execute a pure [`ResumeDecision`] against host ports.
///
/// When FocusPane fails and the capsule has a usable path, opens that path instead.
pub fn run_decision(
    decision: ResumeDecision,
    host: &(impl HostPorts + ?Sized),
    capsule: Option<&ContextCapsule>,
) -> ResumeDecision {
    match decision {
        ResumeDecision::FocusPane { pane_id } => match host.focus_pane(&pane_id) {
            Ok(()) => ResumeDecision::FocusPane { pane_id },
            Err(focus_err) => {
                // pane listed but not focusable → path fallback when usable.
                if let Some(c) = capsule {
                    let path_try = path_decision(c, &mut |path| host.path_usable(path));
                    if matches!(path_try, ResumeDecision::OpenPath { .. }) {
                        return run_decision(path_try, host, None);
                    }
                }
                ResumeDecision::Failed {
                    message: format!("failed to focus pane {pane_id}: {focus_err}"),
                }
            }
        },
        ResumeDecision::OpenPath { path } => match host.open_path(&path) {
            Ok(()) => ResumeDecision::OpenPath { path },
            Err(e) => ResumeDecision::Failed {
                message: format!("failed to open path {path}: {e}"),
            },
        },
        failed @ ResumeDecision::Failed { .. } => failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capsule(pane: Option<&str>, worktree: Option<&str>, repo: Option<&str>) -> ContextCapsule {
        ContextCapsule {
            source_pane_id: pane.map(str::to_string),
            worktree_path: worktree.map(str::to_string),
            repo_path: repo.map(str::to_string),
            ..ContextCapsule::default()
        }
    }

    #[test]
    fn pane_present_focuses_pane_even_when_paths_exist() {
        let c = capsule(Some("w0:p1"), Some("/wt"), Some("/repo"));
        let decision = decide_resume(Some(&c), |_| true, |_| true);
        assert_eq!(
            decision,
            ResumeDecision::FocusPane {
                pane_id: "w0:p1".into()
            }
        );
    }

    #[test]
    fn pane_missing_with_usable_worktree_opens_path() {
        let c = capsule(Some("gone"), Some("/wt/path"), Some("/repo"));
        let decision = decide_resume(Some(&c), |id| id != "gone", |p| p == "/wt/path");
        assert_eq!(
            decision,
            ResumeDecision::OpenPath {
                path: "/wt/path".into()
            }
        );
    }

    #[test]
    fn pane_absent_uses_repo_path_when_worktree_unusable() {
        let c = capsule(None, Some("/missing-wt"), Some("/repo"));
        let decision = decide_resume(Some(&c), |_| false, |p| p == "/repo");
        assert_eq!(
            decision,
            ResumeDecision::OpenPath {
                path: "/repo".into()
            }
        );
    }

    #[test]
    fn pane_absent_uses_cwd_when_worktree_and_repo_unusable() {
        let c = ContextCapsule {
            source_pane_id: None,
            worktree_path: Some("/missing-wt".into()),
            repo_path: Some("/missing-repo".into()),
            cwd: Some("/work/cwd".into()),
            ..ContextCapsule::default()
        };
        let decision = decide_resume(Some(&c), |_| false, |p| p == "/work/cwd");
        assert_eq!(
            decision,
            ResumeDecision::OpenPath {
                path: "/work/cwd".into()
            }
        );
    }

    #[test]
    fn neither_pane_nor_path_fails_with_message() {
        let c = capsule(Some("gone"), Some("/bad"), None);
        let decision = decide_resume(Some(&c), |_| false, |_| false);
        match decision {
            ResumeDecision::Failed { message } => {
                assert!(
                    !message.trim().is_empty(),
                    "failure message must be non-empty"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn empty_capsule_fails() {
        let decision = decide_resume(None, |_| true, |_| true);
        match decision {
            ResumeDecision::Failed { message } => assert!(!message.is_empty()),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn decide_resume_table_covers_order() {
        // (pane_id, pane_ok, worktree, repo, path_ok_pred label, expected kind)
        struct Case {
            pane: Option<&'static str>,
            pane_ok: bool,
            worktree: Option<&'static str>,
            repo: Option<&'static str>,
            usable_paths: &'static [&'static str],
            expect: ResumeDecision,
        }
        let cases = [
            Case {
                pane: Some("p1"),
                pane_ok: true,
                worktree: Some("/wt"),
                repo: Some("/r"),
                usable_paths: &["/wt", "/r"],
                expect: ResumeDecision::FocusPane {
                    pane_id: "p1".into(),
                },
            },
            Case {
                pane: Some("p1"),
                pane_ok: false,
                worktree: Some("/wt"),
                repo: Some("/r"),
                usable_paths: &["/wt"],
                expect: ResumeDecision::OpenPath { path: "/wt".into() },
            },
            Case {
                pane: None,
                pane_ok: false,
                worktree: None,
                repo: Some("/r"),
                usable_paths: &["/r"],
                expect: ResumeDecision::OpenPath { path: "/r".into() },
            },
            Case {
                pane: None,
                pane_ok: false,
                worktree: None,
                repo: None,
                usable_paths: &[],
                expect: ResumeDecision::Failed {
                    message: "no usable resume target (pane missing and no openable path)".into(),
                },
            },
        ];

        for case in cases {
            let c = capsule(case.pane, case.worktree, case.repo);
            let decision = decide_resume(
                Some(&c),
                |id| case.pane_ok && case.pane == Some(id),
                |p| case.usable_paths.contains(&p),
            );
            assert_eq!(decision, case.expect, "case pane={:?}", case.pane);
        }

        // cwd is a path candidate after worktree and repo.
        let with_cwd = ContextCapsule {
            cwd: Some("/only-cwd".into()),
            ..ContextCapsule::default()
        };
        assert_eq!(
            decide_resume(Some(&with_cwd), |_| false, |p| p == "/only-cwd"),
            ResumeDecision::OpenPath {
                path: "/only-cwd".into()
            }
        );
    }

    #[test]
    fn resume_decision_has_no_create_worktree_or_start_agent() {
        // Compile-time / exhaustiveness lock: only FocusPane | OpenPath | Failed.
        // Match arms must cover all variants; adding Create/Start would break this test shape.
        let samples = [
            ResumeDecision::FocusPane {
                pane_id: "x".into(),
            },
            ResumeDecision::OpenPath { path: "/p".into() },
            ResumeDecision::Failed {
                message: "m".into(),
            },
        ];
        for d in samples {
            match d {
                ResumeDecision::FocusPane { .. }
                | ResumeDecision::OpenPath { .. }
                | ResumeDecision::Failed { .. } => {}
            }
        }
    }

    use std::sync::{Arc, Mutex};

    struct RecordHost {
        panes: Vec<String>,
        usable: Vec<String>,
        calls: Arc<Mutex<Vec<String>>>,
        fail_focus: bool,
    }

    impl HostPorts for RecordHost {
        fn list_pane_ids(&self) -> Result<Vec<String>, String> {
            self.calls.lock().unwrap().push("list".into());
            Ok(self.panes.clone())
        }

        fn focus_pane(&self, pane_id: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("focus:{pane_id}"));
            if self.fail_focus {
                return Err("focus failed".into());
            }
            Ok(())
        }

        fn open_path(&self, path: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("open:{path}"));
            Ok(())
        }

        fn path_usable(&self, path: &str) -> bool {
            self.usable.iter().any(|p| p == path)
        }
    }

    #[test]
    fn execute_resume_focuses_when_pane_exists() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let host = RecordHost {
            panes: vec!["w0:p1".into()],
            usable: vec!["/wt".into()],
            calls: Arc::clone(&calls),
            fail_focus: false,
        };
        let c = capsule(Some("w0:p1"), Some("/wt"), None);
        let out = execute_resume(Some(&c), &host);
        assert_eq!(
            out,
            ResumeDecision::FocusPane {
                pane_id: "w0:p1".into()
            }
        );
        let log = calls.lock().unwrap().clone();
        assert!(log.iter().any(|c| c == "list"));
        assert!(log.iter().any(|c| c == "focus:w0:p1"));
        assert!(!log.iter().any(|c| c.starts_with("open:")));
    }

    #[test]
    fn execute_resume_opens_path_when_pane_gone() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let host = RecordHost {
            panes: vec![],
            usable: vec!["/wt".into()],
            calls: Arc::clone(&calls),
            fail_focus: false,
        };
        let c = capsule(Some("gone"), Some("/wt"), None);
        let out = execute_resume(Some(&c), &host);
        assert_eq!(out, ResumeDecision::OpenPath { path: "/wt".into() });
        let log = calls.lock().unwrap().clone();
        assert!(log.iter().any(|c| c == "open:/wt"));
        assert!(!log.iter().any(|c| c.starts_with("focus:")));
    }

    #[test]
    fn execute_resume_focus_fail_falls_back_to_open_path() {
        // pane id still listed but focus fails → usable path opens.
        let calls = Arc::new(Mutex::new(Vec::new()));
        let host = RecordHost {
            panes: vec!["w0:p1".into()],
            usable: vec!["/wt".into()],
            calls: Arc::clone(&calls),
            fail_focus: true,
        };
        let c = capsule(Some("w0:p1"), Some("/wt"), None);
        let out = execute_resume(Some(&c), &host);
        assert_eq!(out, ResumeDecision::OpenPath { path: "/wt".into() });
        let log = calls.lock().unwrap().clone();
        assert!(log.iter().any(|c| c == "focus:w0:p1"));
        assert!(log.iter().any(|c| c == "open:/wt"));
    }

    #[test]
    fn execute_resume_focus_fail_without_path_fails() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let host = RecordHost {
            panes: vec!["w0:p1".into()],
            usable: vec![],
            calls: Arc::clone(&calls),
            fail_focus: true,
        };
        let c = capsule(Some("w0:p1"), Some("/missing"), None);
        let out = execute_resume(Some(&c), &host);
        match out {
            ResumeDecision::Failed { message } => {
                assert!(message.contains("focus"), "message={message}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        let log = calls.lock().unwrap().clone();
        assert!(log.iter().any(|c| c == "focus:w0:p1"));
        assert!(!log.iter().any(|c| c.starts_with("open:")));
    }

    #[test]
    fn execute_resume_fails_without_host_open_when_no_target() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let host = RecordHost {
            panes: vec![],
            usable: vec![],
            calls: Arc::clone(&calls),
            fail_focus: false,
        };
        let c = capsule(None, None, None);
        let out = execute_resume(Some(&c), &host);
        match out {
            ResumeDecision::Failed { message } => assert!(!message.is_empty()),
            other => panic!("expected Failed, got {other:?}"),
        }
        let log = calls.lock().unwrap().clone();
        assert!(!log.iter().any(|c| c.starts_with("focus:")));
        assert!(!log.iter().any(|c| c.starts_with("open:")));
    }
}
