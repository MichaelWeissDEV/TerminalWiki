//! `tw lint` — check wiki for broken links and frontmatter issues (spec §71).

use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::scan::{read_limited, scan};
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

pub fn lint(_args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let mut total_issues = 0usize;

    for wiki in wikis.iter() {
        let files = scan(wiki, &config.index);
        let mut issues: Vec<String> = Vec::new();

        // Collect all known page stems for link resolution.
        let page_stems: Vec<String> = files
            .iter()
            .filter(|f| f.content_type.is_page())
            .map(|f| {
                let s = f.relative.to_string_lossy();
                // Strip extension for loose matching.
                let stem = s.rsplit_once('.').map(|(s, _)| s).unwrap_or(&s);
                stem.to_ascii_lowercase()
            })
            .collect();

        for file in &files {
            if !file.content_type.is_page() {
                continue;
            }

            // Read with size limit.
            let (bytes, _truncated) = read_limited(&file.path, config.index.max_file_size)
                .map_err(|e| Error::io(&file.path, e))?;

            let text = String::from_utf8_lossy(&bytes);

            // Check frontmatter diagnostics.
            if file.content_type == terminalwiki_core::filetype::ContentType::Markdown {
                let fm = terminalwiki_core::frontmatter::Frontmatter::parse(&text);
                for diag in &fm.diagnostics {
                    issues.push(format!(
                        "{}:{}: {}",
                        sanitize_line(&file.relative.to_string_lossy()),
                        diag.line,
                        sanitize_line(&diag.message)
                    ));
                }

                // Check wiki links.
                let links = terminalwiki_core::link::find_links(&text);
                for (_range, link) in links {
                    if let terminalwiki_core::link::LinkTarget::Page { name, .. } = &link.target {
                        let lower = name.to_ascii_lowercase();
                        // Simple check: does any page stem contain this title?
                        let resolved = page_stems.iter().any(|stem| {
                            *stem == lower
                                || stem.ends_with(&format!("/{}", lower))
                                || stem.ends_with(&format!("\\{}", lower))
                        });
                        if !resolved {
                            issues.push(format!(
                                "{}: broken link [[{}]]",
                                sanitize_line(&file.relative.to_string_lossy()),
                                sanitize_line(name)
                            ));
                        }
                    }
                }
            }
        }

        if issues.is_empty() {
            println!("{}: OK", sanitize_line(&wiki.name));
        } else {
            for issue in &issues {
                println!("{}: {}", sanitize_line(&wiki.name), issue);
            }
            total_issues += issues.len();
        }
    }

    if total_issues > 0 {
        eprintln!("{} issue(s) found.", total_issues);
        // Exit code 1 is returned by the error below.
        return Err(Error::other(format!("{} lint issue(s)", total_issues)));
    }

    Ok(())
}
