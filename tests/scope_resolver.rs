use std::path::PathBuf;

use herdr_tasks::context::InvocationSnapshot;
use herdr_tasks::domain::{DomainState, ProvenanceOrigin, TaskScope};
use herdr_tasks::scope::resolve_project_path;
use herdr_tasks::ui::board::{apply_intent, BoardModel, IntentOutcome};
use herdr_tasks::ui::input::BoardIntent;

fn snapshot() -> InvocationSnapshot {
    InvocationSnapshot {
        default_scope: TaskScope::Project {
            path: "/repos/default".into(),
        },
        this_repo: Some(PathBuf::from("/repos/herdr-tasks")),
        title_prefill: None,
        provenance: ProvenanceOrigin::Capture,
        capsule: None,
        agent_meta: None,
    }
}

fn create_project(domain: &mut DomainState, path: &str) {
    domain
        .create(
            "known project",
            None,
            TaskScope::Project { path: path.into() },
            None,
            None,
            ProvenanceOrigin::Manual,
        )
        .expect("create fixture project");
}

fn board_quick_add_scope(
    domain: &mut DomainState,
    model: &mut BoardModel,
    snapshot: &InvocationSnapshot,
    title: &str,
) -> TaskScope {
    assert_eq!(
        apply_intent(
            domain,
            model,
            BoardIntent::OpenCapture,
            Some(snapshot),
            None,
        )
        .expect("open quick add"),
        IntentOutcome::None
    );
    for character in title.chars() {
        apply_intent(
            domain,
            model,
            BoardIntent::QuickAddInsert(character),
            None,
            None,
        )
        .expect("type quick-add title");
    }
    assert_eq!(
        apply_intent(domain, model, BoardIntent::QuickAddSave, None, None).expect("save quick add"),
        IntentOutcome::Persist
    );
    model.sync_from_domain(domain);
    domain.tasks().last().expect("saved task").scope.clone()
}

#[test]
fn shared_resolver_and_board_quick_add_agree_on_fixtures() {
    let mut domain = DomainState::new();
    create_project(&mut domain, "/repos/other");
    create_project(&mut domain, "/repos/normal");
    create_project(&mut domain, "/repos/ghost");
    let ghost = domain.tasks().last().expect("ghost task").id;
    domain.soft_delete(ghost).expect("soft delete ghost");
    let snapshot = snapshot();
    let mut model = BoardModel::from_domain(&domain, snapshot.this_repo.clone());

    for (token, title, expected) in [
        ("normal", "normal path !p normal", "/repos/normal"),
        ("ghost", "soft deleted path !p ghost", "/repos/ghost"),
        (
            "HERDR-TASKS",
            "snapshot repo !p HERDR-TASKS",
            "/repos/herdr-tasks",
        ),
        ("missing", "missing !p missing", "missing"),
        ("/abs/x", "verbatim !p /abs/x", "/abs/x"),
    ] {
        assert_eq!(
            resolve_project_path(token, &domain, Some(&snapshot)),
            expected,
            "shared resolver fixture {token:?}"
        );
        assert_eq!(
            board_quick_add_scope(&mut domain, &mut model, &snapshot, title),
            TaskScope::Project {
                path: expected.into(),
            },
            "board quick-add fixture {token:?}"
        );
    }

    let apply_source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/ui/board/apply.rs"
    ))
    .expect("read board apply source");
    assert!(
        !apply_source.contains("fn resolve_quick_add_project_path"),
        "board apply must delegate to the shared resolver instead of retaining a private matcher"
    );
}
