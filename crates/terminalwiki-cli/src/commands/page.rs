use std::fs;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::resolve;
use terminalwiki_core::filetype;
use crate::args::Args;
use crate::output;

pub fn show_home(args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    let default_wiki = wikis.default_wiki().ok_or_else(|| Error::NoWikiConfigured)?;
    show_page(Some(default_wiki.name.clone()), String::new(), args, config, wikis)
}

pub fn show_page(wiki: Option<String>, mut page: String, args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    let start_wiki = wiki
        .or(args.wiki.clone())
        .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
        .ok_or_else(|| Error::NoWikiConfigured)?;

    // If page is empty, resolve to index/home
    if page.is_empty() {
        page = "index".to_string(); // Fallback to index if no home page is explicitly configured? 
        // Actually, we could try to look for `index.md`, `README.md`, `Home.md`
    }

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;
    
    display_file(&resolution.path, args.plain, args.no_color, &config)
}

fn display_file(path: &std::path::Path, plain: bool, no_color: bool, config: &Config) -> Result<()> {
    if !path.exists() {
        return Err(Error::other(format!("File does not exist: {}", path.display())));
    }
    
    let content = fs::read(path).map_err(|e| Error::io(path, e))?;
    let is_binary = filetype::looks_binary(&content);

    if is_binary {
        // TODO: render binary info using terminalwiki_render once complete
        let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
        println!("Binary file: {}", path.display());
        println!("Size: {} bytes", meta.len());
        return Ok(());
    }

    let text = String::from_utf8_lossy(&content);

    // If plain or not TTY, dump sanitized text directly
    let disable_color = plain || no_color || !output::is_stdout_tty();
    if disable_color {
        return output::writeln_sanitized(&mut std::io::stdout(), &text)
            .map_err(|e| Error::other(format!("Write error: {}", e)));
    }

    // Attempt to use terminalwiki_render
    use terminalwiki_render::{render_markdown, write_document, detect_color_mode, Theme, ColorMode};

    // Minimal theme fallback if load fails
    let theme = Theme::Dark;
    let color_mode = if disable_color { ColorMode::Never } else { detect_color_mode() };

    let doc = render_markdown(&text, config, &theme, color_mode);
    write_document(&doc, &mut std::io::stdout(), color_mode)
        .map_err(|e| Error::other(format!("Write error: {}", e)))?;

    Ok(())
}
