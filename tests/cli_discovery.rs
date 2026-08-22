#[test]
fn skill_documents_retry_and_misfiling() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let skill = std::fs::read_to_string(root.join("skills/herdr-tasks-cli/SKILL.md"))
        .expect("CLI skill should exist");
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md should exist");

    for term in ["failed", "exit 1", "never whole-plan-retry", "exit 3"] {
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
    assert!(agents.contains("herdr-tasks add"));
    assert!(agents.contains("herdr-tasks list"));
}
