//! Text output for headless command results.

use super::CliOutput;
use crate::cli::add::AddError;
use crate::cli::list::{ListError, ListResult};

pub fn add_help() -> CliOutput {
    help_output(
        "usage: herdr-tasks add -t <title> [-n <notes>] [-p <project> | --global] [--state-dir <dir>]\n       herdr-tasks add [--file <path|->] [--state-dir <dir>]",
    )
}

pub fn list_help() -> CliOutput {
    help_output("usage: herdr-tasks list [--json] [--state-dir <dir>]")
}

fn help_output(usage: &str) -> CliOutput {
    CliOutput {
        stdout: format!(
            "{usage}\n\nPlan JSON: [{{\"title\": \"...\", \"notes\": \"...\", \"project\": \"...\"}}]\nPlan result: {{\"created\": [...], \"failed\": [...]}}\n\nExit contract:\n  exit 0: every item was created\n  exit 1: one or more items were refused, retry failed only\n  exit 2: usage or parse error, nothing persisted\n  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn added(title: &str) -> CliOutput {
    CliOutput {
        stdout: format!("added {title}\n"),
        stderr: String::new(),
        code: 0,
    }
}

pub fn plan(result: crate::cli::add::PlanResult) -> CliOutput {
    let code = if result.has_failures() { 1 } else { 0 };
    CliOutput {
        stdout: format!(
            "{}\n",
            serde_json::to_string(&result).expect("plan result is serializable")
        ),
        stderr: String::new(),
        code,
    }
}

pub fn usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "herdr-tasks add: {reason}\nusage: herdr-tasks add -t <title> [-n <notes>] [-p <project> | --global] [--state-dir <dir>]\n"
        ),
        code: 2,
    }
}

pub fn list(result: ListResult, json: bool) -> CliOutput {
    let stdout = if json {
        format!(
            "{}\n",
            serde_json::to_string(&result.rows).expect("list rows are serializable")
        )
    } else {
        result
            .rows
            .iter()
            .map(|row| format!("{}\n", row.title))
            .collect()
    };
    CliOutput {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}

pub fn list_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "herdr-tasks list: {reason}\nusage: herdr-tasks list [--json] [--state-dir <dir>]\n"
        ),
        code: 2,
    }
}

pub fn list_rejected(error: ListError) -> CliOutput {
    let ListError::Store(detail) = error;
    CliOutput {
        stdout: String::new(),
        stderr: format!("herdr-tasks list: {detail}\n"),
        code: 3,
    }
}

pub fn rejected(error: AddError) -> CliOutput {
    let (detail, code) = match error {
        AddError::Store(detail) => (detail, 3),
        other => (other.code().into(), 1),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("herdr-tasks add: {detail}\n"),
        code,
    }
}
