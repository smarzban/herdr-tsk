//! Text output for headless command results.

use std::collections::BTreeMap;

use super::CliOutput;
use crate::cli::add::{AddError, FlagAddResult};
use crate::cli::archive::{ArchiveCliError, ArchiveResult, ProjectResult};
use crate::cli::edit::{EditError, EditResult};
use crate::cli::list::{ListError, ListResult, ListRow, ListView};
use crate::cli::status::{StatusError, StatusResult};
use crate::cli::steps::{StepLine, StepsError, StepsResult};
use crate::cli::trash::{TrashCliError, TrashRestoreResult};
use crate::domain::HumanStatus;
use crate::ui::terminal_text;

fn human_reason(reason: &str) -> String {
    terminal_text(reason)
}

pub fn add_help() -> CliOutput {
    help_output(
        "usage: tsk add -t <title> [-n <notes>] [-p <project> | --desk] [--thread <name>] [--json] [--state-dir <dir>]\n       tsk add [--file <path|->] [--state-dir <dir>]",
    )
}

pub fn list_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: tsk list [<task>] [-p <project> | --desk | --all] [--thread <name>] [--done | --deleted | --archived] [--json] [--state-dir <dir>]\n\n",
            "Lists ready, started, blocked, and review tasks in the invocation project by default, or your desk outside a repository.\n",
            "With a task number (bare digits) or UUID from add --json or list --json, lists that one task alone and prints its steps: one line per step with its [x]/[ ] state and step short id. Direct lookup ignores cwd and searches the live store, including done and live soft-deleted tasks. Tasks that have moved to trash.jsonl need tsk list --deleted. A task operand cannot be combined with scope, thread, or status filters.\n",
            "--project uses the same basename-or-path scope resolution as add; --desk selects your desk, tasks not tied to a project; --all selects every scope. --thread normalizes a thread name and filters within the selected scope; an invalid name is a usage error (exit 2). For dash-leading project and state-directory values, use --project=<scope> and --state-dir=<dir>.\n",
            "--archived lists archived tasks only: individually archived tasks plus tasks of archived projects, each row marked `archived` or `project archived`. --done lists done tasks only. --deleted lists soft-deleted tasks only, regardless of status: live soft-deletes plus trash entries from trash.jsonl (kept 30 days), deduped by task with the live copy winning, newest deletion first.\n",
            "To recover a typo scope, use tsk list --all --json.\n",
            "--json emits a flat array of id, number, title, status, project, and thread (or null) in displayed group order. Human --all groups rows by status, then project scope, using a unique concise trailing path or desk.\n\n",
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
            "{usage}\n\nExamples:\n  tsk add -t \"Draft release notes\"\n  tsk add -t \"Buy milk\" --desk\n  tsk add -t \"Fix widget\" --project widget --thread release-2026\n  tsk add --title=\"-fix parser\" --notes=\"-5 degrees\" --project=\"-maintenance\"\n  tsk add --file plan.json\n  cat plan.json | tsk add\n\nValues beginning with - must use --title=<value>, --notes=<value>, --project=<value>, --state-dir=<dir>, or --file=<path>.\nItem flags plus --file are usage (exit 2, nothing persists). Piped stdin with item flags is ignored and not read. An add whose trimmed title, resolved project scope, and normalized thread already exist succeeds without changing the task. With --json, flag add emits one object with outcome, id, number, title, and project (or null).\nPlan JSON: [{{\"title\": \"...\", \"notes\": \"...\", \"project\": \"...\", \"thread\": \"...\"}}] (thread may also be null)\nPlan result: {{\"created\": [...], \"existing\": [...], \"failed\": [...]}}\n\nExit contract:\n  exit 0: every item was created or already existed\n  exit 1: one or more items were refused, retry failed only\n  exit 2: usage or parse error, nothing persisted\n  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn added(result: FlagAddResult, json: bool) -> CliOutput {
    let stdout = match result {
        FlagAddResult::Created {
            id,
            number,
            title,
            project,
        } if json => format!(
            "{}\n",
            serde_json::json!({
                "outcome": "created",
                "id": id,
                "number": number,
                "title": title,
                "project": project,
            })
        ),
        FlagAddResult::Existing {
            id,
            number,
            title,
            project,
        } if json => format!(
            "{}\n",
            serde_json::json!({
                "outcome": "existing",
                "id": id,
                "number": number,
                "title": title,
                "project": project,
            })
        ),
        FlagAddResult::Created { title, .. } => format!("added {}\n", terminal_text(&title)),
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
            "tsk add: {}\nusage: tsk add -t <title> [-n <notes>] [-p <project> | --desk] [--thread <name>] [--json] [--state-dir <dir>]\n",
            human_reason(reason)
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

/// The flat row array. A single-task listing with steps attaches them to its
/// one row (`steps`: id, done, short_id, text); every other listing keeps
/// the standard task row shape.
fn list_json(result: &ListResult) -> String {
    let mut value = serde_json::to_value(&result.rows).expect("list rows are serializable");
    if !result.steps.is_empty() {
        value
            .as_array_mut()
            .expect("rows serialize to an array")
            .get_mut(0)
            .expect("a steps collection implies the single task row")
            .as_object_mut()
            .expect("row serializes to an object")
            .insert(
                "steps".into(),
                serde_json::to_value(&result.steps).expect("steps are serializable"),
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
        ListView::Archived => &[(None, "ARCHIVED")],
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
            if !result.steps.is_empty() {
                // Single-task listing: the step lines belong under the one row above.
                append_step_lines(&mut output, &result.steps, " ");
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
        output.push_str(&row.number.to_string());
        output.push(' ');
        output.push_str(&terminal_text(&row.title));
        if let Some(thread) = row.thread.as_deref() {
            output.push_str(" #");
            output.push_str(&terminal_text(thread));
        }
        if let Some(mark) = row.archived {
            output.push_str(" · ");
            output.push_str(mark);
        }
        output.push('\n');
    }
}

/// One line per step: state glyph, short id, text, one level under the row.
fn append_step_lines(output: &mut String, steps: &[StepLine], indent: &str) {
    for step in steps {
        output.push_str(indent);
        output.push_str("  [");
        output.push(if step.done { 'x' } else { ' ' });
        output.push_str("] ");
        output.push_str(&step.short_id);
        output.push(' ');
        output.push_str(&terminal_text(&step.text));
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
            None => "desk".into(),
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

pub fn steps_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: tsk steps <task> add <text> [--state-dir <dir>]\n",
            "       tsk steps <task> toggle <step-short-id> [--state-dir <dir>]\n",
            "       tsk steps <task> rename <step-short-id> <text> [--state-dir <dir>]\n",
            "       tsk steps <task> remove <step-short-id> [--state-dir <dir>]\n\n",
            "steps adds, toggles, renames, or removes one step on a task. The task is a bare task number or UUID from tsk list --json; direct lookup ignores cwd.\n",
            "A step short id is the shortest unambiguous prefix of the step id, as printed by tsk list <task>.\n",
            "toggle flips the step state: a blind retry after an unseen success flips it back, so verify with tsk list <task> before retrying.\n",
            "rename is idempotent on the trimmed text. remove is not: a retry after an unseen success is unknown-step, so verify with list before retrying.\n\n",
            "Refusal tokens (exit 1): empty-step-text, invalid-step-text, unknown-task, soft-deleted-task, unknown-step, ambiguous-step.\n\n",
            "Exit contract:\n",
            "  exit 0: step created, toggled, renamed, or removed\n",
            "  exit 1: step refusal; verify state with list before retrying\n",
            "  exit 2: usage or parse error, nothing persisted\n",
            "  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        )
        .into(),
        stderr: String::new(),
        code: 0,
    }
}

pub fn steps(result: StepsResult) -> CliOutput {
    let stdout = match result {
        StepsResult::Added { short_id, text } => {
            format!("added {short_id} {}\n", terminal_text(&text))
        }
        StepsResult::Toggled {
            short_id,
            text,
            done,
        } => format!(
            "toggled {short_id} [{}] {}\n",
            if done { "x" } else { " " },
            terminal_text(&text)
        ),
        StepsResult::Renamed { short_id, text } => {
            format!("renamed {short_id} {}\n", terminal_text(&text))
        }
        StepsResult::Removed { short_id, text } => {
            format!("removed {short_id} {}\n", terminal_text(&text))
        }
    };
    CliOutput {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}

pub fn steps_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "tsk steps: {}\nusage: tsk steps <task> add <text> | toggle <step-short-id> | rename <step-short-id> <text> | remove <step-short-id> [--state-dir <dir>]\n",
            human_reason(reason)
        ),
        code: 2,
    }
}

pub fn steps_rejected(error: StepsError) -> CliOutput {
    let (detail, code) = match error {
        StepsError::Store(detail) => (detail, 3),
        other => (other.code().into(), 1),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("tsk steps: {detail}\n"),
        code,
    }
}

fn status_name(status: HumanStatus) -> &'static str {
    match status {
        HumanStatus::Ready => "ready",
        HumanStatus::Started => "started",
        HumanStatus::Blocked => "blocked",
        HumanStatus::Review => "review",
        HumanStatus::Done => "done",
    }
}

pub fn status_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: tsk status <task> <status> [--state-dir <dir>]\n\n",
            "status sets a task's human status to ready, started, blocked, review, or done. start is accepted as an alias for started. The task is a task number (T<number>, or bare digits) or UUID, as shown by tsk list. Repeating the same status is idempotent: the same output prints and nothing changes.\n\n",
            "Refusal tokens (exit 1): unknown-task, soft-deleted-task.\n\n",
            "Exit contract:\n",
            "  exit 0: status set, or it already had the value\n",
            "  exit 1: status refusal; verify state with list before retrying\n",
            "  exit 2: usage or parse error, nothing persisted\n",
            "  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        )
        .into(),
        stderr: String::new(),
        code: 0,
    }
}

pub fn status(result: StatusResult) -> CliOutput {
    CliOutput {
        stdout: format!(
            "status T{} {} {}\n",
            result.number,
            status_name(result.status),
            terminal_text(&result.title)
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn status_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "tsk status: {}\nusage: tsk status <task> <status> [--state-dir <dir>]\n",
            human_reason(reason)
        ),
        code: 2,
    }
}

pub fn status_rejected(error: StatusError) -> CliOutput {
    let (detail, code) = match error {
        StatusError::Store(detail) => (detail, 3),
        other => (other.code().into(), 1),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("tsk status: {detail}\n"),
        code,
    }
}

pub fn edit_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: tsk edit <task> [--title <title>] [--notes <notes>] [--state-dir <dir>]\n\n",
            "edit updates a task's title and/or notes. Scope and thread are unchanged. The task is a task number (T<number>, or bare digits) or UUID, as shown by tsk list. At least one of --title or --notes is required.\n",
            "Notes that trim to nothing are cleared. Newlines and tabs in notes are kept, same as add. Repeating the stored values is idempotent: the same output prints and nothing changes.\n",
            "Values beginning with - must use --title=<value> or --notes=<value>.\n\n",
            "Refusal tokens (exit 1): unknown-task, soft-deleted-task, empty-title, invalid-title.\n\n",
            "Exit contract:\n",
            "  exit 0: fields written, or they already had the values\n",
            "  exit 1: edit refusal; verify state with list before retrying\n",
            "  exit 2: usage or parse error, nothing persisted\n",
            "  exit 3: store I/O, commit indeterminate, verify with list before retrying\n"
        )
        .into(),
        stderr: String::new(),
        code: 0,
    }
}

pub fn edited(result: EditResult) -> CliOutput {
    CliOutput {
        stdout: format!(
            "edited T{} {}\n",
            result.number,
            terminal_text(&result.title)
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn edit_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "tsk edit: {}\nusage: tsk edit <task> [--title <title>] [--notes <notes>] [--state-dir <dir>]\n",
            human_reason(reason)
        ),
        code: 2,
    }
}

pub fn edit_rejected(error: EditError) -> CliOutput {
    let (detail, code) = match error {
        EditError::Store(detail) => (detail, 3),
        other => (other.code().into(), 1),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("tsk edit: {detail}\n"),
        code,
    }
}

pub fn trash_help() -> CliOutput {
    CliOutput {
        stdout: concat!(
            "usage: tsk trash restore <task> [--state-dir <dir>]\n\n",
            "restore puts a trashed task back on the board. The task is a task number (T<number>, or bare digits) or UUID, as shown by tsk list --deleted. The task returns not soft-deleted, with a restored event, a new revision, and its old number.\n",
            "Deleted tasks live beside the board in trash.jsonl for 30 days; tsk list --deleted lists live soft-deleted tasks and trash entries together, newest deletion first.\n\n",
            "Exit contract:\n",
            "  exit 0: task restored\n",
            "  exit 1: no matching trash line, or the task is already live\n",
            "  exit 2: usage or parse error, nothing persisted\n",
            "  exit 3: store I/O, commit indeterminate, verify with tsk list --deleted before retrying\n"
        )
        .into(),
        stderr: String::new(),
        code: 0,
    }
}

pub fn trash_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "tsk trash: {}\nusage: tsk trash restore <task> [--state-dir <dir>]\n",
            human_reason(reason)
        ),
        code: 2,
    }
}

pub fn trash_restored(result: TrashRestoreResult) -> CliOutput {
    CliOutput {
        stdout: format!(
            "restored {} {}\n",
            result.identifier,
            terminal_text(&result.title)
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn trash_rejected(error: TrashCliError) -> CliOutput {
    let (detail, code) = match error {
        TrashCliError::Store(detail) => (detail, 3),
        TrashCliError::NotInTrash(detail) => (detail, 1),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("tsk trash: {detail}\n"),
        code,
    }
}

pub fn archive_help(verb: &str) -> CliOutput {
    let antiverb = if verb == "archive" {
        "unarchive"
    } else {
        "archive"
    };
    let body = format!(
        "{verb} sets the task's archived flag. The task is a task number (T<number>, or bare digits) or UUID, as shown by tsk list. An archived task keeps its human status and leaves every working view; it is visible again with tsk list --archived and returns with tsk {antiverb}. Repeating the verb is idempotent: the same output prints and nothing changes.",
    );
    CliOutput {
        stdout: format!(
            "usage: tsk {verb} <task> [--state-dir <dir>]\n\n{body}\n\nExit contract:\n  exit 0: the flag was set, or it already had the value\n  exit 1: unknown task, or the task is deleted\n  exit 2: usage or parse error, nothing persisted\n  exit 3: store I/O, commit indeterminate, verify with tsk list before retrying\n"
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn archive_usage(verb: &str, reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "tsk {verb}: {}\nusage: tsk {verb} <task> [--state-dir <dir>]\n",
            human_reason(reason)
        ),
        code: 2,
    }
}

pub fn project_help() -> CliOutput {
    CliOutput {
        stdout: "usage: tsk project archive <name> | tsk project unarchive <name> [--state-dir <dir>]\n\nproject archive keeps a whole project off the working views; project unarchive brings it back with every task in the status it had. <name> follows the same rules as add -p: a project basename (case-insensitive) or a /path verbatim. A name matching no project that has tasks exits 1. Repeating the action is idempotent.\n\nExit contract:\n  exit 0: the record was written, or it already had the value\n  exit 1: no project with that name has tasks\n  exit 2: usage or parse error, nothing persisted\n  exit 3: store I/O, commit indeterminate\n".into(),
        stderr: String::new(),
        code: 0,
    }
}

pub fn project_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "tsk project: {}\nusage: tsk project archive <name> | tsk project unarchive <name> [--state-dir <dir>]\n",
            human_reason(reason)
        ),
        code: 2,
    }
}

pub fn project_archived(result: ProjectResult, verb: &str) -> CliOutput {
    let past = if verb == "archive" {
        "archived"
    } else {
        "unarchived"
    };
    CliOutput {
        stdout: format!("{past} project {}\n", terminal_text(&result.name)),
        stderr: String::new(),
        code: 0,
    }
}

pub fn archived(result: ArchiveResult, verb: &str) -> CliOutput {
    // The row reads in the past tense: `archived T7 title` / `unarchived T7 title`.
    let past = if verb == "archive" {
        "archived"
    } else {
        "unarchived"
    };
    CliOutput {
        stdout: format!(
            "{past} T{} {}\n",
            result.number,
            terminal_text(&result.title)
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn archive_rejected(error: ArchiveCliError, verb: &str) -> CliOutput {
    // Task-verb refusals name the invoked verb (`tsk archive:` / `tsk unarchive:`);
    // project refusals keep their own prefix.
    let (verb, detail, code) = match error {
        ArchiveCliError::Store(detail) => (verb.to_string(), detail, 3),
        ArchiveCliError::UnknownTask(detail) => (verb.to_string(), detail, 1),
        ArchiveCliError::SoftDeleted(detail) => (verb.to_string(), detail, 1),
        ArchiveCliError::UnknownProject(detail) => {
            ("project".to_string(), terminal_text(&detail), 1)
        }
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("tsk {verb}: {detail}\n"),
        code,
    }
}

pub fn list_usage(reason: &str) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!(
            "tsk list: {}\nusage: tsk list [<task>] [-p <project> | --desk | --all] [--thread <name>] [--done | --deleted | --archived] [--json] [--state-dir <dir>]\n",
            human_reason(reason)
        ),
        code: 2,
    }
}

pub fn list_rejected(error: ListError) -> CliOutput {
    match error {
        ListError::Store(detail) => CliOutput {
            stdout: String::new(),
            stderr: format!("tsk list: {detail}\n"),
            code: 3,
        },
        // A well-formed address that addresses no task: the invocation is wrong, not the store.
        ListError::UnknownTask => list_usage("unknown task"),
    }
}

pub fn rejected(error: AddError) -> CliOutput {
    let (detail, code) = match &error {
        AddError::ProjectArchived(name) => {
            let name = terminal_text(name);
            (
                format!(
                    "project-archived: project {name} is archived. Use --desk, -p <other project>, or tsk project unarchive {name}"
                ),
                1,
            )
        }
        AddError::Store(detail) => (detail.clone(), 3),
        other => (other.code().into(), 1),
    };
    CliOutput {
        stdout: String::new(),
        stderr: format!("tsk add: {detail}\n"),
        code,
    }
}

pub fn setup_help() -> CliOutput {
    CliOutput {
        stdout: format!(
            "{}\nRegister the installed binary and bundled plugin assets.\n\
             Adds prefix+t for board and prefix+a for capture; asks before replacing conflicts.\n\
             Uses HERDR_CONFIG_PATH, or XDG_CONFIG_HOME/herdr/config.toml, or ~/.config/herdr/config.toml.\n\
             On a TTY, bare `tsk setup` detects global agent skill roots and asks once to install or update.\n\
             Named agent targets write skills/tsk-cli/SKILL.md into that tool's user-level skills directory.\n\
             Matching skill version exits 1 with skill-exists; a missing or different version updates without --force.\n\
             --force always overwrites. `tsk setup agents --yes` installs or updates every detected agent.\n",
            crate::setup_agent::USAGE
        ),
        stderr: String::new(),
        code: 0,
    }
}

pub fn setup_agent_listed(json: bool) -> CliOutput {
    if json {
        return match crate::setup_agent::detection_json() {
            Ok(text) => CliOutput {
                stdout: text,
                stderr: String::new(),
                code: 0,
            },
            Err(error) => setup_error(&error.to_string(), 1),
        };
    }
    CliOutput {
        stdout: crate::setup_agent::list_text(),
        stderr: String::new(),
        code: 0,
    }
}

pub fn setup_agent_detected_json(text: String) -> CliOutput {
    CliOutput {
        stdout: text,
        stderr: String::new(),
        code: 0,
    }
}

pub fn setup_agent_detected_ids(ids: Vec<String>) -> CliOutput {
    CliOutput {
        stdout: if ids.is_empty() {
            String::new()
        } else {
            format!("{}\n", ids.join(" "))
        },
        stderr: String::new(),
        code: 0,
    }
}

pub fn setup_agent_written(
    target: &crate::setup_agent::Target,
    outcome: &crate::setup_agent::InstallOutcome,
    json: bool,
) -> CliOutput {
    let path = outcome.path();
    if json {
        return setup_agent_json(outcome.kind(), Some(target.name()), Some(path), 0);
    }
    CliOutput {
        stdout: format!("{}\n", path.display()),
        stderr: String::new(),
        code: 0,
    }
}

pub fn setup_agent_exists(
    target: &crate::setup_agent::Target,
    path: &std::path::Path,
    json: bool,
) -> CliOutput {
    if json {
        return setup_agent_json("exists", Some(target.name()), Some(path), 1);
    }
    CliOutput {
        stdout: String::new(),
        stderr: "tsk setup: skill-exists\n".into(),
        code: 1,
    }
}

pub fn setup_agent_batch(result: crate::setup_agent::BatchResult, json: bool) -> CliOutput {
    if json {
        let payload = serde_json::json!({
            "outcome": if result.declined {
                "declined"
            } else if result.none_detected {
                "none-detected"
            } else if result.applied.is_empty() {
                "current"
            } else {
                "batch"
            },
            "applied": result.applied.iter().map(|(id, outcome)| serde_json::json!({
                "id": id,
                "kind": outcome.kind(),
                "path": outcome.path().display().to_string(),
            })).collect::<Vec<_>>(),
            "skipped_current": result.skipped_current,
            "declined": result.declined,
            "none_detected": result.none_detected,
        });
        return CliOutput {
            stdout: format!("{payload}\n"),
            stderr: String::new(),
            code: 0,
        };
    }
    // Interactive path already wrote progress to stderr; keep stdout quiet unless scripted batch.
    let mut stdout = String::new();
    for (id, outcome) in &result.applied {
        stdout.push_str(&format!(
            "{id}: {} {}\n",
            outcome.kind(),
            outcome.path().display()
        ));
    }
    for id in &result.skipped_current {
        stdout.push_str(&format!("{id}: current\n"));
    }
    if result.none_detected && stdout.is_empty() {
        stdout.push_str("No agent skill roots detected.\n");
    }
    CliOutput {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}

fn setup_agent_json(
    outcome: &str,
    target: Option<&str>,
    path: Option<&std::path::Path>,
    code: u8,
) -> CliOutput {
    CliOutput {
        stdout: format!(
            "{}\n",
            serde_json::json!({
                "outcome": outcome,
                "target": target,
                "path": path.map(|value| value.display().to_string()),
            })
        ),
        stderr: String::new(),
        code,
    }
}
pub fn setup_error(reason: &str, code: u8) -> CliOutput {
    CliOutput {
        stdout: String::new(),
        stderr: format!("tsk setup: {}\n", human_reason(reason)),
        code,
    }
}
pub fn setup(result: crate::setup::SetupResult) -> CliOutput {
    let mut stdout = String::new();
    if let Some(backup) = result.backup {
        stdout.push_str(&format!(
            "Herdr config backup: {}\n",
            terminal_text(&backup.display().to_string())
        ));
    }
    stdout.push_str(&format!(
        "Herdr plugin registered, using {}\nPlugin root: {}\n",
        terminal_text(&result.binary.display().to_string()),
        terminal_text(&result.root.display().to_string())
    ));
    stdout.push_str("Configured available shortcuts: prefix+t board, prefix+a quick capture. Declined conflicts were left unchanged.\nReload Herdr configuration (herdr server reload-config), or restart Herdr, to apply shortcuts.\n");
    CliOutput {
        stdout,
        stderr: String::new(),
        code: 0,
    }
}
