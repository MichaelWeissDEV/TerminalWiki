//! `tw init [NAME]` — initialise and register the current directory as a wiki.

use std::fs;
use std::path::Path;

use terminalwiki_core::{Error, Result};

use super::wiki;

/// Creates only missing starter files, then registers the current directory as
/// the default wiki. Existing documents and local configuration are never
/// overwritten.
pub fn init(name: String) -> Result<()> {
    wiki::validate_name(&name)?;
    let root = std::env::current_dir().map_err(|e| Error::other(format!("Cannot get cwd: {e}")))?;
    let local_config = root.join(".terminalwiki.toml");
    let index = root.join("index.md");

    write_if_missing(
        &local_config,
        format!("title = {}\nhome = \"index.md\"\n", toml_string(&name)),
    )?;
    write_if_missing(
        &index,
        format!("# {}\n\nWelcome to your TerminalWiki.\n", name),
    )?;

    wiki::add(name.clone(), root.to_string_lossy().into_owned(), true)?;
    println!("Initialized wiki '{}' in {}", name, root.display());
    Ok(())
}

fn write_if_missing(path: &Path, contents: String) -> Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(contents.as_bytes())
                .map_err(|e| Error::io(path, e))?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(Error::io(path, e)),
    }
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use crate::args::{Args, Command};

    #[test]
    fn init_accepts_name_flag_and_positional_name() {
        let args =
            Args::parse(&["tw".into(), "init".into(), "--name".into(), "notes".into()]).unwrap();
        assert_eq!(
            args.command,
            Command::Init {
                name: "notes".into()
            }
        );

        let args = Args::parse(&["tw".into(), "init".into(), "other".into()]).unwrap();
        assert_eq!(
            args.command,
            Command::Init {
                name: "other".into()
            }
        );
    }
}
