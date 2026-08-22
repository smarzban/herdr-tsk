//! Text output for headless command results.

use std::collections::BTreeMap;

use super::CliOutput;
use crate::cli::add::{AddError, FlagAddResult};
use crate::cli::check::{CheckError, CheckResult, ChecklistLine};
use crate::cli::list::{ListError, ListResult, ListRow, ListView};
use crate::domain::HumanStatus;
use crate::ui::terminal_text;

pub fn add_help() -> CliOutput {
    help_output(
        "usage: herdr-tasks add -t <title> [-n <notes>] [-p <project> | --global] [--json] [--state-dir <dir>]\n       herdr-tasks add [--file <path|->] [--state-dir <dir>]",
    )
}

pub fn list_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: herdr-tasks list [<task-id>] [-p <project> | --global | --all] [--done | --deleted] [--json] [--state-dir <dir>]\n\n",
            "Lists ready, started, blocked, and review tasks in the invocation project by default, or global scope outside a repository.\n",
            "With a task id (a task UUID from add --json or list --json), lists that one task alone and prints its checklist: one line per item with its [x]/[ ] state and item short id. A task id cannot be combined with scope or status filters.\n",
            "--project uses the same basename-or-path scope resolution as add; --global selects global tasks; --all selects every scope. For dash-leading project and state-directory values, use --project=<scope> and --state-dir=<dir>.\n",
            "--done lists done tasks only. --deleted lists soft-deleted tasks only, regardless of status.\n",
            "To recover a typo scope, use herdr-tasks list --all --json.\n",
            "--json emits a flat array of id, title, status, and project in displayed group order. Human --all groups rows by status, then project scope, using a unique concise trailing path or global.\n\n",
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
            "{usage}\n\nExamples:\n  herdr-tasks add -t \"Draft release notes\"\n  herdr-tasks add -t \"Buy milk\" --global\n  herdr-tasks add -t \"Fix widget\" --project widget\n  herdr-tasks add --title=\"-fix parser\" --notes=\"-5 degrees\" --project=\"-maintenance\"\n  herdr-tasks add --file plan.json\n  cat plan.json | herdr-tasks add\n\nValues beginning with - must use --title=<value>, --notes=<value>, --project=<value>, --state-dir=<dir>, or --file=<path>.\nAn add whose trimmed title and resolved project scope already exist succeeds without changing the task. With --json, flag add emits one object with outcome, id, title, and project (or null).\nPlan JSON: [{{\"title\": \"...\", \"notes\": \"...\", \"project\": \"...\"}}]\nPlan result: {{\"created\": [...], \"existing\": [...], \"failed\": [...]}}\n\nExit contract:\n  exit 0: every item was created or already existed\n  exit 1: one or more items were refused, retry failed only\n  exit 2: usage or parse error, nothing persisted\n  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
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
        list_json(&result)
    } else {
        list_human(&result)
    };
    CliOutput {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}

/// The flat row array. A single-task listing with items attaches them to its
/// one row (`checklist`: id, done, short_id, text); every other listing keeps
/// today's exact row shape.
fn list_json(result: &ListResult) -> String {
    let mut value = serde_json::to_value(&result.rows).expect("list rows are serializable");
    if !result.checklist.is_empty() {
        value
            .as_array_mut()
            .expect("rows serialize to an array")
            .get_mut(0)
            .expect("a checklist implies the single task row")
            .as_object_mut()
            .expect("row serializes to an object")
            .insert(
                "checklist".into(),
                serde_json::to_value(&result.checklist).expect("checklist is serializable"),
            );
    }
    format!("{value}\n")
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
    let labels = result.include_scope.then(|| scope_labels(&result.rows));
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
            append_scope_groups(&mut output, rows, labels.as_ref().expect("scope labels"));
        } else {
            append_rows(&mut output, &rows, " ");
            if !result.checklist.is_empty() {
                // Single-task listing: the checklist lines belong under the one row above.
                append_checklist_lines(&mut output, &result.checklist, " ");
            }
        }
    }
    output
}

fn append_scope_groups(
    output: &mut String,
    rows: Vec<&ListRow>,
    labels: &BTreeMap<Option<String>, String>,
) {
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
    for (scope, rows) in scopes {
        output.push_str("  ");
        output.push_str(&terminal_text(
            labels
                .get(&scope.map(str::to_owned))
                .expect("label for displayed scope"),
        ));
        output.push('\n');
        append_rows(output, &rows, "    ");
    }
}

fn append_rows(output: &mut String, rows: &[&ListRow], indent: &str) {
    for row in rows {
        output.push_str(indent);
        output.push_str("- ");
        output.push_str(&terminal_text(&row.title));
        output.push('\n');
    }
}

/// One line per checklist item: state glyph, short id, text, one level under the row.
fn append_checklist_lines(output: &mut String, checklist: &[ChecklistLine], indent: &str) {
    for item in checklist {
        output.push_str(indent);
        output.push_str("  [");
        output.push(if item.done { 'x' } else { ' ' });
        output.push_str("] ");
        output.push_str(&item.short_id);
        output.push(' ');
        output.push_str(&terminal_text(&item.text));
        output.push('\n');
    }
}

fn scope_labels(rows: &[ListRow]) -> BTreeMap<Option<String>, String> {
    let mut entries = Vec::new();
    let mut empty_projects = 0;
    for row in rows {
        if entries
            .iter()
            .any(|entry: &ScopeLabel| entry.scope == row.project)
        {
            continue;
        }
        let empty_number = if row
            .project
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            empty_projects += 1;
            empty_projects
        } else {
            0
        };
        entries.push(ScopeLabel::new(row.project.clone(), empty_number));
    }

    loop {
        let mut labels = BTreeMap::<String, Vec<usize>>::new();
        for (index, entry) in entries.iter().enumerate() {
            labels
                .entry(terminal_text(&entry.label()))
                .or_default()
                .push(index);
        }
        let duplicate_groups = labels
            .values()
            .filter(|indexes| indexes.len() > 1)
            .cloned()
            .collect::<Vec<_>>();
        if duplicate_groups.is_empty() {
            break;
        }

        let mut changed = false;
        for indexes in &duplicate_groups {
            for &index in indexes {
                changed |= entries[index].widen();
            }
        }
        if !changed {
            break;
        }
    }

    entries
        .into_iter()
        .map(|entry| {
            let label = entry.label();
            (entry.scope, label)
        })
        .collect()
}

struct ScopeLabel {
    scope: Option<String>,
    segments: Vec<String>,
    depth: usize,
    prefixed: bool,
    raw: bool,
    empty_number: usize,
}

impl ScopeLabel {
    fn new(scope: Option<String>, empty_number: usize) -> Self {
        let segments = scope
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(path_segments)
            .unwrap_or_default();
        Self {
            scope,
            depth: 1,
            segments,
            prefixed: false,
            raw: false,
            empty_number,
        }
    }

    fn label(&self) -> String {
        let mut label = match self.scope.as_deref() {
            None => "global".into(),
            Some(path) if path.trim().is_empty() => {
                format!("project: <empty project {}>", self.empty_number)
            }
            Some(path) if self.raw => format!(
                "project: {}",
                serde_json::to_string(path).expect("scope path is serializable")
            ),
            Some(path) if self.segments.is_empty() => path.into(),
            Some(_) => {
                let start = self.segments.len().saturating_sub(self.depth);
                self.segments[start..].join("/")
            }
        };
        if self.prefixed && !self.raw {
            label = format!("project: {label}");
        }
        label
    }

    fn widen(&mut self) -> bool {
        if self.scope.is_none()
            || self
                .scope
                .as_deref()
                .is_some_and(|path| path.trim().is_empty())
        {
            return false;
        }
        if self.depth < self.segments.len() {
            self.depth += 1;
            true
        } else if !self.prefixed {
            self.prefixed = true;
            true
        } else if !self.raw {
            self.raw = true;
            true
        } else {
            false
        }
    }
}

fn path_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn check_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: herdr-tasks check <task-id> add <text> [--state-dir <dir>]\n",
            "       herdr-tasks check <task-id> toggle <item-short-id> [--state-dir <dir>]\n\n",
            "check adds one checklist item to a task or toggles one item's done flag. The task id is a task UUID from herdr-tasks list --json.\n",
            "An item short id is the shortest unambiguous prefix of the item id, as printed by herdr-tasks list <task-id>.\n",
            "toggle flips the item state: a blind retry after an unseen success flips it back, so verify with herdr-tasks list <task-id> before retrying.\n\n",
            "Refusal tokens (exit 1): empty-item-text, invalid-item-text, unknown-task, soft-deleted-task, unknown-item, ambiguous-item.\n\n",
            "Exit contract:\n",
            "  exit 0: item created or toggled\n",
            "  exit 1: item refusal; verify state with list before retrying\n",
            "  exit 2: usage or parse error, nothing persisted\n",
            "  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        )
        .into(),
        stderr: String::new(),
        code: 0,
    }
}

pub fn checked(result: CheckResult) -> CliOutput {
    let stdout = match result {
        CheckResult::Added { short_id, text } => format!("added {short_id} {text}\n"),
        CheckResult::Toggled {
            short_id,
            text,
            done,
        } => format!(
            "toggled {short_id} [{}] {text}\n",
            if done { "x" } else { " " }
        ),
    };
    CliOutput {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}

pub fn check_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "herdr-tasks check: {reason}\nusage: herdr-tasks check <task-id> add <text> | toggle <item-short-id> [--state-dir <dir>]\n"
        ),
        code: 2,
    }
}

pub fn check_rejected(error: CheckError) -> CliOutput {
    let (detail, code) = match error {
        CheckError::Store(detail) => (detail, 3),
        other => (other.code().into(), 1),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("herdr-tasks check: {detail}\n"),
        code,
    }
}

pub fn list_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "herdr-tasks list: {reason}\nusage: herdr-tasks list [<task-id>] [-p <project> | --global | --all] [--done | --deleted] [--json] [--state-dir <dir>]\n"
        ),
        code: 2,
    }
}

pub fn list_rejected(error: ListError) -> CliOutput {
    match error {
        ListError::Store(detail) => CliOutput {
            stdout: String::new(),
            stderr: format!("herdr-tasks list: {detail}\n"),
            code: 3,
        },
        // A well-formed id that addresses no task: the invocation is wrong, not the store.
        ListError::UnknownTask(id) => list_usage(&format!("unknown task id {id}")),
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
