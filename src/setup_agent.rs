//! Install the embedded agent skill into a host skills directory.

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli::guide::SKILL_MD;

const SKILL_FOLDER: &str = "tsk-cli";
const SKILL_FILE: &str = "SKILL.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Claude,
    Pi,
    Cursor,
    Codex,
    SkillDir(PathBuf),
}

impl Target {
    pub fn name(&self) -> &str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::Cursor => "cursor",
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
            Self::Codex => Ok(home_dir()?.join(".agents/skills")),
        }
    }

    pub fn skill_path(&self) -> Result<PathBuf, Error> {
        Ok(self.skills_root()?.join(SKILL_FOLDER).join(SKILL_FILE))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    List {
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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(reason) => write!(f, "{reason}"),
            Self::Exists(_) => write!(f, "skill-exists"),
            Self::Symlink(path) => write!(f, "refusing symlink: {}", path.display()),
            Self::Home => write!(f, "HOME is not set"),
            Self::Io(detail) => write!(f, "{detail}"),
        }
    }
}

pub fn parse(args: &[String]) -> Result<Command, Error> {
    let tail = args.get(2..).unwrap_or(&[]);
    if tail.iter().any(|arg| arg == "--help") {
        return Ok(Command::Help);
    }

    let mut herdr = false;
    let mut force = false;
    let mut json = false;
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
            "claude" => push_agent(&mut agents, Target::Claude)?,
            "pi" => push_agent(&mut agents, Target::Pi)?,
            "cursor" => push_agent(&mut agents, Target::Cursor)?,
            "codex" => push_agent(&mut agents, Target::Codex)?,
            "--force" => force = true,
            "--json" => json = true,
            "--skill-dir" => {
                index += 1;
                let Some(value) = tail.get(index) else {
                    return Err(usage());
                };
                if value.starts_with('-') || skill_dir.is_some() {
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

    let named = agents.len() + usize::from(skill_dir.is_some());
    if herdr {
        if named > 0 || force || json {
            return Err(usage());
        }
        return Ok(Command::Herdr);
    }
    if named > 1 {
        return Err(usage());
    }
    if named == 0 {
        if force {
            return Err(usage());
        }
        return Ok(Command::List { json });
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

pub fn install(target: &Target, force: bool) -> Result<PathBuf, Error> {
    let root = target.skills_root()?;
    refuse_symlink(&root)?;
    let dest = root.join(SKILL_FOLDER).join(SKILL_FILE);
    if dest.exists() && !force {
        return Err(Error::Exists(dest));
    }
    fs::create_dir_all(dest.parent().unwrap_or(root.as_path())).map_err(io_error)?;
    refuse_symlink(&root)?;
    fs::write(&dest, SKILL_MD).map_err(io_error)?;
    Ok(dest)
}

pub fn list_text() -> String {
    let home = env::var("HOME").ok().filter(|value| !value.is_empty());
    let display = |suffix: &str| match &home {
        Some(home) => format!("{home}/{suffix}/tsk-cli/SKILL.md"),
        None => format!("$HOME/{suffix}/tsk-cli/SKILL.md"),
    };
    format!(
        "usage: tsk setup herdr | claude | pi | cursor | codex | --skill-dir <path> [--force] [--json]\n\n\
         herdr     register the plugin and shortcuts for this installed binary\n\
         claude    {}\n\
         pi        {}\n\
         cursor    {}\n\
         codex     {}\n\
         --skill-dir <path>  write <path>/tsk-cli/SKILL.md\n",
        display(".claude/skills"),
        display(".pi/agent/skills"),
        display(".cursor/skills"),
        display(".agents/skills"),
    )
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
    Error::Usage(
        "usage: tsk setup herdr | claude | pi | cursor | codex | --skill-dir <path> [--force] [--json]"
            .into(),
    )
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
