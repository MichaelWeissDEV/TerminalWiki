//! Persistent index storage handling entries.jsonl and Tantivy sync (spec §15, §16).

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use terminalwiki_core::error::{Error, Result};

use crate::entry::IndexEntry;
use crate::tantivy_store::TantivyStore;

pub fn save_index(index_dir: &Path, entries: &[IndexEntry]) -> Result<()> {
    if !index_dir.exists() {
        fs::create_dir_all(index_dir).map_err(|e| Error::io(index_dir, e))?;
    }

    let entries_path = index_dir.join("entries.jsonl");

    let mut file = File::create(&entries_path).map_err(|e| Error::io(&entries_path, e))?;

    for entry in entries {
        let json = serde_json::to_string(entry).map_err(|e| Error::index(e.to_string()))?;
        writeln!(file, "{}", json).map_err(|e| Error::io(&entries_path, e))?;
    }

    // Sync Tantivy Store
    let mut tantivy_store = TantivyStore::open_or_create(index_dir)?;
    tantivy_store.update_entries(index_dir, entries)?;

    Ok(())
}

pub fn load_index(index_dir: &Path) -> Result<Option<Vec<IndexEntry>>> {
    let meta_path = index_dir.join("meta.json");
    let entries_path = index_dir.join("entries.jsonl");

    if !meta_path.exists() || !entries_path.exists() {
        return Ok(None);
    }

    let file = File::open(&entries_path).map_err(|e| Error::io(&entries_path, e))?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| Error::io(&entries_path, e))?;
        if let Ok(entry) = serde_json::from_str::<IndexEntry>(&line) {
            entries.push(entry);
        }
    }

    Ok(Some(entries))
}
