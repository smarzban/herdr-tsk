//! Installer-aware binary upgrades.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const INSTALLER_URL: &str = "https://gettsk.sh/install.sh";
const CURL_PATH: &str = "/usr/bin/curl";
const CURL_ENV: &str = "TSK_UPDATE_CURL";
const SH_PATH: &str = "/bin/sh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOutcome {
    Homebrew,
    Installed,
}

/// Update the running installation. Homebrew owns its formula upgrades; all other
/// installations use the same published-release installer shown in the docs.
pub fn run() -> Result<UpdateOutcome, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the running tsk executable: {error}"))?;
    let curl = configured_curl_path(std::env::var_os(CURL_ENV).map(PathBuf::from))?;
    run_for(&executable, &curl, Path::new(SH_PATH))
}

fn configured_curl_path(configured: Option<PathBuf>) -> Result<PathBuf, String> {
    match configured {
        Some(path) if path.is_absolute() => Ok(path),
        Some(_) => Err(format!("{CURL_ENV} must be an absolute path")),
        None => Ok(PathBuf::from(CURL_PATH)),
    }
}

fn run_for(executable: &Path, curl: &Path, shell: &Path) -> Result<UpdateOutcome, String> {
    let executable = normalized(executable);
    if is_homebrew_install(&executable) {
        return Ok(UpdateOutcome::Homebrew);
    }
    let install_dir = executable
        .parent()
        .ok_or_else(|| "the running tsk executable has no installation directory".to_string())?;
    run_installer(install_dir, curl, shell)?;
    Ok(UpdateOutcome::Installed)
}

fn is_homebrew_install(executable: &Path) -> bool {
    let executable = normalized(executable);
    executable.ancestors().any(|path| {
        path.file_name().is_some_and(|name| name == "tsk")
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "Cellar")
    })
}

fn run_installer(install_dir: &Path, curl: &Path, shell: &Path) -> Result<(), String> {
    let mut download = Command::new(curl)
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "-fsSL",
            INSTALLER_URL,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start the installer download: {error}"))?;
    let stdout = download
        .stdout
        .take()
        .expect("curl stdout is piped before it starts");
    let mut installer = match Command::new(shell)
        .env("TSK_INSTALL_DIR", install_dir)
        .stdin(Stdio::from(stdout))
        .spawn()
    {
        Ok(installer) => installer,
        Err(error) => {
            let _ = download.kill();
            let _ = download.wait();
            return Err(format!("could not start the installer: {error}"));
        }
    };

    let installer_status = installer
        .wait()
        .map_err(|error| format!("could not wait for the installer: {error}"))?;
    let download_status = download
        .wait()
        .map_err(|error| format!("could not wait for the installer download: {error}"))?;
    if !installer_status.success() {
        return Err(format!(
            "installer failed (exit {})",
            exit_label(installer_status.code())
        ));
    }
    if !download_status.success() {
        return Err(format!(
            "could not download {INSTALLER_URL} (exit {})",
            exit_label(download_status.code())
        ));
    }
    Ok(())
}

fn exit_label(code: Option<i32>) -> String {
    code.map_or_else(|| "signal".to_string(), |code| code.to_string())
}

fn normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{configured_curl_path, is_homebrew_install, run_for, UpdateOutcome};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tsk-update-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create temporary directory");
        dir
    }

    fn command(dir: &Path, name: &str, source: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, source).expect("write test command");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .expect("make test command executable");
        }
        path
    }

    #[test]
    fn homebrew_install_is_identified_without_brew_on_path() {
        assert!(is_homebrew_install(Path::new(
            "/opt/homebrew/Cellar/tsk/0.7.0/bin/tsk"
        )));
        assert!(is_homebrew_install(Path::new(
            "/home/linuxbrew/.linuxbrew/Cellar/tsk/0.7.0/bin/tsk"
        )));
        assert!(!is_homebrew_install(Path::new(
            "/Users/alex/.local/bin/tsk"
        )));
    }

    #[test]
    fn nonstandard_curl_path_must_be_explicit_and_absolute() {
        assert_eq!(
            configured_curl_path(Some(PathBuf::from("/opt/tools/curl"))),
            Ok(PathBuf::from("/opt/tools/curl"))
        );
        assert_eq!(
            configured_curl_path(Some(PathBuf::from("curl"))),
            Err("TSK_UPDATE_CURL must be an absolute path".into())
        );
    }

    #[test]
    fn installer_uses_explicit_tools_and_preserves_the_running_install_directory() {
        let dir = temp_dir("installer");
        let log = dir.join("installer-input");
        let installed_to = dir.join("installer-destination");
        let curl = command(&dir, "curl", "#!/bin/sh\nprintf installer-payload\n");
        let shell = command(
            &dir,
            "sh",
            &format!(
                "#!/bin/sh\ncat > '{}'\nprintf '%s' \"$TSK_INSTALL_DIR\" > '{}'\n",
                log.display(),
                installed_to.display()
            ),
        );
        let executable = dir.join("custom/bin/tsk");
        fs::create_dir_all(executable.parent().expect("executable parent"))
            .expect("create custom install directory");

        assert_eq!(
            run_for(&executable, &curl, &shell),
            Ok(UpdateOutcome::Installed)
        );
        assert_eq!(
            fs::read_to_string(&log).expect("installer received download"),
            "installer-payload"
        );
        assert_eq!(
            fs::read_to_string(&installed_to).expect("installer destination"),
            executable
                .parent()
                .expect("executable parent")
                .display()
                .to_string()
        );
        let _ = fs::remove_dir_all(dir);
    }
}
