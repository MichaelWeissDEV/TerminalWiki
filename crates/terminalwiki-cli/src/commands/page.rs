//! `tw [WIKI] PAGE` and `tw` — resolve and display wiki pages or source files (spec §8, §10, §27).

use std::fs;
use std::path::Path;

use terminalwiki_core::filetype::{classify, ContentType};
use terminalwiki_core::resolve;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_render::{
    detect_color_mode, render_binary_info, render_code_file, render_markdown, write_document,
    ColorMode, Theme,
};

use crate::args::Args;
use crate::output;

pub fn show_home(args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    let default_wiki = wikis.default_wiki().ok_or(Error::NoWikiConfigured)?;
    show_page(Some(default_wiki.name.clone()), String::new(), args, config, wikis)
}

pub fn show_page(
    wiki_opt: Option<String>,
    page_arg: String,
    args: Args,
    config: Config,
    wikis: WikiSet,
) -> Result<()> {
    let start_wiki = wiki_opt
        .or(args.wiki.clone())
        .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
        .ok_or(Error::NoWikiConfigured)?;

    let wiki = wikis.require(&start_wiki)?;

    // 1. Home page resolution if page argument is empty (bare `tw` or `tw WIKI`)
    if page_arg.trim().is_empty() {
        if let Some(home_path) = wiki.home_page() {
            return display_file(&home_path, args.plain, args.no_color, &config);
        } else {
            // Generate automatic wiki overview (spec §10)
            return render_wiki_overview(wiki, &config, args.plain, args.no_color);
        }
    }

    // 2. Resolve named page or file path
    let resolution = resolve::resolve(&wikis, &start_wiki, &page_arg, &config.index)?;
    display_file(&resolution.path, args.plain, args.no_color, &config)
}

fn display_file(
    path: &Path,
    plain: bool,
    no_color: bool,
    config: &Config,
) -> Result<()> {
    if !path.exists() {
        return Err(Error::other(format!("File does not exist: {}", path.display())));
    }

    let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
    let content_type = classify(path, &bytes);

    let disable_color = plain || no_color || !output::is_stdout_tty();
    let theme = match config.theme {
        terminalwiki_core::config::Theme::Light => Theme::Light,
        terminalwiki_core::config::Theme::Mono => Theme::Mono,
        _ => Theme::Dark,
    };
    let color_mode = if disable_color {
        ColorMode::Never
    } else {
        detect_color_mode()
    };

    match content_type {
        ContentType::Binary => {
            let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
            let mtime = meta
                .modified()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);
            let doc = render_binary_info(path, meta.len(), mtime);
            write_document(&doc, &mut std::io::stdout(), color_mode)
                .map_err(|e| Error::other(format!("Write error: {e}")))?;
        }
        ContentType::Code => {
            let text = String::from_utf8_lossy(&bytes);
            let lang = terminalwiki_core::filetype::language_for(path);
            let doc = render_code_file(&text, lang, path, config, &theme, color_mode, None);
            write_document(&doc, &mut std::io::stdout(), color_mode)
                .map_err(|e| Error::other(format!("Write error: {e}")))?;
        }
        ContentType::Markdown | ContentType::Text | ContentType::Latex | ContentType::Image => {
            let text = String::from_utf8_lossy(&bytes);
            if disable_color {
                output::writeln_sanitized(&mut std::io::stdout(), &text)
                    .map_err(|e| Error::other(format!("Write error: {e}")))?;
            } else {
                let doc = render_markdown(&text, config, &theme, color_mode);
                write_document(&doc, &mut std::io::stdout(), color_mode)
                    .map_err(|e| Error::other(format!("Write error: {e}")))?;
            }
        }
    }

    Ok(())
}

fn render_wiki_overview(
    wiki: &terminalwiki_core::wiki::Wiki,
    config: &Config,
    plain: bool,
    no_color: bool,
) -> Result<()> {
    let mut overview = format!("# {}\n\n*Knowledge base overview*\n\n## Pages\n\n", wiki.name);
    let files = terminalwiki_core::scan::scan(wiki, &config.index);
    for file in files {
        if file.content_type.is_page() {
            let path_str = file.relative.to_string_lossy();
            overview.push_str(&format!("- [[{}]]\n", sanitize_line(&path_str)));
        }
    }

    let disable_color = plain || no_color || !output::is_stdout_tty();
    let theme = Theme::Dark;
    let color_mode = if disable_color {
        ColorMode::Never
    } else {
        detect_color_mode()
    };

    let doc = render_markdown(&overview, config, &theme, color_mode);
    write_document(&doc, &mut std::io::stdout(), color_mode)
        .map_err(|e| Error::other(format!("Write error: {e}")))?;

    Ok(())
}
