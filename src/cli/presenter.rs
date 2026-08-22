//! Text output for headless command results.

use std::path::Path;

use super::CliOutput;
use crate::cli::add::{AddError, FlagAddResult};
use crate::cli::list::{ListError, ListResult, ListRow, ListView};
use crate::domain::HumanStatus;

pub fn add_help() -> CliOutput {
    help_output(
        "usage: herdr-tasks add -t <title> [-n <notes>] [-p <project> | --global] [--json] [--state-dir <dir>]\n       herdr-tasks add [--file <path|->] [--state-dir <dir>]",
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
            "--json emits a flat array of id, title, status, and project in displayed group order. Human --all groups rows by status, then project scope (basename when unique, full path when needed) or global.\n\n",
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
            "{usage}\n\nExamples:\n  herdr-tasks add -t \"Draft release notes\"\n  herdr-tasks add -t \"Buy milk\" --global\n  herdr-tasks add -t \"Fix widget\" --project widget\n  herdr-tasks add --title=\"-fix parser\" --notes=\"-5 degrees\" --project=\"-maintenance\"\n  herdr-tasks add --file plan.json\n  cat plan.json | herdr-tasks add\n\nValues beginning with - must use --title=<value>, --notes=<value>, or --project=<value>.\nAn add whose trimmed title and resolved project scope already exist succeeds without changing the task. With --json, flag add emits one object with outcome, id, title, and project (or null).\nPlan JSON: [{{\"title\": \"...\", \"notes\": \"...\", \"project\": \"...\"}}]\nPlan result: {{\"created\": [...], \"existing\": [...], \"failed\": [...]}}\n\nExit contract:\n  exit 0: every item was created or already existed\n  exit 1: one or more items were refused, retry failed only\n  exit 2: usage or parse error, nothing persisted\n  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn added(result: FlagAddResult, json: bool) -> CliOutput {
    let stdout = match result {
        FlagAddResult::Created { id, title, project } if json => format!(
            "{}\n",
            serde_json::json!({
                "outcome": "created",
                "id": id,
                "title": title,
                "project": project,
            })
        ),
        FlagAddResult::Existing { id, title, project } if json => format!(
            "{}\n",
            serde_json::json!({
                "outcome": "existing",
                "id": id,
                "title": title,
                "project": project,
            })
        ),
        FlagAddResult::Created { title, .. } => format!("added {title}\n"),
        FlagAddResult::Existing { .. } => "task already exists\n".into(),
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
            "herdr-tasks add: {reason}\nusage: herdr-tasks add -t <title> [-n <notes>] [-p <project> | --global] [--json] [--state-dir <dir>]\n"
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
            .filter(|row| status.is_none_or(|status| row.status == status))
            .collect::<Vec<_>>();
        if rows.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(heading);
        output.push('\n');
        if result.include_scope {
            append_scope_groups(&mut output, rows);
        } else {
            append_rows(&mut output, &rows, " ");
        }
    }
    output
}

fn append_scope_groups(output: &mut String, rows: Vec<&ListRow>) {
    let mut scopes = Vec::<(Option<&str>, Vec<&ListRow>)>::new();
    for row in rows {
        let scope = row.project.as_deref();
        match scopes
            .iter_mut()
            .find(|(group_scope, _)| *group_scope == scope)
        {
            Some((_, rows)) => rows.push(row),
            None => scopes.push((scope, vec![row])),
        }
    }
    for index in 0..scopes.len() {
        let (scope, rows) = &scopes[index];
        let concise = concise_scope(*scope);
        let disambiguate = scopes
            .iter()
            .filter(|(other_scope, _)| concise_scope(*other_scope) == concise)
            .count()
            > 1;
        output.push_str("  ");
        output.push_str(&scope_label(*scope, disambiguate));
        output.push('\n');
        append_rows(output, rows, "    ");
    }
}

fn append_rows(output: &mut String, rows: &[&ListRow], indent: &str) {
    for row in rows {
        output.push_str(indent);
        output.push_str("- ");
        output.push_str(&row.title);
        output.push('\n');
    }
}

fn scope_label(scope: Option<&str>, disambiguate: bool) -> String {
    match scope {
        None => "global".into(),
        Some(path) if path.trim().is_empty() => "<empty project>".into(),
        Some(path) if disambiguate && concise_scope(scope) == "global" => {
            format!("project: {path}")
        }
        Some(path) if disambiguate => path.into(),
        Some(_) => concise_scope(scope).into(),
    }
}

fn concise_scope(scope: Option<&str>) -> &str {
    match scope {
        None => "global",
        Some(path) if path.trim().is_empty() => "<empty project>",
        Some(path) => Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path),
    }
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
