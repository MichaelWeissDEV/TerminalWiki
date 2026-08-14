//! `tw wiki ...` — manage the list of registered wikis (spec §83).
//!
//! Changes are written back to the global config file so they survive across
//! invocations. The file is read fresh before each mutation to avoid losing
//! edits made by another tool (spec §78).

use std::path::PathBuf;

use terminalwiki_core::config::WikiEntry;
use terminalwiki_core::paths::{config_file, expand_tilde};
use terminalwiki_core::sanitize::sanitize_path_display;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::{Args, WikiCommand};

pub fn run(cmd: WikiCommand, args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    match cmd {
        WikiCommand::List => list(args, wikis),
        WikiCommand::Add { name, path, default } => add(name, path, default),
        WikiCommand::Remove { name } => remove(name),
        WikiCommand::Rename { old, new } => rename(old, new),
        WikiCommand::Mount { parent, child } => mount(parent, child),
        WikiCommand::Unmount { parent, child } => unmount(parent, child),
        WikiCommand::Default { name } => set_default(name),
    }
}

// ─── list ────────────────────────────────────────────────────────────────────

fn list(_args: Args, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        println!("No wikis configured.");
        println!("\nAdd one with:");
        println!("  tw wiki add main ~/Knowledge --default");
        return Ok(());
    }
    for wiki in wikis.iter() {
        let is_default = wikis.default_wiki().is_some_and(|d| d.name == wiki.name);
        let marker = if is_default { "*" } else { " " };
        println!("{} {}  {}", marker, wiki.name, sanitize_path_display(&wiki.root));
    }
    Ok(())
}

// ─── add ─────────────────────────────────────────────────────────────────────

fn add(name: String, path: String, make_default: bool) -> Result<()> {
    validate_name(&name)?;
    let expanded = expand_tilde(&PathBuf::from(&path));
    let abs = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map_err(|e| Error::other(format!("Cannot get cwd: {e}")))?
            .join(expanded)
    };

    let mut cfg = read_config_raw()?;

    if cfg.wikis.iter().any(|w| w.name == name) {
        return Err(Error::invalid_arguments(format!(
            "Wiki '{}' is already registered. Use 'tw wiki rename' to rename it.",
            name
        )));
    }

    cfg.wikis.push(WikiEntry { name: name.clone(), path: abs.clone(), mounts: Vec::new() });

    if make_default || cfg.default_wiki.is_none() {
        cfg.default_wiki = Some(name.clone());
    }

    write_config_raw(&cfg)?;

    println!("Added wiki '{}' at {}", name, sanitize_path_display(&abs));
    if make_default || cfg.default_wiki.as_deref() == Some(&name) {
        println!("Set as default wiki.");
    }
    Ok(())
}

// ─── remove ──────────────────────────────────────────────────────────────────

fn remove(name: String) -> Result<()> {
    let mut cfg = read_config_raw()?;

    let before = cfg.wikis.len();
    cfg.wikis.retain(|w| w.name != name);

    if cfg.wikis.len() == before {
        return Err(Error::WikiNotFound {
            name: name.clone(),
            known: cfg.wikis.iter().map(|w| w.name.clone()).collect(),
        });
    }

    if cfg.default_wiki.as_deref() == Some(&name) {
        cfg.default_wiki = cfg.wikis.first().map(|w| w.name.clone());
    }

    write_config_raw(&cfg)?;
    println!("Removed wiki '{}'.", name);
    Ok(())
}

// ─── rename ──────────────────────────────────────────────────────────────────

fn rename(old: String, new: String) -> Result<()> {
    validate_name(&new)?;
    let mut cfg = read_config_raw()?;

    let Some(entry) = cfg.wikis.iter_mut().find(|w| w.name == old) else {
        return Err(Error::WikiNotFound {
            name: old.clone(),
            known: cfg.wikis.iter().map(|w| w.name.clone()).collect(),
        });
    };
    entry.name = new.clone();

    if cfg.default_wiki.as_deref() == Some(&old) {
        cfg.default_wiki = Some(new.clone());
    }

    write_config_raw(&cfg)?;
    println!("Renamed wiki '{}' → '{}'.", old, new);
    Ok(())
}

// ─── mount / unmount ─────────────────────────────────────────────────────────

fn mount(parent: String, child: String) -> Result<()> {
    let mut cfg = read_config_raw()?;

    let Some(p) = cfg.wikis.iter_mut().find(|w| w.name == parent) else {
        return Err(Error::WikiNotFound {
            name: parent.clone(),
            known: cfg.wikis.iter().map(|w| w.name.clone()).collect(),
        });
    };

    if p.mounts.contains(&child) {
        return Err(Error::invalid_arguments(format!(
            "Wiki '{}' is already mounted into '{}'.",
            child, parent
        )));
    }
    p.mounts.push(child.clone());

    write_config_raw(&cfg)?;
    println!("Mounted '{}' into '{}'.", child, parent);
    Ok(())
}

fn unmount(parent: String, child: String) -> Result<()> {
    let mut cfg = read_config_raw()?;

    let Some(p) = cfg.wikis.iter_mut().find(|w| w.name == parent) else {
        return Err(Error::WikiNotFound {
            name: parent.clone(),
            known: cfg.wikis.iter().map(|w| w.name.clone()).collect(),
        });
    };

    let before = p.mounts.len();
    p.mounts.retain(|m| m != &child);

    if p.mounts.len() == before {
        return Err(Error::invalid_arguments(format!(
            "Wiki '{}' is not mounted into '{}'.",
            child, parent
        )));
    }

    write_config_raw(&cfg)?;
    println!("Unmounted '{}' from '{}'.", child, parent);
    Ok(())
}

// ─── default ─────────────────────────────────────────────────────────────────

fn set_default(name: String) -> Result<()> {
    let mut cfg = read_config_raw()?;

    if !cfg.wikis.iter().any(|w| w.name == name) {
        return Err(Error::WikiNotFound {
            name: name.clone(),
            known: cfg.wikis.iter().map(|w| w.name.clone()).collect(),
        });
    }

    cfg.default_wiki = Some(name.clone());
    write_config_raw(&cfg)?;
    println!("Default wiki is now '{}'.", name);
    Ok(())
}

// ─── config I/O ──────────────────────────────────────────────────────────────

/// Reads the raw config file into a `Config`. Falls back to defaults when the
/// file doesn't exist yet (first run).
fn read_config_raw() -> Result<Config> {
    let path = config_file().ok_or_else(|| Error::config("Cannot determine config directory"))?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config { message: e.to_string(), path: Some(path.clone()) })?;
    let layer = terminalwiki_core::config::parse_global(&text, Some(path))?;
    let mut cfg = Config::default();
    layer.apply_to(&mut cfg);
    Ok(cfg)
}

/// Serialises the config back to the TOML file, creating parent dirs as needed.
fn write_config_raw(config: &Config) -> Result<()> {
    let path = config_file().ok_or_else(|| Error::config("Cannot determine config directory"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Config { message: e.to_string(), path: Some(path.clone()) })?;
    }

    // Build a minimal TOML document from the config fields.
    let mut doc = toml::map::Map::new();

    if let Some(ref default) = config.default_wiki {
        doc.insert("default_wiki".into(), toml::Value::String(default.clone()));
    }
    if let Some(ref editor) = config.editor {
        doc.insert("editor".into(), toml::Value::String(editor.clone()));
    }

    let wikis: Vec<toml::Value> = config
        .wikis
        .iter()
        .map(|w| {
            let mut m = toml::map::Map::new();
            m.insert("name".into(), toml::Value::String(w.name.clone()));
            m.insert(
                "path".into(),
                toml::Value::String(w.path.to_string_lossy().into_owned()),
            );
            if !w.mounts.is_empty() {
                m.insert(
                    "mount".into(),
                    toml::Value::Array(
                        w.mounts.iter().map(|s| toml::Value::String(s.clone())).collect(),
                    ),
                );
            }
            toml::Value::Table(m)
        })
        .collect();

    if !wikis.is_empty() {
        doc.insert("wiki".into(), toml::Value::Array(wikis));
    }

    let text = toml::to_string_pretty(&toml::Value::Table(doc))
        .map_err(|e| Error::Config { message: e.to_string(), path: Some(path.clone()) })?;

    std::fs::write(&path, text)
        .map_err(|e| Error::Config { message: e.to_string(), path: Some(path) })?;

    Ok(())
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::invalid_arguments("Wiki name cannot be empty."));
    }
    if name.contains(['/', '\\', ' ', '\t']) {
        return Err(Error::invalid_arguments(format!(
            "Wiki name '{}' must not contain path separators or whitespace.",
            name
        )));
    }
    Ok(())
}