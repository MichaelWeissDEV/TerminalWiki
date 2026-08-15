//! Compact persistent metadata store in state.json (spec §15-§17).

use std::fs::{self, File};
use std::path::Path;

use serde::{Deserialize, Serialize};
use terminalwiki_core::error::{Error, Result};

use crate::entry::DocumentState;
use crate::tantivy_store::INDEX_SCHEMA_VERSION;

/// Persistent state container stored in `state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexState {
    pub schema_version: u32,
    pub built_at: u64,
    pub document_count: usize,
    pub entries: Vec<DocumentState>,
}

pub fn save_state(index_dir: &Path, states: &[DocumentState]) -> Result<()> {
    if !index_dir.exists() {
        fs::create_dir_all(index_dir).map_err(|e| Error::io(index_dir, e))?;
    }

    let state = IndexState {
        schema_version: INDEX_SCHEMA_VERSION,
        built_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        document_count: states.len(),
        entries: states.to_vec(),
    };

    let state_path = index_dir.join("state.json");
    let state_file = File::create(&state_path).map_err(|e| Error::io(&state_path, e))?;
    serde_json::to_writer(state_file, &state)
        .map_err(|e| Error::index(format!("Failed to serialize state.json: {e}")))?;

    Ok(())
}

/// Why persisted state could not be used, or the state itself.
///
/// A schema mismatch is deliberately distinguished from "no index yet": the
/// former requires a full rebuild and must be announced to the user, whereas
/// collapsing both into `None` turns a rebuild into a silent, minutes-long
/// "incremental update" of every document.
#[derive(Debug)]
pub enum StoredState {
    /// No index has been built for this wiki yet.
    Absent,
    /// State exists but was written by a different index schema.
    SchemaMismatch {
        found: u32,
        expected: u32,
    },
    /// State exists but could not be parsed; treated like a rebuild.
    Unreadable,
    Loaded(Vec<DocumentState>),
}

/// Loads persisted document state, reporting *why* it is unusable when it is.
pub fn load_state(index_dir: &Path) -> Result<StoredState> {
    let state_path = index_dir.join("state.json");

    if !state_path.exists() {
        return Ok(StoredState::Absent);
    }

    let file = File::open(&state_path).map_err(|e| Error::io(&state_path, e))?;
    let state: IndexState = match serde_json::from_reader(file) {
        Ok(s) => s,
        Err(_) => return Ok(StoredState::Unreadable),
    };

    if state.schema_version != INDEX_SCHEMA_VERSION {
        return Ok(StoredState::SchemaMismatch {
            found: state.schema_version,
            expected: INDEX_SCHEMA_VERSION,
        });
    }

    Ok(StoredState::Loaded(state.entries))
}

pub fn load_index(index_dir: &Path) -> Result<Option<Vec<DocumentState>>> {
    Ok(match load_state(index_dir)? {
        StoredState::Loaded(entries) => Some(entries),
        _ => None,
    })
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
