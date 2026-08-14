//! `tw files [--type TYPE]` — list files in the wiki (spec §20).

use terminalwiki_core::filetype::ContentType;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::scan::scan;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

pub fn files(
    _type_filter: Option<String>,
    args: Args,
    config: Config,
    wikis: WikiSet,
) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let type_filter: Option<ContentType> = _type_filter.as_deref().and_then(ContentType::parse);

    for wiki in wikis.iter() {
        let scanned = scan(wiki, &config.index);
        for file in &scanned {
            if let Some(t) = type_filter {
                if file.content_type != t {
                    continue;
                }
            }
            let path = sanitize_line(&file.relative.to_string_lossy());
            if args.json {
                let size = file.size;
                let ct = file.content_type.as_str();
                let mtime = file.mtime;
                println!(
                    "{{\"wiki\":\"{}\",\"path\":\"{}\",\"type\":\"{}\",\"size\":{},\"mtime\":{}}}",
                    sanitize_line(&wiki.name),
                    path,
                    ct,
                    size,
                    mtime,
                );
            } else {
                println!("{}", path);
            }
        }
        if !args.all {
            break;
        }
    }

    Ok(())
}
