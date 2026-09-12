//! Installer-aware binary upgrades.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const INSTALLER_URL: &str = "https://gettsk.sh/install.sh";

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
    if is_homebrew_install(&executable, homebrew_formula_prefix().as_deref()) {
        return Ok(UpdateOutcome::Homebrew);
    }

    run_installer()?;
    Ok(UpdateOutcome::Installed)
}

fn homebrew_formula_prefix() -> Option<PathBuf> {
    let output = Command::new("brew")
        .args(["--prefix", "tsk"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let prefix = String::from_utf8(output.stdout).ok()?;
    let prefix = PathBuf::from(prefix.trim());
    (!prefix.as_os_str().is_empty()).then_some(prefix)
}

fn run_installer() -> Result<(), String> {
    let mut download = Command::new("curl")
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
    let mut installer = match Command::new("sh").stdin(Stdio::from(stdout)).spawn() {
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

fn is_homebrew_install(executable: &Path, formula_prefix: Option<&Path>) -> bool {
    let Some(prefix) = formula_prefix else {
        return false;
    };
    normalized(executable).starts_with(normalized(prefix))
}

fn normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::is_homebrew_install;

    #[test]
    fn homebrew_install_is_identified_from_its_formula_prefix() {
        let prefix = Path::new("/opt/homebrew/Cellar/tsk/0.7.0");
        assert!(is_homebrew_install(
            Path::new("/opt/homebrew/Cellar/tsk/0.7.0/bin/tsk"),
            Some(prefix)
        ));
        assert!(!is_homebrew_install(
            Path::new("/Users/alex/.local/bin/tsk"),
            Some(prefix)
        ));
        assert!(!is_homebrew_install(
            Path::new("/Users/alex/.local/bin/tsk"),
            None
        ));
    }
}
