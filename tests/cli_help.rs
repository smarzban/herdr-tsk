use std::io::Cursor;
use std::process::Command;

use tsk_tui::cli::run_with;

fn help(args: &[&str]) -> tsk_tui::cli::CliOutput {
    run_with(args, Cursor::new(Vec::<u8>::new()), true)
}

fn assert_sections_in_order(output: &str, headings: &[&str]) {
    let mut previous = 0;
    for heading in headings {
        let position = output
            .find(heading)
            .unwrap_or_else(|| panic!("help should contain {heading:?}"));
        assert!(
            position >= previous,
            "{heading:?} should follow the prior section"
        );
        previous = position;
    }
    assert!(
        output.contains("Exit:"),
        "help should contain an exit section"
    );
}

macro_rules! verb_help_test {
    ($name:ident, [$($arg:expr),+], [$($heading:expr),+]) => {
        #[test]
        fn $name() {
            let output = help(&["tsk", $($arg),+]);
            assert_eq!(output.code, 0);
            assert!(output.stderr.is_empty());
            assert_sections_in_order(&output.stdout, &[$($heading),+]);
        }
    };
}

verb_help_test!(
    add_help_has_reference_sections,
    ["add", "--help"],
    [
        "usage:",
        "Scope",
        "Output",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    list_help_has_reference_sections,
    ["list", "--help"],
    [
        "usage:",
        "Scope",
        "Filters",
        "Output",
        "Values",
        "Examples:",
        "Exit:"
    ]
);
verb_help_test!(
    status_help_has_reference_sections,
    ["status", "--help"],
    [
        "usage:",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    edit_help_has_reference_sections,
    ["edit", "--help"],
    [
        "usage:",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    steps_help_has_reference_sections,
    ["steps", "--help"],
    [
        "usage:",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    trash_help_has_reference_sections,
    ["trash", "--help"],
    [
        "usage:",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    archive_help_has_reference_sections,
    ["archive", "--help"],
    [
        "usage:",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    unarchive_help_has_reference_sections,
    ["unarchive", "--help"],
    [
        "usage:",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    project_help_has_reference_sections,
    ["project", "--help"],
    [
        "usage:",
        "Scope",
        "Values",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    setup_help_has_reference_sections,
    ["setup", "--help"],
    [
        "usage:",
        "Output",
        "Examples:",
        "Refusals (exit 1):",
        "Exit:"
    ]
);
verb_help_test!(
    update_help_has_reference_sections,
    ["update", "--help"],
    ["usage:", "Examples:", "Refusals (exit 1):", "Exit:"]
);
verb_help_test!(
    guide_help_has_reference_sections,
    ["guide", "--help"],
    ["usage:", "Examples:", "Exit:"]
);

#[test]
fn help_without_an_operand_is_top_level_help() {
    let output = help(&["tsk", "help"]);
    assert_eq!(output.code, 0);
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, tsk_tui::cli::presenter::top_level_help());
}

#[test]
fn help_operand_matches_verb_help_byte_for_byte() {
    let via_help = help(&["tsk", "help", "add"]);
    let direct = help(&["tsk", "add", "--help"]);
    assert_eq!(via_help.code, 0);
    assert_eq!(via_help, direct);
}

#[test]
fn unknown_help_operand_is_a_usage_error() {
    let output = help(&["tsk", "help", "nope"]);
    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        "tsk help: unknown command nope\nusage: tsk help [<command>]\n"
    );
}

#[test]
fn add_version_is_not_a_global_flag_after_the_verb() {
    let output = help(&["tsk", "add", "--version"]);
    assert_eq!(output.code, 2);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.starts_with("tsk add:"));
}

#[test]
fn version_flags_print_the_package_version() {
    for flag in ["--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tsk"))
            .arg(flag)
            .output()
            .expect("run version flag");
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("UTF-8 version"),
            format!("tsk {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}
