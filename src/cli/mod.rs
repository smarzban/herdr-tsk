//! Process command-line routing and headless command execution.

use std::fs;
use std::io::Read;

use serde_json::Value;

pub mod add;
pub mod archive;
pub mod list;
pub mod parser;
pub mod presenter;
pub mod router;
pub mod steps;
pub mod trash;

/// Captured process output, used by the binary and headless integration tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: u8,
}

/// Run a selected headless command. `stdin` is consumed only by plan-form `add`.
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
    match args.get(1).map(String::as_str) {
        Some("add") => run_add(args, &mut stdin, stdin_is_tty),
        Some("steps") => run_steps(args),
        Some("list") => run_list(args),
        Some("trash") => run_trash(args),
        verb @ (Some("archive") | Some("unarchive")) => {
            let (verb, archive) = if verb == Some("archive") {
                ("archive", true)
            } else {
                ("unarchive", false)
            };
            run_archive(args, verb, archive)
        }
        _ => presenter::usage(
            "expected add, steps, list, trash, archive, unarchive, or project command",
        ),
    }
}

fn run_steps(args: Vec<String>) -> CliOutput {
    let input = match parser::parse_flag_steps(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::steps_usage(&reason),
    };
    if input.help {
        return presenter::steps_help();
    }
    let (Some(task), Some(action)) = (input.task, input.action) else {
        return presenter::steps_usage("task id and action are required");
    };
    match steps::run(task, action, input.state_dir) {
        Ok(result) => presenter::steps(result),
        Err(error) => presenter::steps_rejected(error),
    }
}

fn run_archive(args: Vec<String>, verb: &'static str, archive: bool) -> CliOutput {
    let input = match parser::parse_flag_archive(&args, verb) {
        Ok(input) => input,
        Err(reason) => return presenter::archive_usage(verb, &reason),
    };
    if input.help {
        return presenter::archive_help(verb);
    }
    let Some(task) = input.task else {
        return presenter::archive_usage(verb, "task number is required");
    };
    match archive::run_task(task, archive, input.state_dir) {
        Ok(result) => presenter::archived(result, verb),
        Err(error) => presenter::archive_rejected(error),
    }
}

fn run_list(args: Vec<String>) -> CliOutput {
    let input = match list::parse(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::list_usage(&reason),
    };
    if input.help {
        return presenter::list_help();
    }
    let json = input.json;
    match list::run(input) {
        Ok(result) => presenter::list(result, json),
        Err(error) => presenter::list_rejected(error),
    }
}

fn run_trash(args: Vec<String>) -> CliOutput {
    let input = match parser::parse_flag_trash(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::trash_usage(&reason),
    };
    if input.help {
        return presenter::trash_help();
    }
    let Some(action) = input.action else {
        return presenter::trash_usage("trash action is required");
    };
    match action {
        parser::TrashAction::Restore { target } => {
            match trash::run_restore(target, input.state_dir) {
                Ok(result) => presenter::trash_restored(result),
                Err(error) => presenter::trash_rejected(error),
            }
        }
    }
}

fn run_add<R: Read>(args: Vec<String>, stdin: &mut R, stdin_is_tty: bool) -> CliOutput {
    let input = match parser::parse_flag_add(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::usage(&reason),
    };

    if input.help {
        return presenter::add_help();
    }

    if input.has_item_flags {
        if input.title.is_none() {
            return presenter::usage("title is required");
        }
        let json = input.json;
        return match add::run(input) {
            Ok(result) => presenter::added(result, json),
            Err(error) => presenter::rejected(error),
        };
    }

    if input.file.is_none() && stdin_is_tty {
        return presenter::usage("title is required");
    }

    let source = match read_plan_source(&input, stdin) {
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
