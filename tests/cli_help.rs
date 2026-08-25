use std::io::Cursor;

use tsk_tui::cli::run_with;

#[test]
fn add_help_documents_plan_shape_and_exit_contract() {
    let add = run_with(
        ["tsk", "add", "--help"],
        Cursor::new(Vec::<u8>::new()),
        true,
    );

    assert_eq!(add.code, 0);
    assert!(add.stderr.is_empty());
    for term in [
        "title",
        "notes",
        "project",
        "created",
        "existing",
        "already existed",
        "failed",
        "exit 1",
        "retry",
        "exit 2",
        "nothing persisted",
        "exit 3",
        "indeterminate",
        "list",
        "--json",
        "tsk add -t",
        "--desk",
        "--project",
        "--title=<value>",
        "--notes=<value>",
        "--project=<value>",
        "--state-dir=<dir>",
        "--file=<path>",
        "--file plan.json",
        "cat plan.json | tsk add",
    ] {
        assert!(
            add.stdout.contains(term),
            "add help should contain {term:?}"
        );
    }

    let list = run_with(
        ["tsk", "list", "--help"],
        Cursor::new(Vec::<u8>::new()),
        true,
    );

    assert_eq!(list.code, 0);
    assert!(list.stderr.is_empty());
    for term in [
        "project",
        "desk",
        "done",
        "deleted",
        "soft-deleted",
        "status",
        "json",
        "all",
        "state-dir",
        "invocation project",
        "list --all --json",
        "--project=<scope>",
        "--state-dir=<dir>",
        "dash-leading",
        "Exit contract",
        "exit 0",
        "exit 2",
        "exit 3",
    ] {
        assert!(
            list.stdout.contains(term),
            "list help should contain {term:?}"
        );
    }
}
