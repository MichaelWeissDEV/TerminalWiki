use std::path::PathBuf;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use terminalwiki_core::filetype::ContentType;

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub wiki: String,
    pub path: PathBuf,
    pub relative: PathBuf,
    pub size: u64,
    pub mtime: u64,
    pub content_hash: [u8; 32],
    pub content_type: ContentType,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub headings: Vec<String>,
    pub body_text: String,
    pub wiki_links: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct IndexEntryDto {
    wiki: String,
    path: PathBuf,
    relative: PathBuf,
    size: u64,
    mtime: u64,
    content_hash: [u8; 32],
    content_type: String,
    title: String,
    aliases: Vec<String>,
    tags: Vec<String>,
    headings: Vec<String>,
    body_text: String,
    wiki_links: Vec<String>,
}

impl Serialize for IndexEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let ct_str = match self.content_type {
            ContentType::Markdown => "markdown",
            ContentType::Text => "text",
            ContentType::Code => "code",
            ContentType::Image => "image",
            ContentType::Latex => "latex",
            ContentType::Binary => "binary",
        };

        let dto = IndexEntryDto {
            wiki: self.wiki.clone(),
            path: self.path.clone(),
            relative: self.relative.clone(),
            size: self.size,
            mtime: self.mtime,
            content_hash: self.content_hash,
            content_type: ct_str.to_string(),
            title: self.title.clone(),
            aliases: self.aliases.clone(),
            tags: self.tags.clone(),
            headings: self.headings.clone(),
            body_text: self.body_text.clone(),
            wiki_links: self.wiki_links.clone(),
        };
        dto.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IndexEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let dto = IndexEntryDto::deserialize(deserializer)?;
        let content_type = match dto.content_type.as_str() {
            "markdown" => ContentType::Markdown,
            "text" => ContentType::Text,
            "code" => ContentType::Code,
            "image" => ContentType::Image,
            "latex" => ContentType::Latex,
            _ => ContentType::Binary,
        };

        Ok(IndexEntry {
            wiki: dto.wiki,
            path: dto.path,
            relative: dto.relative,
            size: dto.size,
            mtime: dto.mtime,
            content_hash: dto.content_hash,
            content_type,
            title: dto.title,
            aliases: dto.aliases,
            tags: dto.tags,
            headings: dto.headings,
            body_text: dto.body_text,
            wiki_links: dto.wiki_links,
        })
    }
}
