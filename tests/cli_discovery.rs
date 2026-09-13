#[test]
fn skill_documents_retry_and_misfiling() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let skill = std::fs::read_to_string(root.join("skills/tsk-cli/SKILL.md"))
        .expect("CLI skill should exist");
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md should exist");

    for term in [
        "tsk add -t",
        "tsk status",
        "tsk edit",
        "--desk",
        "-p <project>",
        "tsk help <command>",
        "cat plan.json | tsk add",
        "| 0 |",
        "| 1 |",
        "| 2 |",
        "| 3 |",
        "retry only",
        "list --all --json",
        "Human status is the user's",
        "## Refine a task",
        "propose, then write",
    ] {
        assert!(skill.contains(term), "skill should contain {term:?}");
    }
    assert!(
        skill.contains("indeterminate") || skill.contains("list"),
        "skill should describe indeterminate exit-3 recovery"
    );
    assert!(
        skill.contains("typo") && skill.contains("scope"),
        "skill should warn that a typo can create a new scope"
    );
    assert!(agents.contains("tsk add"));
    assert!(agents.contains("tsk list"));
}
