//! Text output for headless command results.

use super::CliOutput;
use crate::cli::add::{AddError, FlagAddResult};
use crate::cli::list::{ListError, ListResult, ListView};
use crate::domain::HumanStatus;

pub fn add_help() -> CliOutput {
    help_output(
        "usage: herdr-tasks add -t <title> [-n <notes>] [-p <project> | --global] [--state-dir <dir>]\n       herdr-tasks add [--file <path|->] [--state-dir <dir>]",
    )
}

pub fn list_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: herdr-tasks list [-p <project> | --global | --all] [--done | --deleted] [--json] [--state-dir <dir>]\n\n",
            "Lists ready, started, blocked, and review tasks in the invocation project by default, or global scope outside a repository.\n",
            "--project uses the same basename-or-path scope resolution as add; --global selects global tasks; --all selects every scope.\n",
            "--done lists done tasks only. --deleted lists soft-deleted tasks only, regardless of status.\n",
            "To recover a typo scope, use herdr-tasks list --all --json.\n",
            "--json emits a flat array of id, title, status, and project in displayed group order.\n\n",
            "Exit contract:\n",
            "  exit 0: tasks were listed\n",
            "  exit 2: usage or parse error, nothing persisted\n",
            "  exit 3: store I/O failure, no tasks listed\n"
        )
        .into(),
        stderr: String::new(),
        code: 0,
    }
}

fn help_output(usage: &str) -> CliOutput {
    CliOutput {
        stdout: format!(
            "{usage}\n\nAn add whose trimmed title and resolved project scope already exist succeeds without changing the task.\nPlan JSON: [{{\"title\": \"...\", \"notes\": \"...\", \"project\": \"...\"}}]\nPlan result: {{\"created\": [...], \"existing\": [...], \"failed\": [...]}}\n\nExit contract:\n  exit 0: every item was created or already existed\n  exit 1: one or more items were refused, retry failed only\n  exit 2: usage or parse error, nothing persisted\n  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn added(result: FlagAddResult) -> CliOutput {
    let stdout = match result {
        FlagAddResult::Created(title) => format!("added {title}\n"),
        FlagAddResult::Existing => "task already exists\n".into(),
    };
    CliOutput {
        stdout,
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
        list_human(&result)
    };
    CliOutput {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}

fn list_human(result: &ListResult) -> String {
    let groups: &[(Option<HumanStatus>, &str)] = match result.view {
        ListView::Open => &[
            (Some(HumanStatus::Started), "STARTED"),
            (Some(HumanStatus::Ready), "READY"),
            (Some(HumanStatus::Blocked), "BLOCKED"),
            (Some(HumanStatus::Review), "REVIEW"),
        ],
        ListView::Done => &[(None, "DONE")],
        ListView::Deleted => &[(None, "DELETED")],
    };
    let mut output = String::new();
    for (status, heading) in groups {
        let rows = result
            .rows
            .iter()
            .filter(|row| status.is_none_or(|status| row.status == status));
        let mut rows = rows.peekable();
        if rows.peek().is_none() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(heading);
        output.push('\n');
        for row in rows {
            output.push_str(" - ");
            output.push_str(&row.title);
            output.push('\n');
        }
    }
    output
}

pub fn list_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "herdr-tasks list: {reason}\nusage: herdr-tasks list [-p <project> | --global | --all] [--done | --deleted] [--json] [--state-dir <dir>]\n"
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
