//! Process command-line routing and headless command execution.

use std::fs;
use std::io::Read;

use serde_json::Value;

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

/// Run the `add` command with its selected flag or JSON-plan input.
pub fn run_with<S, I, R>(args: I, mut stdin: R, stdin_is_tty: bool) -> CliOutput
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

    if input.has_item_flags {
        if input.title.is_none() {
            return presenter::usage("title is required");
        }
        return match add::run(input) {
            Ok(title) => presenter::added(&title),
            Err(error) => presenter::rejected(error),
        };
    }

    if input.file.is_none() && stdin_is_tty {
        return presenter::usage("title is required");
    }

    let source = match read_plan_source(&input, &mut stdin) {
        Ok(source) => source,
        Err(reason) => return presenter::usage(&reason),
    };
    let values = match parse_plan(&source) {
        Ok(values) => values,
        Err(reason) => return presenter::usage(&reason),
    };
    match add::run_plan(values, input.state_dir) {
        Ok(result) => presenter::plan(result),
        Err(error) => presenter::rejected(error),
    }
}

fn read_plan_source<R: Read>(input: &parser::FlagAdd, stdin: &mut R) -> Result<String, String> {
    match input.file.as_deref() {
        Some(path) if path == std::path::Path::new("-") => {
            let mut source = String::new();
            stdin
                .read_to_string(&mut source)
                .map_err(|error| format!("could not read plan stdin: {error}"))?;
            Ok(source)
        }
        Some(path) => fs::read_to_string(path)
            .map_err(|error| format!("could not read plan file {}: {error}", path.display())),
        None => {
            let mut source = String::new();
            stdin
                .read_to_string(&mut source)
                .map_err(|error| format!("could not read plan stdin: {error}"))?;
            Ok(source)
        }
    }
}

fn parse_plan(source: &str) -> Result<Vec<Value>, String> {
    let value: Value =
        serde_json::from_str(source).map_err(|error| format!("invalid JSON plan: {error}"))?;
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "JSON plan must be an array".into())
}
