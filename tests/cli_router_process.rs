use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn binary() -> String {
    std::env::var("CARGO_BIN_EXE_herdr-tasks")
        .expect("Cargo must provide the herdr-tasks binary path")
}

#[test]
fn top_level_help_names_subcommands_and_their_help() {
    let output = Command::new(binary())
        .arg("--help")
        .output()
        .expect("run global help");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
    assert!(stdout.contains("add"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("herdr-tasks add --help"));
    assert!(stdout.contains("herdr-tasks list --help"));
}

#[test]
fn unknown_positional_exits_2_without_opening_the_board() {
    let mut child = Command::new(binary())
        .arg("foo")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn herdr-tasks foo");
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll child process") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop TUI process after timeout");
            let _ = child.wait();
            panic!("unknown positional kept a TUI open");
        }
        thread::sleep(Duration::from_millis(10));
    };

    assert_eq!(status.code(), Some(2));
}
