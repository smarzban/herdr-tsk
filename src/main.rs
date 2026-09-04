//! tsk binary entry.

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use tsk_tui::cli::router::{route, Surface};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match route(&args, std::env::var(tsk_tui::app::MODE_ENV).ok().as_deref()) {
        Surface::FindBoardPane => find_board_pane_main(),
        Surface::GlobalHelp => {
            println!(
                "usage: tsk [capture] | add | steps | list | trash | archive | unarchive | --find-board-pane | --help\n\nCommands:\n  add    create one task or apply a JSON plan\n  steps  add or toggle one step on a task\n  list   inspect tasks\n  trash  restore a trashed task\n  archive    keep a task off the working views\n  unarchive  put an archived task back\n\nRun `tsk add --help`, `tsk steps --help`, `tsk list --help`, `tsk trash --help`, `tsk archive --help`, or `tsk unarchive --help` for command details."
            );
            ExitCode::SUCCESS
        }
        Surface::Usage => usage_exit(),
        Surface::Add
        | Surface::Steps
        | Surface::List
        | Surface::Trash
        | Surface::Archive
        | Surface::Unarchive => headless_main(args),
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
        "usage: tsk [capture] | add | steps | list | trash | archive | unarchive | --find-board-pane | --help"
    );
    ExitCode::from(2)
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
