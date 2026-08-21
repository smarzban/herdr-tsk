//! Text output for headless command results.

use super::CliOutput;
use crate::cli::add::AddError;

pub fn added(title: &str) -> CliOutput {
    CliOutput {
        stdout: format!("added {title}\n"),
        stderr: String::new(),
        code: 0,
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

pub fn rejected(error: AddError) -> CliOutput {
    let detail = match error {
        AddError::Store(detail) => detail,
        other => other.code().into(),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("herdr-tasks add: {detail}\n"),
        code: 1,
    }
}
