//! herdr-tasks binary entry.

use std::io::{self, Write};
use std::process::ExitCode;

use herdr_tasks::cli::router::{route, Surface};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match route(
        &args,
        std::env::var(herdr_tasks::app::MODE_ENV).ok().as_deref(),
    ) {
        Surface::FindBoardPane => find_board_pane_main(),
        Surface::GlobalHelp => {
            println!("usage: herdr-tasks [capture] | add | list | --find-board-pane | --help");
            ExitCode::SUCCESS
        }
        Surface::Usage => usage_exit(),
        Surface::Add | Surface::List => {
            eprintln!("herdr-tasks: this command is not available yet");
            ExitCode::from(2)
        }
        Surface::Board | Surface::Capture => match herdr_tasks::run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("herdr-tasks: {err}");
                ExitCode::from(1)
            }
        },
    }
}

fn usage_exit() -> ExitCode {
    eprintln!("usage: herdr-tasks [capture] | add | list | --find-board-pane | --help");
    ExitCode::from(2)
}

/// Read herdr `pane list` JSON from stdin; print first Tasks pane_id or exit 1.
fn find_board_pane_main() -> ExitCode {
    match herdr_tasks::find_board_pane_from_stdin() {
        Ok(Some(id)) => {
            if writeln!(io::stdout(), "{id}").is_err() {
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::from(1),
        Err(err) => {
            eprintln!("herdr-tasks --find-board-pane: {err}");
            ExitCode::from(1)
        }
    }
}
