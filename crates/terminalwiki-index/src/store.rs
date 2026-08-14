//! Compact persistent metadata store in state.json (spec §15, §16, §21, §22).

use std::fs::{self, File};
use std::path::Path;

use serde::{Deserialize, Serialize};
use terminalwiki_core::error::{Error, Result};

use crate::entry::IndexEntry;
use crate::tantivy_store::{TantivyStore, INDEX_SCHEMA_VERSION};

/// Persistent state container stored in `state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexState {
    pub schema_version: u32,
    pub built_at: u64,
    pub document_count: usize,
    pub entries: Vec<IndexEntry>,
}

pub fn save_index(index_dir: &Path, entries: &[IndexEntry]) -> Result<()> {
    if !index_dir.exists() {
        fs::create_dir_all(index_dir).map_err(|e| Error::io(index_dir, e))?;
    }

    let state = IndexState {
        schema_version: INDEX_SCHEMA_VERSION,
        built_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        document_count: entries.len(),
        entries: entries.to_vec(),
    };

    let state_path = index_dir.join("state.json");
    let state_file = File::create(&state_path).map_err(|e| Error::io(&state_path, e))?;
    serde_json::to_writer(state_file, &state)
        .map_err(|e| Error::index(format!("Failed to serialize state.json: {e}")))?;

    // Sync Tantivy in its isolated `index_dir/tantivy` directory
    let mut tantivy_store = TantivyStore::open_or_create(index_dir)?;
    tantivy_store.update_entries(index_dir, entries)?;

    Ok(())
}

pub fn load_index(index_dir: &Path) -> Result<Option<Vec<IndexEntry>>> {
    let state_path = index_dir.join("state.json");

    if !state_path.exists() {
        return Ok(None);
    }

    let file = File::open(&state_path).map_err(|e| Error::io(&state_path, e))?;
    let state: IndexState = match serde_json::from_reader(file) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    if state.schema_version != INDEX_SCHEMA_VERSION {
        return Ok(None);
    }

    Ok(Some(state.entries))
}

pub fn load_meta(index_dir: &Path) -> Result<Option<IndexState>> {
    let state_path = index_dir.join("state.json");

    if !state_path.exists() {
        return Ok(None);
    }

    let file = File::open(&state_path).map_err(|e| Error::io(&state_path, e))?;
    let state: IndexState = match serde_json::from_reader(file) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    Ok(Some(state))
}
