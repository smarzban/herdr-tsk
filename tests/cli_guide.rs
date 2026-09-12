use std::io::Cursor;

use tsk_tui::cli::run_with;

fn skill_minus_frontmatter() -> &'static str {
    let source = include_str!("../skills/tsk-cli/SKILL.md");
    let rest = source
        .strip_prefix("---\n")
        .expect("skill starts with YAML frontmatter");
    let close = rest.find("\n---\n").expect("skill has a closing fence");
    let after = &rest[close + "\n---\n".len()..];
    after.strip_prefix('\n').unwrap_or(after)
}

#[test]
fn guide_prints_skill_minus_frontmatter_and_exits_0() {
    let output = run_with(["tsk", "guide"], Cursor::new(Vec::<u8>::new()), true);
    assert_eq!(output.code, 0);
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, skill_minus_frontmatter());
    assert!(
        !output.stdout.starts_with("---"),
        "frontmatter must be stripped"
    );
    assert!(
        output.stdout.starts_with("# "),
        "body must open with the H1"
    );
}
