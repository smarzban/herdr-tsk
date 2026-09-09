//! Explicit, user-approved registration of the installed binary with Herdr.
use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
};
#[cfg(unix)]
mod dir;
#[cfg(unix)]
use dir::{Dir, TempFile};

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
            if !remove_binding(&mut keys[&name], key) {
                keys.remove(&name);
            }
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
    if let Some(path) = env::var_os("HERDR_CONFIG_PATH").filter(|v| !v.is_empty()) {
        return absolute(path.into());
    }
    let root = env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })
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

/// Persisted format: FNV-1a-64 over a domain tag, then each ordered UTF-8 name/body,
/// each prefixed by its byte length as a u64 little-endian integer. No std Hash encoding.
/// Integrity/change detection in a user-owned directory, not cryptographic authentication.
fn asset_root_name(assets: &[(&str, String)]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    feed(b"tsk-assets-v1\0");
    for (name, body) in assets {
        for bytes in [name.as_bytes(), body.as_bytes()] {
            feed(&(bytes.len() as u64).to_le_bytes());
            feed(bytes);
        }
    }
    format!("{hash:016x}")
}

fn managed_assets(binary: &Path, version: &str) -> io::Result<Vec<(&'static str, String)>> {
    let path = binary
        .to_str()
        .ok_or_else(|| error("installed binary path must be UTF-8"))?;
    let mut manifest = include_str!("../herdr-plugin.toml")
        .parse::<DocumentMut>()
        .map_err(|e| error(e.to_string()))?;
    manifest.remove("build");
    manifest["min_herdr_version"] = value("0.9.0");
    manifest["version"] = value(version);
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
fn herdr(args: &[&str], config: &Path) -> io::Result<String> {
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
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Prompt via an injected reader/writer, shared by production and regression tests.
pub fn confirm(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    key: &str,
    detail: &str,
) -> io::Result<bool> {
    for line in detail.lines() {
        writeln!(writer, "{}", crate::ui::terminal_text(line))?;
    }
    write!(
        writer,
        "Replace the existing {} binding? [y/N] ",
        crate::ui::terminal_text(key)
    )?;
    writer.flush()?;
    let mut response = String::new();
    if reader.read_line(&mut response)? == 0 {
        return Err(error("confirmation ended; no changes made"));
    }
    Ok(matches!(
        response.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub struct SetupResult {
    pub binary: PathBuf,
    pub root: PathBuf,
    pub backup: Option<PathBuf>,
}

pub fn run(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    interactive: bool,
) -> io::Result<SetupResult> {
    #[cfg(unix)]
    {
        run_at(
            &config_path()?,
            env!("CARGO_PKG_VERSION"),
            reader,
            writer,
            interactive,
            &mut herdr,
        )
    }
    #[cfg(not(unix))]
    {
        let _ = (reader, writer, interactive);
        Err(error("Herdr setup requires macOS or Linux"))
    }
}

fn registered_root(
    config: &Path,
    host: &mut impl FnMut(&[&str], &Path) -> io::Result<String>,
) -> io::Result<Option<PathBuf>> {
    let result = host(
        &["plugin", "list", "--plugin", "herdr-tsk", "--json"],
        config,
    )?;
    let json: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| error(format!("invalid Herdr plugin list: {e}")))?;
    let plugins = json
        .pointer("/result/plugins")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| error("Herdr plugin list is missing plugins"))?;
    if plugins.len() > 1 {
        return Err(error("multiple herdr-tsk registrations; refusing cleanup"));
    }
    plugins
        .first()
        .map(|p| {
            if p["plugin_id"].as_str() != Some("herdr-tsk") {
                return Err(error("unexpected plugin in filtered Herdr list"));
            }
            p["plugin_root"]
                .as_str()
                .map(PathBuf::from)
                .ok_or_else(|| error("Herdr registration is missing plugin_root"))
        })
        .transpose()
}

#[cfg(unix)]
fn run_at(
    config: &Path,
    version: &str,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    interactive: bool,
    host: &mut impl FnMut(&[&str], &Path) -> io::Result<String>,
) -> io::Result<SetupResult> {
    let parent_path = config
        .parent()
        .ok_or_else(|| error("config has no parent directory"))?;
    let filename = Path::new(
        config
            .file_name()
            .ok_or_else(|| error("config has no filename"))?,
    );
    // Pin an existing parent before reading or prompting. No writes before all conflict decisions.
    let existing = match Dir::open(parent_path, false) {
        Ok(d) => Some(d),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let before = if let Some(dir) = &existing {
        dir.read(filename)?
    } else {
        read_config(config)?
    };
    let edited = edit_bindings(
        before.as_deref().unwrap_or(""),
        interactive,
        |key, detail| confirm(reader, writer, key, detail),
    )?;
    let binary = installed_binary()?;
    let assets = managed_assets(&binary, version)?;
    host(&["--version"], config)?;
    let parent = match existing {
        Some(dir) => dir,
        None => Dir::open(parent_path, true)?,
    };
    parent.validate()?;
    let _lock = parent.lock()?;
    let unchanged = || -> io::Result<()> {
        parent.validate()?;
        if parent.read(filename)? != before {
            return Err(error("Herdr config changed during setup; retry"));
        }
        Ok(())
    };
    unchanged()?;
    let staged = TempFile {
        dir: &parent,
        name: format!(".tsk-config-{}.toml", uuid::Uuid::new_v4()).into(),
    };
    parent.write_new(&staged.name, &edited)?;
    host(&["config", "check"], &parent.path.join(&staged.name))?;
    unchanged()?;
    let old = registered_root(config, host)?;
    unchanged()?;
    let base = parent.child(Path::new("tsk-plugins"), true)?;
    let root = base.child(Path::new(&asset_root_name(&assets)), true)?;
    for (name, contents) in &assets {
        let path = Path::new(name);
        let directory = if path.parent().is_some_and(|p| p != Path::new("")) {
            Some(root.child(path.parent().unwrap(), true)?)
        } else {
            None
        };
        let directory = directory.as_ref().unwrap_or(&root);
        let file = Path::new(path.file_name().unwrap());
        match directory.read(file)? {
            Some(actual) if actual != *contents => {
                return Err(error(format!(
                    "managed plugin asset was modified; refusing overwrite: {}",
                    directory.path.join(file).display()
                )))
            }
            Some(_) => {}
            None => directory.write_new(file, contents)?,
        }
    }
    // Stage the recovery copy before registration; failed links leave no backup behind.
    let mut staged_backup = if before.as_deref() != Some(&edited) {
        if let Some(original) = &before {
            let staged = TempFile {
                dir: &parent,
                name: format!(".tsk-backup-{}.tmp", uuid::Uuid::new_v4()).into(),
            };
            parent.write_new(&staged.name, original)?;
            Some(staged)
        } else {
            None
        }
    } else {
        None
    };
    let backup = staged_backup.as_ref().map(|_| {
        parent
            .path
            .join(format!("config.toml.tsk-backup-{}", uuid::Uuid::new_v4()))
    });
    unchanged()?;
    base.validate()?;
    root.validate()?;
    host(
        &[
            "plugin",
            "link",
            root.path
                .to_str()
                .ok_or_else(|| error("plugin path must be UTF-8"))?,
        ],
        config,
    )?;
    // Everything after successful registration reports its partial state on failure.
    (|| -> io::Result<()> {
        unchanged()?;
        base.validate()?;
        root.validate()?;
        let linked = registered_root(config, host)?
            .ok_or_else(|| error("plugin link did not register herdr-tsk"))?;
        if fs::canonicalize(&linked)? != fs::canonicalize(&root.path)? {
            return Err(error(
                "Herdr registration changed; refusing config write and cleanup",
            ));
        }
        unchanged()?;
        if before.as_deref() != Some(&edited) {
            if let Err(e) = parent.rename(&staged.name, filename) {
                let recovery = if let Some(backup) = staged_backup.as_mut() {
                    let path = parent.path.join(&backup.name);
                    backup.preserve();
                    format!("backup retained at {}", path.display())
                } else {
                    "no previous config to back up".into()
                };
                return Err(error(format!("config replacement failed: {e}; {recovery}")));
            }
            if let Some(mut staged) = staged_backup.take() {
                let final_path = backup.as_ref().expect("staged backup has a destination");
                if let Err(e) =
                    parent.rename(&staged.name, Path::new(final_path.file_name().unwrap()))
                {
                    let pending_path = parent.path.join(&staged.name);
                    // Keep the recovery bytes if promotion or its directory sync failed.
                    staged.preserve();
                    return Err(error(format!(
                        "config saved, backup promotion failed: {e}; inspect {} and {}",
                        pending_path.display(),
                        final_path.display()
                    )));
                }
            }
        }
        if let Some(old) = old {
            cleanup_old(&base, &root, &old)?;
        }
        Ok(())
    })()
    .map_err(|e| {
        error(format!(
            "plugin registered, setup incomplete: {e}; inspect config and rerun setup"
        ))
    })?;
    Ok(SetupResult {
        binary,
        root: root.path.clone(),
        backup,
    })
}

#[cfg(unix)]
fn cleanup_old(base: &Dir, current: &Dir, old: &Path) -> io::Result<()> {
    base.validate()?;
    current.validate()?;
    // Never remove a source checkout or another installation. Only an intact generated root
    // directly in this setup's pinned asset base, with its content hash matching its name.
    let Some(name) = old.file_name() else {
        return Ok(());
    };
    let Some(parent) = old.parent() else {
        return Ok(());
    };
    let parent = match fs::canonicalize(parent) {
        Ok(path) => path,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if parent != fs::canonicalize(&base.path)? {
        return Ok(());
    }
    let old = match base.child(Path::new(name), false) {
        Ok(dir) => dir,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if Some(name) == current.path.file_name() {
        return Ok(());
    }
    let scripts = old.child(Path::new("scripts"), false)?;
    let mut contents = Vec::new();
    for file in [
        "herdr-plugin.toml",
        "scripts/open-board.sh",
        "scripts/open-capture.sh",
    ] {
        let path = Path::new(file);
        let dir = if file.starts_with("scripts/") {
            &scripts
        } else {
            &old
        };
        let body = dir
            .read(Path::new(path.file_name().unwrap()))?
            .ok_or_else(|| {
                error(format!(
                    "stale asset missing: {}",
                    old.path.join(file).display()
                ))
            })?;
        contents.push((file, body));
    }
    if name != std::ffi::OsStr::new(&asset_root_name(&contents)) {
        return Err(error(format!(
            "stale plugin root was modified; kept {}",
            old.path.display()
        )));
    }
    // Herdr 0.9 link replaces by plugin ID (online map insert, offline retain+push).
    // Do NOT unlink by ID here: that would remove the new registration too.
    scripts.remove(Path::new("open-board.sh"), false)?;
    scripts.remove(Path::new("open-capture.sh"), false)?;
    old.remove(Path::new("herdr-plugin.toml"), false)?;
    old.remove(Path::new("scripts"), true)?;
    base.remove(Path::new(name), true)?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests;
