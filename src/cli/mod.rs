//! Process command-line routing and headless command execution.

use std::fs;
use std::io::{IsTerminal, Read};

use serde_json::Value;

pub mod add;
pub mod archive;
pub mod edit;
pub mod list;
pub mod parser;
pub mod presenter;
pub mod router;
pub mod status;
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
        Some("setup") => run_setup(args, &mut stdin, stdin_is_tty),
        Some("add") => run_add(args, &mut stdin, stdin_is_tty),
        Some("steps") => run_steps(args),
        Some("list") => run_list(args),
        Some("status") => run_status(args),
        Some("edit") => run_edit(args),
        Some("trash") => run_trash(args),
        Some("project") => run_project(args),
        verb @ (Some("archive") | Some("unarchive")) => {
            let (verb, archive) = if verb == Some("archive") {
                ("archive", true)
            } else {
                ("unarchive", false)
            };
            run_archive(args, verb, archive)
        }
        _ => presenter::usage(
            "expected add, steps, list, status, edit, trash, archive, unarchive, or project command",
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

fn run_status(args: Vec<String>) -> CliOutput {
    let input = match parser::parse_flag_status(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::status_usage(&reason),
    };
    if input.help {
        return presenter::status_help();
    }
    let (Some(task), Some(status)) = (input.task, input.status) else {
        return presenter::status_usage(if input.task.is_none() {
            "task number is required"
        } else {
            "status is required"
        });
    };
    match status::run(task, status, input.state_dir) {
        Ok(result) => presenter::status(result),
        Err(error) => presenter::status_rejected(error),
    }
}

fn run_edit(args: Vec<String>) -> CliOutput {
    let input = match parser::parse_flag_edit(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::edit_usage(&reason),
    };
    if input.help {
        return presenter::edit_help();
    }
    let Some(task) = input.task else {
        return presenter::edit_usage("task number is required");
    };
    if input.title.is_none() && input.notes.is_none() {
        return presenter::edit_usage("title or notes is required");
    }
    match edit::run(
        task,
        edit::EditFields {
            title: input.title,
            notes: input.notes,
        },
        input.state_dir,
    ) {
        Ok(result) => presenter::edited(result),
        Err(error) => presenter::edit_rejected(error),
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
        Err(error) => presenter::archive_rejected(error, verb),
    }
}

fn run_project(args: Vec<String>) -> CliOutput {
    let input = match parser::parse_flag_project(&args) {
        Ok(input) => input,
        Err(reason) => return presenter::project_usage(&reason),
    };
    if input.help {
        return presenter::project_help();
    }
    let Some(action) = input.action else {
        return presenter::project_usage("project action is required");
    };
    let (verb, name, archive) = match action {
        parser::ProjectAction::Archive { name } => ("archive", name, true),
        parser::ProjectAction::Unarchive { name } => ("unarchive", name, false),
    };
    match archive::run_project(name, archive, input.state_dir) {
        Ok(result) => presenter::project_archived(result, verb),
        Err(error) => presenter::archive_rejected(error, verb),
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

fn run_setup<R: Read>(args: Vec<String>, stdin: &mut R, stdin_is_tty: bool) -> CliOutput {
    let tail: Vec<_> = args.iter().skip(2).map(String::as_str).collect();
    match tail.as_slice() {
        ["--help"] | ["herdr", "--help"] => presenter::setup_help(),
        ["herdr"] => {
            let mut reader = std::io::BufReader::new(stdin);
            let mut stderr = std::io::stderr();
            let interactive = stdin_is_tty && stderr.is_terminal();
            match crate::setup::run(&mut reader, &mut stderr, interactive) {
                Ok(result) => presenter::setup(result),
                Err(error) => presenter::setup_error(&error.to_string(), 1),
            }
        }
        _ => presenter::setup_error("usage: tsk setup herdr", 2),
    }
}
