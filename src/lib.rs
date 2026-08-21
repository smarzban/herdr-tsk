//! herdr-tasks library root.

pub mod app;
pub mod attention;
pub mod board_pane;
pub mod capture;
pub mod config;
pub mod context;
pub mod dispatch;
pub mod domain;
pub mod host;
pub mod resume;
pub mod save_recovery;
pub mod scope;
pub mod store;
pub(crate) mod text;
pub mod ui;
pub mod views;

pub use board_pane::{find_board_pane_from_stdin, find_board_pane_id};

/// Run the herdr-tasks binary entrypoint with argv-style arguments.
///
/// Default mode is the Tasks board. Pass `capture` (or set `HERDR_TASKS_MODE=capture`)
/// for Capture UI mode (form; exits after save/cancel).
///
/// `--find-board-pane` is handled by the binary (`main`) before this entry.
pub fn run(
    args: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<(), Box<dyn std::error::Error>> {
    app::run(args)
}
