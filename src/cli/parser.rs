//! Argument parsing for the `add` command.

use std::path::PathBuf;

use uuid::Uuid;

use super::steps::StepsAction;
use crate::domain::normalize_thread;

/// A direct task operand, either the internal UUID or its human task number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAddress {
    Id(Uuid),
    Number(u64),
}

impl TaskAddress {
    pub fn matches(self, task: &crate::domain::Task) -> bool {
        match self {
            Self::Id(id) => task.id == id,
            Self::Number(number) => task.number == Some(number),
        }
    }
}

/// Parse a task UUID, bare-decimal human number, or the displayed `T<number>` form.
pub fn parse_task_address(value: &str) -> Result<TaskAddress, String> {
    let number = value
        .strip_prefix('T')
        .or_else(|| value.strip_prefix('t'))
        .unwrap_or(value);
    if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
        return number
            .parse::<u64>()
            .map(TaskAddress::Number)
            .map_err(|_| format!("invalid task id {value}"));
    }
    Uuid::parse_str(value)
        .map(TaskAddress::Id)
        .map_err(|_| format!("invalid task id {value}"))
}

/// Parsed `trash` input. Positionals are the action (`restore`) and the task address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagTrash {
    pub action: Option<TrashAction>,
    pub state_dir: Option<PathBuf>,
    pub help: bool,
}

/// The parsed `trash` action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashAction {
    Restore { target: TaskAddress },
}

/// Parse `tsk trash` arguments, including argv0 and the `trash` subcommand.
/// Parsed `archive` / `unarchive` input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagArchive {
    pub task: Option<TaskAddress>,
    pub state_dir: Option<PathBuf>,
    pub help: bool,
}

/// Parse `tsk archive <task>` / `tsk unarchive <task>` arguments, including argv0.
pub fn parse_flag_archive(args: &[String], verb: &str) -> Result<FlagArchive, String> {
    if args.get(1).map(String::as_str) != Some(verb) {
        return Err(format!("expected {verb} command"));
    }

    let mut parsed = FlagArchive {
        task: None,
        state_dir: None,
        help: false,
    };
    let mut index = 2;
    while let Some(flag) = args.get(index).map(String::as_str) {
        let value = |name: &str| match args.get(index + 1) {
            Some(value) if !value.starts_with('-') => Ok(value.clone()),
            _ => Err(format!("missing value for {name}")),
        };
        match flag {
            "--help" => {
                parsed.help = true;
                index += 1;
            }
            flag if flag.starts_with("--state-dir=") => {
                parsed.state_dir = Some(PathBuf::from(flag["--state-dir=".len()..].to_owned()));
                index += 1;
            }
            "--state-dir" => {
                parsed.state_dir = Some(PathBuf::from(value(flag)?));
                index += 2;
            }
            flag if flag.starts_with('-') => return Err(format!("unknown {verb} argument {flag}")),
            flag => {
                if parsed.task.is_some() {
                    return Err(format!("unknown {verb} argument {flag}"));
                }
                parsed.task = Some(parse_task_address(flag)?);
                index += 1;
            }
        }
    }
    Ok(parsed)
}

pub fn parse_flag_trash(args: &[String]) -> Result<FlagTrash, String> {
    if args.get(1).map(String::as_str) != Some("trash") {
        return Err("expected trash command".into());
    }

    let mut parsed = FlagTrash {
        action: None,
        state_dir: None,
        help: false,
    };
    let mut positionals: Vec<&str> = Vec::new();
    let mut index = 2;
    while let Some(flag) = args.get(index).map(String::as_str) {
        let value = |name: &str| match args.get(index + 1) {
            Some(value) if !value.starts_with('-') => Ok(value.clone()),
            _ => Err(format!("missing value for {name}")),
        };
        match flag {
            "--help" => {
                parsed.help = true;
                index += 1;
            }
            flag if flag.starts_with("--state-dir=") => {
                parsed.state_dir = Some(PathBuf::from(flag["--state-dir=".len()..].to_owned()));
                index += 1;
            }
            "--state-dir" => {
                parsed.state_dir = Some(PathBuf::from(value(flag)?));
                index += 2;
            }
            flag if flag.starts_with('-') => return Err(format!("unknown trash argument {flag}")),
            positional => {
                if positionals.len() == 2 {
                    return Err(format!("unexpected trash argument {positional}"));
                }
                positionals.push(positional);
                index += 1;
            }
        }
    }

    if parsed.help {
        return Ok(parsed);
    }
    match positionals.as_slice() {
        [] => {}
        ["restore", address] => {
            parsed.action = Some(TrashAction::Restore {
                target: parse_task_address(address)?,
            });
        }
        [action, _] => return Err(format!("unknown trash action {action}")),
        [action] => {
            return Err(match *action {
                "restore" => "task id is required".into(),
                other => format!("unknown trash action {other}"),
            })
        }
        _ => unreachable!("positionals are capped at two"),
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{parse_flag_trash, parse_task_address, TaskAddress, TrashAction};

    #[test]
    fn task_addresses_accept_the_displayed_identifier_case_insensitively() {
        assert_eq!(parse_task_address("T30"), Ok(TaskAddress::Number(30)));
        assert_eq!(parse_task_address("t30"), Ok(TaskAddress::Number(30)));
        assert_eq!(parse_task_address("30"), Ok(TaskAddress::Number(30)));
        assert!(parse_task_address("T-30").is_err());
    }

    #[test]
    fn trash_parse_accepts_restore_with_number_and_flags() {
        let parsed = parse_flag_trash(&[
            "tsk".into(),
            "trash".into(),
            "restore".into(),
            "T7".into(),
            "--state-dir".into(),
            "/tmp/dir".into(),
        ])
        .expect("parse");
        assert_eq!(
            parsed.action,
            Some(TrashAction::Restore {
                target: TaskAddress::Number(7)
            })
        );
        assert_eq!(
            parsed.state_dir.as_deref(),
            Some(std::path::Path::new("/tmp/dir"))
        );

        let parsed =
            parse_flag_trash(&["tsk".into(), "trash".into(), "--help".into()]).expect("parse help");
        assert!(parsed.help);

        assert!(parse_flag_trash(&["tsk".into(), "trash".into(), "restore".into()]).is_err());
        assert!(
            parse_flag_trash(&["tsk".into(), "trash".into(), "bogus".into(), "T1".into()]).is_err()
        );
        assert!(parse_flag_trash(&[
            "tsk".into(),
            "trash".into(),
            "restore".into(),
            "T1".into(),
            "extra".into()
        ])
        .is_err());
    }
}

/// Parsed add input. A plan source is selected by `file` or piped stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagAdd {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub project: Option<String>,
    /// Normalized at the argv boundary so add only receives valid thread names.
    pub thread: Option<String>,
    pub global: bool,
    pub json: bool,
    pub state_dir: Option<PathBuf>,
    pub file: Option<PathBuf>,
    pub has_item_flags: bool,
    pub help: bool,
}

/// Parse `tsk add` arguments, including argv0 and the `add` subcommand.
pub fn parse_flag_add(args: &[String]) -> Result<FlagAdd, String> {
    if args.get(1).map(String::as_str) != Some("add") {
        return Err("expected add command".into());
    }

    let mut parsed = FlagAdd {
        title: None,
        notes: None,
        project: None,
        thread: None,
        global: false,
        json: false,
        state_dir: None,
        file: None,
        has_item_flags: false,
        help: false,
    };
    let mut index = 2;
    while let Some(flag) = args.get(index).map(String::as_str) {
        let value = |name: &str| match args.get(index + 1) {
            Some(value) if !value.starts_with('-') => Ok(value.clone()),
            _ => Err(format!("missing value for {name}")),
        };
        let file_value = |name: &str| match args.get(index + 1) {
            Some(value) if value == "-" || !value.starts_with('-') => Ok(value.clone()),
            _ => Err(format!("missing value for {name}")),
        };
        match flag {
            flag if flag.starts_with("--title=") => {
                parsed.title = Some(flag["--title=".len()..].to_owned());
                parsed.has_item_flags = true;
                index += 1;
            }
            flag if flag.starts_with("--notes=") => {
                parsed.notes = Some(flag["--notes=".len()..].to_owned());
                parsed.has_item_flags = true;
                index += 1;
            }
            flag if flag.starts_with("--project=") => {
                parsed.project = Some(flag["--project=".len()..].to_owned());
                parsed.has_item_flags = true;
                index += 1;
            }
            flag if flag.starts_with("--thread=") => {
                parsed.thread = Some(
                    normalize_thread(&flag["--thread=".len()..])
                        .map_err(|_| "invalid thread name".to_owned())?,
                );
                parsed.has_item_flags = true;
                index += 1;
            }
            "-t" | "--title" => {
                parsed.title = Some(value(flag)?);
                parsed.has_item_flags = true;
                index += 2;
            }
            "-n" | "--notes" => {
                parsed.notes = Some(value(flag)?);
                parsed.has_item_flags = true;
                index += 2;
            }
            "-p" | "--project" => {
                parsed.project = Some(value(flag)?);
                parsed.has_item_flags = true;
                index += 2;
            }
            "--thread" => {
                parsed.thread = Some(
                    normalize_thread(&value(flag)?)
                        .map_err(|_| "invalid thread name".to_owned())?,
                );
                parsed.has_item_flags = true;
                index += 2;
            }
            "--desk" => {
                parsed.global = true;
                parsed.has_item_flags = true;
                index += 1;
            }
            "--json" => {
                parsed.json = true;
                index += 1;
            }
            "--help" => {
                parsed.help = true;
                index += 1;
            }
            flag if flag.starts_with("--state-dir=") => {
                parsed.state_dir = Some(PathBuf::from(flag["--state-dir=".len()..].to_owned()));
                index += 1;
            }
            flag if flag.starts_with("--file=") => {
                parsed.file = Some(PathBuf::from(flag["--file=".len()..].to_owned()));
                index += 1;
            }
            "--state-dir" => {
                parsed.state_dir = Some(PathBuf::from(value(flag)?));
                index += 2;
            }
            "--file" => {
                parsed.file = Some(PathBuf::from(file_value(flag)?));
                index += 2;
            }
            _ => return Err(format!("unknown add argument {flag}")),
        }
    }

    if parsed.global && parsed.project.is_some() {
        return Err("--desk cannot be used with --project".into());
    }
    if parsed.has_item_flags && parsed.file.is_some() {
        return Err("item flags cannot be used with --file".into());
    }
    Ok(parsed)
}

/// Parsed `steps` input. Flags come first; the positionals are task id, action,
/// and the action's operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagSteps {
    pub task: Option<TaskAddress>,
    pub action: Option<StepsAction>,
    pub state_dir: Option<PathBuf>,
    pub help: bool,
}

/// Parse `tsk steps` arguments, including argv0 and the `steps` subcommand.
///
/// Flags may appear anywhere; the positionals in order are task id, action, and
/// the action's operand.
pub fn parse_flag_steps(args: &[String]) -> Result<FlagSteps, String> {
    if args.get(1).map(String::as_str) != Some("steps") {
        return Err("expected steps command".into());
    }

    let mut parsed = FlagSteps {
        task: None,
        action: None,
        state_dir: None,
        help: false,
    };
    let mut positionals: Vec<&str> = Vec::new();
    let mut index = 2;
    while let Some(flag) = args.get(index).map(String::as_str) {
        let value = |name: &str| match args.get(index + 1) {
            Some(value) if !value.starts_with('-') => Ok(value.clone()),
            _ => Err(format!("missing value for {name}")),
        };
        match flag {
            "--help" => {
                parsed.help = true;
                index += 1;
            }
            flag if flag.starts_with("--state-dir=") => {
                parsed.state_dir = Some(PathBuf::from(flag["--state-dir=".len()..].to_owned()));
                index += 1;
            }
            "--state-dir" => {
                parsed.state_dir = Some(PathBuf::from(value(flag)?));
                index += 2;
            }
            flag if flag.starts_with('-') => return Err(format!("unknown steps argument {flag}")),
            positional => {
                if positionals.len() == 3 {
                    return Err(format!("unexpected steps argument {positional}"));
                }
                positionals.push(positional);
                index += 1;
            }
        }
    }

    if !parsed.help {
        if positionals.len() < 2 {
            return Err(if positionals.is_empty() {
                "task id is required".into()
            } else {
                "steps action is required".into()
            });
        }
        let task = parse_task_address(positionals[0])?;
        let operand = positionals
            .get(2)
            .copied()
            .map(str::to_owned)
            .ok_or_else(|| match positionals[1] {
                "add" => "step text is required".to_owned(),
                "toggle" => "step short id is required".to_owned(),
                other => format!("unknown steps action {other}"),
            })?;
        parsed.action = Some(match positionals[1] {
            "add" => StepsAction::Add { text: operand },
            "toggle" => StepsAction::Toggle { short_id: operand },
            other => return Err(format!("unknown steps action {other}")),
        });
        parsed.task = Some(task);
    }
    Ok(parsed)
}
