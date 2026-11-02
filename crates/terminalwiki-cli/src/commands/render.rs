//! `tw render FILE|-` — render arbitrary file or stdin to terminal (spec §12).

use std::fs;
use std::io::{stdin, stdout, Read};
use std::path::PathBuf;

use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_render::{
    detect_color_mode, render_markdown, render_path, write_document, ColorMode, Theme,
};

use crate::args::Args;
use crate::output;

pub fn render(file: String, args: Args, config: Config, _wikis: WikiSet) -> Result<()> {
    let disable_color = args.plain || args.no_color || !output::is_stdout_tty();
    let theme = Theme::Dark;
    let color_mode = if disable_color {
        ColorMode::Never
    } else {
        detect_color_mode()
    };

    if file == "-" {
        let mut buf = String::new();
        stdin()
            .read_to_string(&mut buf)
            .map_err(|e| Error::other(format!("Failed to read stdin: {e}")))?;
        if disable_color {
            return output::writeln_sanitized(&mut stdout(), &buf)
                .map_err(|e| Error::other(format!("Write error: {}", e)));
        }
        let doc = render_markdown(&buf, &config, &theme, color_mode);
        return write_document(&doc, &mut stdout(), color_mode)
            .map_err(|e| Error::other(format!("Write error: {}", e)));
    }

    let path = PathBuf::from(&file);
    if !path.exists() {
        return Err(Error::other(format!("File not found: {}", path.display())));
    }
    let bytes = fs::read(&path).map_err(|e| Error::io(&path, e))?;
    let doc = render_path(&path, &bytes, &config, &theme, color_mode);
    write_document(&doc, &mut stdout(), color_mode)
        .map_err(|e| Error::other(format!("Write error: {}", e)))?;

    Ok(())
}
