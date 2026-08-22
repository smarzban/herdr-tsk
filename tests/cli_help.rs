use std::io::Cursor;

use herdr_tasks::cli::run_with;

#[test]
fn add_help_documents_plan_shape_and_exit_contract() {
    let add = run_with(
        ["herdr-tasks", "add", "--help"],
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
    ] {
        assert!(
            add.stdout.contains(term),
            "add help should contain {term:?}"
        );
    }

    let list = run_with(
        ["herdr-tasks", "list", "--help"],
        Cursor::new(Vec::<u8>::new()),
        true,
    );

    assert_eq!(list.code, 0);
    assert!(list.stderr.is_empty());
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
    ] {
        assert!(
            list.stdout.contains(term),
            "list help should contain {term:?}"
        );
    }
}
