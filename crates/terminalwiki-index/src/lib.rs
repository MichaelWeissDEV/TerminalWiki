pub mod backlinks;
pub mod entry;
pub mod indexer;
pub mod query;
pub mod search;
pub mod store;

use std::collections::BTreeMap;
use std::path::Path;

use terminalwiki_core::config::Config;
use terminalwiki_core::error::{Error, Result};
use terminalwiki_core::paths;
use terminalwiki_core::wiki::Wiki;

pub use backlinks::BacklinkResult;
pub use entry::IndexEntry;
pub use query::{Query, QueryTerm};
pub use search::SearchResult;

pub struct WikiIndex {
    pub entries: Vec<IndexEntry>,
}

impl WikiIndex {
    /// Loads the index for the given wiki from disk.
    pub fn load(wiki_name: &str) -> Result<WikiIndex> {
        let dir = paths::index_dir_for(wiki_name).ok_or_else(|| Error::Config {
            path: None, message: "Cannot determine index directory".into(),
        })?;
        let entries = store::load_index(&dir)?.unwrap_or_default();
        Ok(WikiIndex { entries })
    }

    /// Builds a fresh index by scanning the wiki.
    pub fn build(wiki: &Wiki, config: &Config) -> Result<WikiIndex> {
        let entries = indexer::update_index(wiki, config, None)?;
        let index = WikiIndex { entries };
        index.save(&wiki.name)?;
        Ok(index)
    }

    /// Incrementally updates the existing index for a wiki.
    pub fn update(wiki: &Wiki, config: &Config) -> Result<WikiIndex> {
        let dir = paths::index_dir_for(&wiki.name).ok_or_else(|| Error::Config {
            path: None, message: "Cannot determine index directory".into(),
        })?;
        let existing = store::load_index(&dir)?;
        let entries = indexer::update_index(wiki, config, existing)?;
        let index = WikiIndex { entries };
        index.save(&wiki.name)?;
        Ok(index)
    }

    /// Saves the current in-memory index to disk.
    pub fn save(&self, wiki_name: &str) -> Result<()> {
        let dir = paths::index_dir_for(wiki_name).ok_or_else(|| Error::Config {
            path: None, message: "Cannot determine index directory".into(),
        })?;
        store::save_index(&dir, &self.entries)
    }

    /// Searches the index based on the given query.
    pub fn search(&self, query: &Query) -> Vec<SearchResult> {
        search::search(&self.entries, query)
    }

    /// Finds all pages that link to the given relative path.
    pub fn backlinks(&self, page_relative: &Path) -> Vec<BacklinkResult> {
        backlinks::find_backlinks(&self.entries, page_relative)
    }

    /// Returns a map of all tags and the pages that use them.
    pub fn tags(&self) -> BTreeMap<String, Vec<String>> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in &self.entries {
            for tag in &entry.tags {
                map.entry(tag.clone())
                    .or_default()
                    .push(entry.relative.to_string_lossy().to_string());
            }
        }
        map
    }
}
