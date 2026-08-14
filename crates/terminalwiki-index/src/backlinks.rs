//! Backlink extraction from indexed metadata (spec §16, §41, §42).

use std::path::{Path, PathBuf};

use crate::entry::DocumentState;

#[derive(Debug, Clone)]
pub struct BacklinkResult {
    pub from: PathBuf,
    pub context: String,
}

pub fn find_backlinks(entries: &[DocumentState], target_relative: &Path) -> Vec<BacklinkResult> {
    let mut results = Vec::new();
    let target_str = target_relative.to_string_lossy();
    let target_stem = target_relative
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    for entry in entries {
        for link in &entry.wiki_links {
            if link == &target_str || link == &target_stem {
                results.push(BacklinkResult {
                    from: entry.relative.clone(),
                    context: format!("Page: {}", entry.title),
                });
                break;
            }
        }
    }

    results
}
