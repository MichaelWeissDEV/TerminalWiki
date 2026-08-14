//! Per-document metadata and record definitions (spec §15, §16).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use terminalwiki_core::filetype::ContentType;

/// Persistent metadata entry representing an indexed file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub wiki: String,
    pub path: PathBuf,
    pub relative: PathBuf,
    pub size: u64,
    pub mtime: u64,
    #[serde(with = "hex_serde")]
    pub content_hash: [u8; 32],
    pub content_type: ContentTypeHelper,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub headings: Vec<String>,
    pub body_text: String,
    pub wiki_links: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentTypeHelper {
    Markdown,
    Text,
    Code,
    Latex,
    Image,
    Binary,
}

impl From<ContentType> for ContentTypeHelper {
    fn from(ct: ContentType) -> Self {
        match ct {
            ContentType::Markdown => ContentTypeHelper::Markdown,
            ContentType::Text => ContentTypeHelper::Text,
            ContentType::Code => ContentTypeHelper::Code,
            ContentType::Latex => ContentTypeHelper::Latex,
            ContentType::Image => ContentTypeHelper::Image,
            ContentType::Binary => ContentTypeHelper::Binary,
        }
    }
}

impl From<ContentTypeHelper> for ContentType {
    fn from(helper: ContentTypeHelper) -> Self {
        match helper {
            ContentTypeHelper::Markdown => ContentType::Markdown,
            ContentTypeHelper::Text => ContentType::Text,
            ContentTypeHelper::Code => ContentType::Code,
            ContentTypeHelper::Latex => ContentType::Latex,
            ContentTypeHelper::Image => ContentType::Image,
            ContentTypeHelper::Binary => ContentType::Binary,
        }
    }
}

mod hex_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = String::with_capacity(64);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.len() != 64 {
            return Err(serde::de::Error::custom("invalid hex hash length"));
        }
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|e| serde::de::Error::custom(e.to_string()))?;
        }
        Ok(out)
    }
}
