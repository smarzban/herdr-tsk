//! Install the embedded agent skill into a host skills directory.

use std::env;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::cli::guide::SKILL_MD;

const SKILL_FOLDER: &str = "tsk-cli";
const SKILL_FILE: &str = "SKILL.md";

pub const USAGE: &str = "usage: tsk setup [herdr | agents | claude | pi | cursor | grok | codex | --skill-dir <path>] [--yes] [--force] [--json]\n       tsk setup --detected-ids";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Claude,
    Pi,
    Cursor,
    Grok,
    Codex,
    SkillDir(PathBuf),
}

impl Target {
    pub fn name(&self) -> &str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::Cursor => "cursor",
            Self::Grok => "grok",
            Self::Codex => "codex",
            Self::SkillDir(_) => "skill-dir",
        }
    }

    pub fn skills_root(&self) -> Result<PathBuf, Error> {
        match self {
            Self::SkillDir(path) => Ok(path.clone()),
            Self::Claude => Ok(home_dir()?.join(".claude/skills")),
            Self::Pi => Ok(home_dir()?.join(".pi/agent/skills")),
            Self::Cursor => Ok(home_dir()?.join(".cursor/skills")),
            Self::Grok => Ok(home_dir()?.join(".grok/skills")),
            Self::Codex => Ok(home_dir()?.join(".agents/skills")),
        }
    }

    pub fn skill_path(&self) -> Result<PathBuf, Error> {
        Ok(self.skills_root()?.join(SKILL_FOLDER).join(SKILL_FILE))
    }

    fn named_agents() -> [Target; 5] {
        [
            Target::Claude,
            Target::Pi,
            Target::Cursor,
            Target::Grok,
            Target::Codex,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillState {
    Missing,
    Current,
    Outdated,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatus {
    pub id: String,
    pub skills_root: PathBuf,
    pub skill_path: PathBuf,
    pub present: bool,
    pub installed_version: Option<String>,
    pub state: SkillState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Written(PathBuf),
    Updated {
        path: PathBuf,
        previous: Option<String>,
    },
}

impl InstallOutcome {
    pub fn path(&self) -> &Path {
        match self {
            Self::Written(path) => path,
            Self::Updated { path, .. } => path,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Written(_) => "written",
            Self::Updated { .. } => "updated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchResult {
    pub applied: Vec<(String, InstallOutcome)>,
    pub skipped_current: Vec<String>,
    pub declined: bool,
    pub none_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    List {
        json: bool,
    },
    DetectedIds,
    Interactive {
        json: bool,
    },
    AgentsYes {
        json: bool,
    },
    Herdr,
    Skill {
        target: Target,
        force: bool,
        json: bool,
    },
}

#[derive(Debug)]
pub enum Error {
    Usage(String),
    Exists(PathBuf),
    Symlink(PathBuf),
    Home,
    Io(String),
    Ended,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(reason) => write!(f, "{reason}"),
            Self::Exists(_) => write!(f, "skill-exists"),
            Self::Symlink(path) => write!(f, "refusing symlink: {}", path.display()),
            Self::Home => write!(f, "HOME is not set"),
            Self::Io(detail) => write!(f, "{detail}"),
            Self::Ended => write!(f, "confirmation ended; no changes made"),
        }
    }
}

pub fn parse(args: &[String]) -> Result<Command, Error> {
    let tail = args.get(2..).unwrap_or(&[]);
    if tail.iter().any(|arg| arg == "--help") {
        return Ok(Command::Help);
    }

    let mut herdr = false;
    let mut agents_cmd = false;
    let mut yes = false;
    let mut force = false;
    let mut json = false;
    let mut detected_ids = false;
    let mut agents: Vec<Target> = Vec::new();
    let mut skill_dir: Option<PathBuf> = None;
    let mut index = 0;
    while index < tail.len() {
        let arg = tail[index].as_str();
        match arg {
            "herdr" => {
                if herdr {
                    return Err(usage());
                }
                herdr = true;
            }
            "agents" => {
                if agents_cmd {
                    return Err(usage());
                }
                agents_cmd = true;
            }
            "claude" => push_agent(&mut agents, Target::Claude)?,
            "pi" => push_agent(&mut agents, Target::Pi)?,
            "cursor" => push_agent(&mut agents, Target::Cursor)?,
            "grok" => push_agent(&mut agents, Target::Grok)?,
            "codex" => push_agent(&mut agents, Target::Codex)?,
            "--yes" => yes = true,
            "--force" => force = true,
            "--json" => json = true,
            "--detected-ids" => detected_ids = true,
            "--skill-dir" => {
                index += 1;
                let Some(value) = tail.get(index) else {
                    return Err(usage());
                };
                if value.is_empty() || value.starts_with('-') || skill_dir.is_some() {
                    return Err(usage());
                }
                skill_dir = Some(PathBuf::from(value));
            }
            flag if flag.starts_with("--skill-dir=") => {
                let value = &flag["--skill-dir=".len()..];
                if value.is_empty() || skill_dir.is_some() {
                    return Err(usage());
                }
                skill_dir = Some(PathBuf::from(value));
            }
            _ => return Err(usage()),
        }
        index += 1;
    }

    if detected_ids {
        if herdr || agents_cmd || yes || force || json || !agents.is_empty() || skill_dir.is_some()
        {
            return Err(usage());
        }
        return Ok(Command::DetectedIds);
    }

    let named = agents.len() + usize::from(skill_dir.is_some());
    if herdr {
        if named > 0 || force || json || yes || agents_cmd {
            return Err(usage());
        }
        return Ok(Command::Herdr);
    }
    if agents_cmd {
        if named > 0 || force {
            return Err(usage());
        }
        if yes {
            return Ok(Command::AgentsYes { json });
        }
        return Ok(Command::Interactive { json });
    }
    if named > 1 {
        return Err(usage());
    }
    if named == 0 {
        if force || yes {
            return Err(usage());
        }
        return Ok(Command::List { json });
    }
    if yes {
        return Err(usage());
    }
    let target = if let Some(path) = skill_dir {
        Target::SkillDir(path)
    } else {
        agents.pop().expect("one named agent")
    };
    Ok(Command::Skill {
        target,
        force,
        json,
    })
}

/// Bare `tsk setup` becomes interactive on a TTY; otherwise lists guidance.
pub fn parse_bare(args: &[String], interactive: bool) -> Result<Command, Error> {
    let command = parse(args)?;
    match command {
        Command::List { json } if interactive && !json => Ok(Command::Interactive { json: false }),
        other => Ok(other),
    }
}

pub fn embedded_skill_version() -> String {
    frontmatter_version(SKILL_MD).expect("embedded SKILL.md must declare version")
}

pub fn frontmatter_version(md: &str) -> Option<String> {
    let rest = md.strip_prefix("---\n")?;
    let close = rest.find("\n---\n")?;
    let yaml = &rest[..close];
    for line in yaml.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("version:") else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        return normalize_version(value);
    }
    None
}

fn normalize_version(raw: &str) -> Option<String> {
    let value = raw.strip_prefix('v').unwrap_or(raw).trim();
    let mut parts = value.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if [major, minor, patch]
        .iter()
        .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    Some(format!("{major}.{minor}.{patch}"))
}

pub fn install(target: &Target, force: bool) -> Result<InstallOutcome, Error> {
    let root = target.skills_root()?;
    if root.as_os_str().is_empty() {
        return Err(usage());
    }
    let folder = root.join(SKILL_FOLDER);
    let dest = folder.join(SKILL_FILE);
    refuse_symlink(&root)?;
    refuse_symlink(&folder)?;
    refuse_symlink(&dest)?;

    let previous = if path_present(&dest)? {
        let existing = fs::read_to_string(&dest).map_err(io_error)?;
        let installed = frontmatter_version(&existing);
        if !force {
            if let Some(ref version) = installed {
                if version.as_str() == embedded_skill_version().as_str() {
                    return Err(Error::Exists(dest));
                }
            }
        }
        Some(installed)
    } else {
        None
    };

    let had_file = previous.is_some();
    let previous_version = previous.flatten();

    fs::create_dir_all(&folder).map_err(io_error)?;
    refuse_symlink(&root)?;
    refuse_symlink(&folder)?;
    refuse_symlink(&dest)?;
    fs::write(&dest, SKILL_MD).map_err(io_error)?;
    if had_file {
        Ok(InstallOutcome::Updated {
            path: dest,
            previous: previous_version,
        })
    } else {
        Ok(InstallOutcome::Written(dest))
    }
}

pub fn detect() -> Result<Vec<AgentStatus>, Error> {
    let home = home_dir()?;
    let embedded = embedded_skill_version();
    let mut out = Vec::new();
    for target in Target::named_agents() {
        let skills_root = target.skills_root()?;
        let skill_path = target.skill_path()?;
        if !agent_present(&home, &target, &skills_root) {
            continue;
        }
        let (installed_version, state) = match skill_status(&skills_root, &skill_path)? {
            SkillStatusDetail::Blocked => (None, SkillState::Blocked),
            SkillStatusDetail::Missing => (None, SkillState::Missing),
            SkillStatusDetail::Ready { version } => {
                let state = match &version {
                    Some(v) if v.as_str() == embedded.as_str() => SkillState::Current,
                    _ => SkillState::Outdated,
                };
                (version, state)
            }
        };
        out.push(AgentStatus {
            id: target.name().to_string(),
            skills_root,
            skill_path,
            present: true,
            installed_version,
            state,
        });
    }
    Ok(out)
}

pub fn detected_ids() -> Result<Vec<String>, Error> {
    Ok(detect()?.into_iter().map(|agent| agent.id).collect())
}

pub fn install_detected(force: bool) -> Result<BatchResult, Error> {
    let detected = detect()?;
    if detected.is_empty() {
        return Ok(BatchResult {
            applied: Vec::new(),
            skipped_current: Vec::new(),
            declined: false,
            none_detected: true,
        });
    }
    let mut applied = Vec::new();
    let mut skipped_current = Vec::new();
    for status in detected {
        if status.state == SkillState::Blocked {
            continue;
        }
        if !force && status.state == SkillState::Current {
            skipped_current.push(status.id);
            continue;
        }
        let target = named_target(&status.id)?;
        match install(&target, force) {
            Ok(outcome) => applied.push((status.id, outcome)),
            Err(Error::Exists(_)) => skipped_current.push(status.id),
            Err(error) => return Err(error),
        }
    }
    Ok(BatchResult {
        applied,
        skipped_current,
        declined: false,
        none_detected: false,
    })
}

pub fn run_interactive_batch(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    interactive: bool,
) -> Result<BatchResult, Error> {
    let detected = detect()?;
    if detected.is_empty() {
        let _ = write!(writer, "{}", list_text());
        let _ = writeln!(
            writer,
            "\nNo agent skill roots detected.\nRun `tsk setup herdr` to register the Herdr plugin."
        );
        return Ok(BatchResult {
            applied: Vec::new(),
            skipped_current: Vec::new(),
            declined: false,
            none_detected: true,
        });
    }

    let embedded = embedded_skill_version();
    let _ = writeln!(writer, "Detected agents:");
    let mut needs_work = Vec::new();
    let mut current = Vec::new();
    for status in &detected {
        let detail = match status.state {
            SkillState::Missing => "not installed".to_string(),
            SkillState::Current => format!("current v{embedded}"),
            SkillState::Outdated => match &status.installed_version {
                Some(v) => format!("v{v} → v{embedded}"),
                None => format!("update to v{embedded}"),
            },
            SkillState::Blocked => "blocked (symlink)".to_string(),
        };
        let _ = writeln!(
            writer,
            "  {:<8} {}  ({detail})",
            status.id,
            status.skills_root.display()
        );
        match status.state {
            SkillState::Missing | SkillState::Outdated => needs_work.push(status.id.clone()),
            SkillState::Current => current.push(status.id.clone()),
            SkillState::Blocked => {}
        }
    }

    if needs_work.is_empty() {
        let _ = writeln!(
            writer,
            "All detected agent skills are current (v{embedded})."
        );
        return Ok(BatchResult {
            applied: Vec::new(),
            skipped_current: current,
            declined: false,
            none_detected: false,
        });
    }

    if !interactive {
        let _ = writeln!(
            writer,
            "Skipping agent skill setup (no TTY). Run:\n  tsk setup\nor:\n  tsk setup agents --yes"
        );
        return Ok(BatchResult {
            applied: Vec::new(),
            skipped_current: current,
            declined: true,
            none_detected: false,
        });
    }

    let list = needs_work.join(", ");
    let _ = write!(writer, "Install or update the tsk skill for {list}? [y/N] ");
    let _ = writer.flush();
    let mut line = String::new();
    if reader.read_line(&mut line).map_err(io_error)? == 0 {
        return Err(Error::Ended);
    }
    let answer = line.trim();
    if !matches!(answer, "y" | "Y" | "yes" | "YES") {
        let _ = writeln!(writer, "Skipped agent skill setup. Run later:\n  tsk setup");
        return Ok(BatchResult {
            applied: Vec::new(),
            skipped_current: current,
            declined: true,
            none_detected: false,
        });
    }

    let mut applied = Vec::new();
    for id in &needs_work {
        let target = named_target(id)?;
        let outcome = install(&target, false)?;
        let _ = writeln!(writer, "  {} {}", outcome.kind(), outcome.path().display());
        applied.push((id.clone(), outcome));
    }
    Ok(BatchResult {
        applied,
        skipped_current: current,
        declined: false,
        none_detected: false,
    })
}

pub fn list_text() -> String {
    let home = env::var("HOME").ok().filter(|value| !value.is_empty());
    let display = |suffix: &str| match &home {
        Some(home) => format!("{home}/{suffix}/tsk-cli/SKILL.md"),
        None => format!("$HOME/{suffix}/tsk-cli/SKILL.md"),
    };
    format!(
        "{USAGE}\n\n\
         On a TTY, bare `tsk setup` detects agents and asks once to install or update.\n\
         herdr     register the plugin and shortcuts for this installed binary\n\
         agents    detect agents; --yes installs/updates without asking\n\
         claude    {}\n\
         pi        {}\n\
         cursor    {}\n\
         grok      {}\n\
         codex     {}\n\
         --skill-dir <path>  write <path>/tsk-cli/SKILL.md\n\
         --detected-ids      print detected agent ids (for installers)\n",
        display(".claude/skills"),
        display(".pi/agent/skills"),
        display(".cursor/skills"),
        display(".grok/skills"),
        display(".agents/skills"),
    )
}

pub fn detection_json() -> Result<String, Error> {
    let agents = detect()?;
    let payload = serde_json::json!({
        "outcome": "detected",
        "embedded_skill_version": embedded_skill_version(),
        "agents": agents.iter().map(|agent| serde_json::json!({
            "id": agent.id,
            "present": agent.present,
            "skills_root": agent.skills_root.display().to_string(),
            "skill_path": agent.skill_path.display().to_string(),
            "installed_version": agent.installed_version,
            "state": match agent.state {
                SkillState::Missing => "missing",
                SkillState::Current => "current",
                SkillState::Outdated => "outdated",
                SkillState::Blocked => "blocked-symlink",
            },
        })).collect::<Vec<_>>(),
    });
    Ok(format!("{payload}\n"))
}

fn named_target(id: &str) -> Result<Target, Error> {
    match id {
        "claude" => Ok(Target::Claude),
        "pi" => Ok(Target::Pi),
        "cursor" => Ok(Target::Cursor),
        "grok" => Ok(Target::Grok),
        "codex" => Ok(Target::Codex),
        _ => Err(usage()),
    }
}

fn agent_present(home: &Path, target: &Target, skills_root: &Path) -> bool {
    let marker = match target {
        Target::Claude => home.join(".claude"),
        Target::Pi => home.join(".pi"),
        Target::Cursor => home.join(".cursor"),
        Target::Grok => home.join(".grok"),
        Target::Codex => home.join(".codex"),
        Target::SkillDir(_) => return true,
    };
    real_dir(&marker)
        || real_dir(skills_root)
        || match target {
            Target::Claude => cli_on_path("claude"),
            Target::Cursor => cli_on_path("cursor"),
            Target::Codex => cli_on_path("codex"),
            Target::Pi | Target::Grok | Target::SkillDir(_) => false,
        }
}

fn real_dir(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.is_dir() && !meta.file_type().is_symlink(),
        Err(_) => false,
    }
}

fn cli_on_path(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| {
        let candidate = dir.join(name);
        match fs::symlink_metadata(&candidate) {
            Ok(meta) => meta.is_file(),
            Err(_) => false,
        }
    })
}

enum SkillStatusDetail {
    Missing,
    Blocked,
    Ready { version: Option<String> },
}

fn skill_status(root: &Path, dest: &Path) -> Result<SkillStatusDetail, Error> {
    if is_symlink(root)? || is_symlink(&root.join(SKILL_FOLDER))? || is_symlink(dest)? {
        return Ok(SkillStatusDetail::Blocked);
    }
    if !path_present(dest)? {
        return Ok(SkillStatusDetail::Missing);
    }
    let text = fs::read_to_string(dest).map_err(io_error)?;
    Ok(SkillStatusDetail::Ready {
        version: frontmatter_version(&text),
    })
}

fn is_symlink(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(meta.file_type().is_symlink()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn push_agent(agents: &mut Vec<Target>, target: Target) -> Result<(), Error> {
    if agents
        .iter()
        .any(|existing| existing.name() == target.name())
    {
        return Err(usage());
    }
    agents.push(target);
    Ok(())
}

fn usage() -> Error {
    Error::Usage(USAGE.into())
}

fn path_present(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn home_dir() -> Result<PathBuf, Error> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or(Error::Home)
}

fn refuse_symlink(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(Error::Symlink(path.to_path_buf())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

fn io_error(error: io::Error) -> Error {
    Error::Io(format!("could not write skill: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    #[test]
    fn embedded_skill_declares_semver() {
        let version = embedded_skill_version();
        assert_eq!(version, "1.0.0");
        assert_eq!(frontmatter_version(SKILL_MD).as_deref(), Some("1.0.0"));
    }

    #[test]
    fn normalize_strips_v_prefix() {
        assert_eq!(normalize_version("v1.2.3").as_deref(), Some("1.2.3"));
        assert!(normalize_version("1.2").is_none());
        assert!(normalize_version("1.2.3.4").is_none());
    }

    #[test]
    fn missing_version_parses_as_none() {
        let md = "---\nname: x\n---\n\nbody\n";
        assert_eq!(frontmatter_version(md), None);
    }

    #[test]
    fn interactive_batch_yes_installs_detected_agent() {
        let _lock = env_lock();
        let root =
            std::env::temp_dir().join(format!("tsk-setup-agent-batch-yes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home/.cursor")).expect("cursor");
        fs::create_dir_all(root.join("empty-bin")).expect("bin");
        let previous_home = std::env::var_os("HOME");
        let previous_path = std::env::var_os("PATH");
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("PATH", root.join("empty-bin"));
        let mut reader = Cursor::new(b"y\n".to_vec());
        let mut writer = Vec::new();
        let result = run_interactive_batch(&mut reader, &mut writer, true);
        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match previous_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        let result = result.expect("batch");
        assert!(!result.declined);
        assert_eq!(result.applied.len(), 1);
        assert_eq!(result.applied[0].0, "cursor");
        let dest = root.join("home/.cursor/skills/tsk-cli/SKILL.md");
        assert_eq!(fs::read_to_string(&dest).expect("written"), SKILL_MD);
        let transcript = String::from_utf8_lossy(&writer);
        assert!(
            transcript.contains("Install or update the tsk skill for cursor?"),
            "{transcript}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interactive_batch_no_skips_write() {
        let _lock = env_lock();
        let root =
            std::env::temp_dir().join(format!("tsk-setup-agent-batch-no-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home/.cursor")).expect("cursor");
        fs::create_dir_all(root.join("empty-bin")).expect("bin");
        let previous_home = std::env::var_os("HOME");
        let previous_path = std::env::var_os("PATH");
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("PATH", root.join("empty-bin"));
        let mut reader = Cursor::new(b"n\n".to_vec());
        let mut writer = Vec::new();
        let result = run_interactive_batch(&mut reader, &mut writer, true);
        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match previous_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        let result = result.expect("batch");
        assert!(result.declined);
        assert!(result.applied.is_empty());
        assert!(!root.join("home/.cursor/skills/tsk-cli/SKILL.md").exists());
        let transcript = String::from_utf8_lossy(&writer);
        assert!(transcript.contains("tsk setup"), "{transcript}");
        let _ = fs::remove_dir_all(root);
    }
}
