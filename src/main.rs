//! herdr-tasks binary entry.

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--find-board-pane") {
        return find_board_pane_main();
    }

    if let Err(err) = herdr_tasks::run(args) {
        eprintln!("herdr-tasks: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
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
