//! Explicit, user-approved registration of the installed binary with Herdr.
use std::{
    collections::hash_map::DefaultHasher,
    env, fs,
    hash::{Hash, Hasher},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::Command,
};
use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table};

const BINDINGS: [(&str, &str); 2] = [
    ("prefix+t", "herdr-tsk.open-board"),
    ("prefix+a", "herdr-tsk.quick-capture"),
];
fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
fn same(raw: &str, key: &str) -> bool {
    raw.trim().strip_prefix("prefix+").map(str::trim) == key.strip_prefix("prefix+")
}
fn has(item: &Item, key: &str) -> bool {
    item.as_str().is_some_and(|s| same(s, key))
        || item
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str().is_some_and(|s| same(s, key))))
}
fn remove_binding(item: &mut Item, key: &str) -> bool {
    if let Some(array) = item.as_array_mut() {
        array.retain(|v| !v.as_str().is_some_and(|s| same(s, key)));
        !array.is_empty()
    } else {
        *item = value("");
        false
    }
}

/// Plan all edits before any filesystem or registration side effects.
pub fn edit_bindings(
    source: &str,
    interactive: bool,
    mut confirm: impl FnMut(&str, &str) -> io::Result<bool>,
) -> io::Result<String> {
    let mut doc = source
        .parse::<DocumentMut>()
        .map_err(|e| error(format!("invalid Herdr TOML: {e}")))?;
    if doc.get("keys").is_none() {
        doc["keys"] = Item::Table(Table::new());
    }
    if let Some(inline) = doc["keys"].as_inline_table().cloned() {
        doc["keys"] = Item::Table(inline.into_table());
    }
    let keys = doc["keys"]
        .as_table_mut()
        .ok_or_else(|| error("keys must be a table"))?;
    if let Some(array) = keys.get("command").and_then(Item::as_array) {
        let mut commands = ArrayOfTables::new();
        for entry in array.iter() {
            commands.push(
                entry
                    .as_inline_table()
                    .ok_or_else(|| error("keys.command must contain tables"))?
                    .clone()
                    .into_table(),
            );
        }
        keys["command"] = Item::ArrayOfTables(commands);
    }
    if keys.get("command").is_none() {
        keys["command"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    if keys["command"].as_array_of_tables().is_none() {
        return Err(error("keys.command must be an array of tables"));
    }
    for (key, action) in BINDINGS {
        let builtin: Vec<String> = keys
            .iter()
            .filter(|(name, item)| *name != "prefix" && *name != "command" && has(item, key))
            .map(|(name, _)| name.to_owned())
            .collect();
        let commands = keys["command"].as_array_of_tables().unwrap();
        let matched: Vec<_> = commands
            .iter()
            .filter(|t| t.get("key").is_some_and(|v| has(v, key)))
            .collect();
        let correct = |t: &&Table| {
            t.get("type").and_then(Item::as_str) == Some("plugin_action")
                && t.get("command").and_then(Item::as_str) == Some(action)
        };
        let conflicts = !builtin.is_empty() || matched.iter().any(|t| !correct(t));
        if conflicts {
            if !interactive {
                return Err(error(format!("{key} is already assigned; rerun setup in an interactive terminal to choose whether to replace it (no changes made)")));
            }
            let detail = format!(
                "{}{}",
                builtin
                    .iter()
                    .map(|k| format!("keys.{k} = {}\n", keys[k]))
                    .collect::<String>(),
                matched.iter().map(|t| t.to_string()).collect::<String>()
            );
            if !confirm(key, &detail)? {
                continue;
            }
        } else if matched.len() == 1 {
            continue;
        }
        for name in builtin {
            remove_binding(&mut keys[&name], key);
        }
        let commands = keys["command"].as_array_of_tables_mut().unwrap();
        let mut remove = Vec::new();
        for (index, table) in commands.iter_mut().enumerate() {
            if table.get("key").is_some_and(|v| has(v, key))
                && !remove_binding(&mut table["key"], key)
            {
                remove.push(index);
            }
        }
        for index in remove.into_iter().rev() {
            commands.remove(index);
        }
        let mut command = Table::new();
        command["key"] = value(key);
        command["type"] = value("plugin_action");
        command["command"] = value(action);
        commands.push(command);
    }
    Ok(doc.to_string())
}

fn absolute(path: PathBuf) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()?.join(path))
    }
}
fn config_path() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("HERDR_CONFIG_PATH") {
        return absolute(path.into());
    }
    let root = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| error("HOME or XDG_CONFIG_HOME is required"))?;
    absolute(root.join("herdr/config.toml"))
}
/// Preserve the invocation symlink (e.g. Homebrew's bin/tsk), not its versioned Cellar target.
fn installed_binary() -> io::Result<PathBuf> {
    let invoked = PathBuf::from(
        env::args_os()
            .next()
            .ok_or_else(|| error("missing executable path"))?,
    );
    let candidate = if invoked.components().count() > 1 || invoked.is_absolute() {
        absolute(invoked)?
    } else {
        env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(&invoked))
            .find(|p| p.is_file())
            .ok_or_else(|| error("could not locate tsk on PATH"))?
    };
    let candidate = absolute(candidate)?;
    if fs::canonicalize(&candidate)? != fs::canonicalize(env::current_exe()?)? {
        return Err(error("invoked tsk path does not match running binary"));
    }
    Ok(candidate)
}

fn managed_assets(binary: &Path) -> io::Result<Vec<(&'static str, String)>> {
    let path = binary
        .to_str()
        .ok_or_else(|| error("installed binary path must be UTF-8"))?;
    let mut manifest = include_str!("../herdr-plugin.toml")
        .parse::<DocumentMut>()
        .map_err(|e| error(e.to_string()))?;
    manifest.remove("build");
    manifest["min_herdr_version"] = value("0.9.0");
    manifest["version"] = value(env!("CARGO_PKG_VERSION"));
    let mut command = Array::new();
    command.push(path);
    manifest["panes"]
        .as_array_of_tables_mut()
        .unwrap()
        .get_mut(0)
        .unwrap()["command"] = value(command);
    let board = include_str!("../scripts/open-board.sh");
    let quoted = format!("'{}'", path.replace('\'', "'\\''"));
    let board = board
        .lines()
        .map(|line| {
            if line.starts_with("plugin_bin=") {
                format!("plugin_bin={quoted}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    Ok(vec![
        ("herdr-plugin.toml", manifest.to_string()),
        ("scripts/open-board.sh", board),
        (
            "scripts/open-capture.sh",
            include_str!("../scripts/open-capture.sh").to_string(),
        ),
    ])
}
fn no_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            Err(error(format!("refusing symlink: {}", path.display())))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
fn read_config(path: &Path) -> io::Result<Option<String>> {
    no_symlink(path)?;
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn write_new(path: &Path, contents: &str) -> io::Result<()> {
    use std::fs::OpenOptions;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}
struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn herdr(args: &[&str], config: &Path) -> io::Result<()> {
    let output = Command::new("herdr")
        .args(args)
        .env("HERDR_CONFIG_PATH", config)
        .output()
        .map_err(|e| error(format!("could not run herdr: {e}")))?;
    if !output.status.success() {
        return Err(error(format!(
            "herdr {} failed: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

pub fn run() -> io::Result<()> {
    let config = config_path()?;
    let before = read_config(&config)?;
    let interactive = io::stdin().is_terminal() && io::stderr().is_terminal();
    let edited = edit_bindings(
        before.as_deref().unwrap_or(""),
        interactive,
        |key, detail| {
            for line in detail.lines() {
                eprintln!("{}", crate::ui::terminal_text(line));
            }
            eprint!("Replace the existing {key} binding? [y/N] ");
            io::stderr().flush()?;
            let mut response = String::new();
            if io::stdin().read_line(&mut response)? == 0 {
                return Err(error("confirmation ended; no changes made"));
            }
            Ok(matches!(
                response.trim().to_ascii_lowercase().as_str(),
                "y" | "yes"
            ))
        },
    )?;
    let assets = managed_assets(&installed_binary()?)?;
    // Check Herdr before creating any files. config check below validates the full candidate.
    herdr(&["--version"], &config)?;
    let parent = config
        .parent()
        .ok_or_else(|| error("config has no parent directory"))?;
    no_symlink(parent)?;
    fs::create_dir_all(parent)?;
    let lock = parent.join(".tsk-setup.lock");
    write_new(&lock, "tsk setup in progress\n").map_err(|e| {
        error(format!(
            "could not lock setup (remove stale {} only after checking no setup runs): {e}",
            lock.display()
        ))
    })?;
    let _lock = RemoveOnDrop(lock);
    if read_config(&config)? != before {
        return Err(error("Herdr config changed during setup; retry"));
    }
    let staged = parent.join(format!(".tsk-config-{}.toml", uuid::Uuid::new_v4()));
    write_new(&staged, &edited)?;
    let _staged = RemoveOnDrop(staged.clone());
    herdr(&["config", "check"], &staged)?;
    let mut hash = DefaultHasher::new();
    assets.hash(&mut hash);
    let base = parent.join("tsk-plugins");
    no_symlink(&base)?;
    crate::fsperm::ensure_private_dir(&base)?;
    let root = base.join(format!("{:016x}", hash.finish()));
    no_symlink(&root)?;
    crate::fsperm::ensure_private_dir(&root)?;
    for (name, contents) in assets {
        let path = root.join(name);
        no_symlink(&path)?;
        no_symlink(path.parent().unwrap())?;
        fs::create_dir_all(path.parent().unwrap())?;
        if path.exists() {
            if fs::read_to_string(&path)? != contents {
                return Err(error(
                    "managed plugin assets were modified; refusing overwrite",
                ));
            }
        } else {
            write_new(&path, &contents)?;
        }
    }
    if read_config(&config)? != before {
        return Err(error("Herdr config changed during setup; retry"));
    }
    // Registration failure leaves the user's config intact. Asset files are harmless until linked.
    herdr(
        &[
            "plugin",
            "link",
            root.to_str()
                .ok_or_else(|| error("plugin path must be UTF-8"))?,
        ],
        &config,
    )?;
    if read_config(&config)? != before {
        return Err(error(
            "plugin registered, but config changed; shortcuts not written, rerun setup",
        ));
    }
    if before.as_deref() != Some(&edited) {
        if let Some(original) = &before {
            let backup = parent.join(format!("config.toml.tsk-backup-{}", uuid::Uuid::new_v4()));
            write_new(&backup, original)?;
            println!(
                "Herdr config backup: {}",
                crate::ui::terminal_text(&backup.display().to_string())
            );
        }
        fs::rename(&staged, &config).map_err(|e| {
            error(format!(
                "plugin registered but shortcuts were not saved: {e}; rerun setup"
            ))
        })?;
    }
    println!(
        "Herdr plugin registered, using {}",
        crate::ui::terminal_text(&installed_binary()?.display().to_string())
    );
    println!("Configured available shortcuts: prefix+t board, prefix+a quick capture. Declined conflicts were left unchanged.");
    println!("Reload Herdr configuration (herdr server reload-config), or restart Herdr, to apply shortcuts.");
    Ok(())
}
