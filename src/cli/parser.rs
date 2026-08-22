//! Argument parsing for the `add` command.

use std::path::PathBuf;

/// Parsed add input. A plan source is selected by `file` or piped stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagAdd {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub project: Option<String>,
    pub global: bool,
    pub json: bool,
    pub state_dir: Option<PathBuf>,
    pub file: Option<PathBuf>,
    pub has_item_flags: bool,
    pub help: bool,
}

/// Parse `herdr-tasks add` arguments, including argv0 and the `add` subcommand.
pub fn parse_flag_add(args: &[String]) -> Result<FlagAdd, String> {
    if args.get(1).map(String::as_str) != Some("add") {
        return Err("expected add command".into());
    }

    let mut parsed = FlagAdd {
        title: None,
        notes: None,
        project: None,
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
            "--global" => {
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
        return Err("--global cannot be used with --project".into());
    }
    if parsed.has_item_flags && parsed.file.is_some() {
        return Err("item flags cannot be used with --file".into());
    }
    Ok(parsed)
}
