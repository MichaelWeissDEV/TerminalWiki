use std::env;
use std::path::Path;
use std::process::Command;
use terminalwiki_core::{Config, Error, Result};

/// Resolves the editor from config, then env vars, then $PATH fallbacks.
pub fn resolve_editor(config: &Config) -> String {
    // config.editor overrides everything.
    if let Some(ref ed) = config.editor {
        if !ed.is_empty() {
            return ed.clone();
        }
    }
    resolve_editor_no_config()
}

/// Resolves the editor from env vars and $PATH only, without a config.
pub fn resolve_editor_no_config() -> String {
    if let Ok(ed) = env::var("TW_EDITOR") {
        if !ed.is_empty() {
            return ed;
        }
    }
    if let Ok(ed) = env::var("VISUAL") {
        if !ed.is_empty() {
            return ed;
        }
    }
    if let Ok(ed) = env::var("EDITOR") {
        if !ed.is_empty() {
            return ed;
        }
    }
    // Fallbacks
    if is_in_path("nvim") {
        return "nvim".to_string();
    }
    if is_in_path("vim") {
        return "vim".to_string();
    }
    if is_in_path("vi") {
        return "vi".to_string();
    }
    if is_in_path("nano") {
        return "nano".to_string();
    }

    // Last resort fallback
    "vi".to_string()
}

fn is_in_path(cmd: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            let p = Path::new(dir).join(cmd);
            if p.is_executable() {
                return true;
            }
        }
    }
    false
}

trait IsExecutable {
    fn is_executable(&self) -> bool;
}

impl IsExecutable for Path {
    fn is_executable(&self) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = self.metadata() {
                return meta.is_file() && meta.permissions().mode() & 0o111 != 0;
            }
        }
        #[cfg(not(unix))]
        {
            if let Ok(meta) = self.metadata() {
                return meta.is_file();
            }
        }
        false
    }
}

pub fn open_editor(path: &Path, config: &Config) -> Result<()> {
    let editor = resolve_editor(config);

    // Handle case where editor might contain arguments like `code -w`
    let parts: Vec<&str> = editor.split_whitespace().collect();
    if parts.is_empty() {
        return Err(Error::other("Editor variable is empty"));
    }

    let mut cmd = Command::new(parts[0]);
    for arg in &parts[1..] {
        cmd.arg(arg);
    }
    cmd.arg(path);

    let status = cmd
        .status()
        .map_err(|e| Error::other(format!("Failed to execute editor '{}': {}", editor, e)))?;

    if !status.success() {
        return Err(Error::other(format!(
            "Editor exited with status: {}",
            status
        )));
    }

    Ok(())
}
