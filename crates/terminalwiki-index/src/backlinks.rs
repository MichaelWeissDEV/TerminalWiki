use std::path::{Path, PathBuf};
use crate::entry::IndexEntry;

#[derive(Debug, Clone)]
pub struct BacklinkResult {
    pub from: PathBuf,
    pub context: String,
}

pub fn find_backlinks(entries: &[IndexEntry], target_relative: &Path) -> Vec<BacklinkResult> {
    let mut results = Vec::new();
    let target_str = target_relative.to_string_lossy();
    let target_stem = target_relative.file_stem().unwrap_or_default().to_string_lossy();

    for entry in entries {
        for link in &entry.wiki_links {
            // Very simple link matching for now
            if link == &target_str || link == &target_stem {
                results.push(BacklinkResult {
                    from: entry.relative.clone(),
                    context: String::new(), // To be implemented: extract snippet around link
                });
                break;
            }
        }
    }

    results
}
