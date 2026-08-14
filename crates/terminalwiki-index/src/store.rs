use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use terminalwiki_core::error::{Error, Result};
use crate::entry::IndexEntry;

const INDEX_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct IndexMeta {
    version: u32,
    built_at: u64,
}

pub fn save_index(index_dir: &Path, entries: &[IndexEntry]) -> Result<()> {
    if !index_dir.exists() {
        fs::create_dir_all(index_dir).map_err(|e| Error::Io {
            path: Some(index_dir.to_path_buf()),
            source: e,
        })?;
    }

    let meta_path = index_dir.join("meta.json");
    let entries_path = index_dir.join("entries.jsonl");

    let meta = IndexMeta {
        version: INDEX_VERSION,
        built_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    let meta_json = serde_json::to_string(&meta).unwrap();
    fs::write(&meta_path, meta_json).map_err(|e| Error::Io {
        path: Some(meta_path.to_path_buf()),
        source: e,
    })?;

    let mut file = File::create(&entries_path).map_err(|e| Error::Io {
        path: Some(entries_path.to_path_buf()),
        source: e,
    })?;

    for entry in entries {
        let json = serde_json::to_string(entry).unwrap();
        writeln!(file, "{}", json).map_err(|e| Error::Io {
            path: Some(entries_path.to_path_buf()),
            source: e,
        })?;
    }

    Ok(())
}

pub fn load_index(index_dir: &Path) -> Result<Option<Vec<IndexEntry>>> {
    let meta_path = index_dir.join("meta.json");
    let entries_path = index_dir.join("entries.jsonl");

    if !meta_path.exists() || !entries_path.exists() {
        return Ok(None);
    }

    let meta_json = fs::read_to_string(&meta_path).map_err(|e| Error::Io {
        path: Some(meta_path.to_path_buf()),
        source: e,
    })?;

    let meta: IndexMeta = match serde_json::from_str(&meta_json) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };

    if meta.version != INDEX_VERSION {
        return Ok(None);
    }

    let file = File::open(&entries_path).map_err(|e| Error::Io {
        path: Some(entries_path.to_path_buf()),
        source: e,
    })?;

    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| Error::Io {
            path: Some(entries_path.to_path_buf()),
            source: e,
        })?;
        match serde_json::from_str::<IndexEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(_) => continue,
        }
    }

    Ok(Some(entries))
}
