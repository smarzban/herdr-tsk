//! tsk binary entry.

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use tsk_tui::cli::router::{route, Surface};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match route(&args, std::env::var(tsk_tui::app::MODE_ENV).ok().as_deref()) {
        Surface::FindBoardPane => find_board_pane_main(),
        Surface::ResolveContext => resolve_context_main(),
        Surface::GlobalHelp => {
            println!(
                "usage: tsk [capture] | add | steps | list | status | edit | trash | archive | unarchive | project | --find-board-pane | --help\n\nCommands:\n  add    create one task or apply a JSON plan\n  steps  add, toggle, rename, or remove one step on a task\n  list   inspect tasks\n  status set a task's human status\n  edit   update a task's title or notes\n  trash  restore a trashed task\n  archive    keep a task off the working views\n  unarchive  put an archived task back\n  project    archive or unarchive a project\n\nRun `tsk add --help`, `tsk steps --help`, `tsk list --help`, `tsk status --help`, `tsk edit --help`, `tsk trash --help`, `tsk archive --help`, `tsk unarchive --help`, or `tsk project --help` for command details."
            );
            ExitCode::SUCCESS
        }
        Surface::Usage => usage_exit(),
        Surface::Add
        | Surface::Steps
        | Surface::List
        | Surface::Status
        | Surface::Edit
        | Surface::Trash
        | Surface::Archive
        | Surface::Unarchive
        | Surface::Project => headless_main(args),
        Surface::Board | Surface::Capture => match tsk_tui::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("tsk: {err}");
                ExitCode::from(1)
            }
        },
    }
}

fn usage_exit() -> ExitCode {
    eprintln!(
        "usage: tsk [capture] | add | steps | list | status | edit | trash | archive | unarchive | project | --find-board-pane | --help"
    );
    ExitCode::from(2)
}

/// Internal launcher helper: turn the host's invocation JSON into a one-shot
/// request for an already-running board, then print the resolved repository.
fn resolve_context_main() -> ExitCode {
    const MAX_CONTEXT_BYTES: usize = 64 * 1024;
    let mut json = String::new();
    if io::stdin()
        .take((MAX_CONTEXT_BYTES + 1) as u64)
        .read_to_string(&mut json)
        .is_err()
        || json.len() > MAX_CONTEXT_BYTES
    {
        eprintln!("tsk --resolve-context: invalid context payload");
        return ExitCode::from(1);
    }
    let raw = match serde_json::from_str::<tsk_tui::context::RawHostContext>(&json) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("tsk --resolve-context: invalid context JSON: {error}");
            return ExitCode::from(1);
        }
    };
    let snapshot = tsk_tui::context::build_snapshot(&raw, std::path::PathBuf::new());
    let project = snapshot.this_repo.clone();
    if let Err(error) = tsk_tui::reopen::ReopenRequest::new(project.clone())
        .write(&tsk_tui::store::default_state_dir())
    {
        eprintln!("tsk --resolve-context: {error}");
        return ExitCode::from(1);
    }
    if let Some(project) = project {
        println!("{}", project.display());
    }
    ExitCode::SUCCESS
}

fn headless_main(args: Vec<String>) -> ExitCode {
    let stdin = io::stdin();
    let stdin_is_tty = stdin.is_terminal();
    let output = tsk_tui::cli::run_with(args, stdin, stdin_is_tty);
    if io::stdout().write_all(output.stdout.as_bytes()).is_err() {
        return ExitCode::from(1);
    }
    if io::stderr().write_all(output.stderr.as_bytes()).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::from(output.code)
}

/// Read herdr `pane list` JSON from stdin; print first Tasks pane_id or exit 1.
fn find_board_pane_main() -> ExitCode {
    match tsk_tui::find_board_pane_from_stdin() {
        Ok(Some(id)) => {
            if writeln!(io::stdout(), "{id}").is_err() {
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::from(1),
        Err(err) => {
            eprintln!("tsk --find-board-pane: {err}");
            ExitCode::from(1)
        }
    }
}
