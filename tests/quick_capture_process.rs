//! Real capture entrypoint, isolated PTY and store. Never uses a live Herdr session.
#![cfg(unix)]
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tsk_tui::domain::{DomainState, ProvenanceOrigin, TaskScope};
use tsk_tui::reopen::ReopenRequest;
use tsk_tui::store::TaskStore;
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Session {
    child: Child,
    tty: File,
    root: std::path::PathBuf,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let mut drain = [0; 8192];
        while matches!(self.tty.read(&mut drain), Ok(n) if n > 0) {}
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Session {
    fn output_until(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut out = String::new();
        while Instant::now() < deadline {
            let mut bytes = [0; 8192];
            if let Ok(n) = self.tty.read(&mut bytes) {
                out.push_str(&String::from_utf8_lossy(&bytes[..n]));
            }
            if out.contains(needle) {
                return out;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("missing {needle}: {out}");
    }
}
#[test]
fn capture_entrypoint_skips_launch_card_preserves_reopen_and_exits_on_escape() {
    let root = std::env::temp_dir().join(format!(
        "tsk-capture-pty-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("repo/.git")).unwrap();
    let repo = root.join("repo");
    let store = TaskStore::new(root.join("state"));
    let mut state = DomainState::new();
    state
        .create(
            "archived seed",
            None,
            TaskScope::Project {
                path: repo.display().to_string(),
            },
            ProvenanceOrigin::Manual,
            None,
        )
        .unwrap();
    state.archive_project(repo.to_str().unwrap()).unwrap();
    store.save(&state).unwrap();
    ReopenRequest::new(None).write(store.path()).unwrap();
    let request = fs::read(store.path().join("reopen.json")).unwrap();
    let mut master = -1;
    let mut slave = -1;
    let mut size = libc::winsize {
        ws_row: 13,
        ws_col: 78,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: openpty initializes valid descriptors; size is live for the call.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut size,
            )
        },
        0
    );
    // SAFETY: each descriptor has one owned File, and is valid after openpty.
    let tty = unsafe { File::from_raw_fd(master) };
    let input = unsafe { File::from_raw_fd(slave) };
    // SAFETY: prevent the child from retaining an extra master/slave across exec.
    unsafe {
        assert_ne!(libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC), -1);
        assert_ne!(libc::fcntl(slave, libc::F_SETFD, libc::FD_CLOEXEC), -1);
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsk"));
    command
        .arg("capture")
        .current_dir(&repo)
        .env("TSK_STATE_DIR", store.path())
        .env("TSK_NO_UPDATE_CHECK", "1")
        .env("TSK_CONFIG_DIR", root.join("config"))
        .env_remove("TSK_MODE")
        .env("TERM", "xterm-256color")
        .env(
            "HERDR_PLUGIN_CONTEXT_JSON",
            serde_json::json!({"focused_pane_cwd":repo,"selected_text":"PTY capture"}).to_string(),
        )
        .stdin(Stdio::from(input.try_clone().unwrap()))
        .stdout(Stdio::from(input.try_clone().unwrap()))
        .stderr(Stdio::from(input));
    // SAFETY: only async-signal-safe system calls run before exec; stdin is the PTY slave.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().unwrap();
    // SAFETY: fcntl changes only this valid owned PTY descriptor's file status flags.
    unsafe {
        let flags = libc::fcntl(tty.as_raw_fd(), libc::F_GETFL);
        assert_ne!(
            libc::fcntl(tty.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK),
            -1
        );
    }
    let mut session = Session { child, tty, root };
    let output = session.output_until("cancel");
    assert!(
        output.contains("+ step"),
        "capture must open the expanded page: {output}"
    );
    assert!(
        output.contains("desk"),
        "archived launch falls back to desk: {output}"
    );
    assert!(!output.contains("would you like to unarchive"));
    assert_eq!(
        fs::read(store.path().join("reopen.json")).unwrap(),
        request,
        "popup must not retire board requests"
    );
    ReopenRequest::new(None).write(store.path()).unwrap();
    let fresh = fs::read(store.path().join("reopen.json")).unwrap();
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(fs::read(store.path().join("reopen.json")).unwrap(), fresh);
    session.tty.write_all(b"\x1b[27u").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut drain = [0; 8192];
        let _ = session.tty.read(&mut drain);
        if let Some(status) = session.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "one Escape must exit real capture loop"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        store.load().unwrap().tasks().len(),
        1,
        "cancel saves no task"
    );
}
