//! Process command-line routing and headless command execution.

use std::io::Read;

pub mod add;
pub mod parser;
pub mod presenter;
pub mod router;

/// Captured process output, used by the binary and headless integration tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: u8,
}

/// Run the `add` command without reading stdin.
///
/// T-4 extends this seam with JSON plan input. Flag add deliberately ignores even a piped stdin.
pub fn run_with<S, I, R>(args: I, _stdin: R, stdin_is_tty: bool) -> CliOutput
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
    R: Read,
{
    let args = args
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect::<Vec<_>>();
    let input = match parser::parse_flag_add(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::usage(&reason),
    };
    if stdin_is_tty && !input.has_item_flags {
        return presenter::usage("title is required");
    }
    if input.title.is_none() {
        return presenter::usage("title is required");
    }

    match add::run(input) {
        Ok(title) => presenter::added(&title),
        Err(error) => presenter::rejected(error),
    }
}
