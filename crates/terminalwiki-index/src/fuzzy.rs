//! Nucleo-powered in-memory fuzzy finder (spec §13, §14).

use std::path::PathBuf;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};
use serde::{Deserialize, Serialize};

/// A single item searchable by the fuzzy finder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzyItem {
    pub wiki: String,
    pub relative: PathBuf,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
}

/// A matched result from the fuzzy finder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzyHit {
    pub wiki: String,
    pub relative: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    pub score: u32,
    pub matched_indices: Vec<u32>,
}

/// In-memory dataset for instant fuzzy page finding.
pub struct FuzzyDataset {
    items: Vec<FuzzyItem>,
    matcher: Matcher,
}

impl FuzzyDataset {
    pub fn new(items: Vec<FuzzyItem>) -> Self {
        Self {
            items,
            matcher: Matcher::new(NucleoConfig::DEFAULT),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Read-only view of every indexed page, used by the sidebar.
    pub fn items(&self) -> &[FuzzyItem] {
        &self.items
    }

    pub fn find(&mut self, query: &str, limit: usize) -> Vec<FuzzyHit> {
        self.find_with_tags(query, &[], limit)
    }

    /// Fuzzy-matches `query` and keeps only pages carrying every tag in
    /// `tags` (without the leading `#`). A hierarchical tag matches its
    /// subtree, so `security` also finds `security/web`.
    pub fn find_with_tags(&mut self, query: &str, tags: &[String], limit: usize) -> Vec<FuzzyHit> {
        let query = query.trim();
        if query.is_empty() {
            return self
                .items
                .iter()
                .filter(|item| item_has_tags(item, tags))
                .take(limit)
                .map(|item| FuzzyHit {
                    wiki: item.wiki.clone(),
                    relative: item.relative.clone(),
                    title: item.title.clone(),
                    tags: item.tags.clone(),
                    score: 0,
                    matched_indices: Vec::new(),
                })
                .collect();
        }

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut scored = Vec::new();

        let mut buf = Vec::new();
        for item in &self.items {
            if !item_has_tags(item, tags) {
                continue;
            }
            let rel_str = item.relative.to_string_lossy();
            let mut best_score = 0u32;
            let mut best_indices = Vec::new();

            // Match against Title (given higher priority)
            if !item.title.is_empty() {
                buf.clear();
                let title_utf32 = Utf32Str::new(&item.title, &mut buf);
                let mut indices = Vec::new();
                if let Some(score) = pattern.indices(title_utf32, &mut self.matcher, &mut indices) {
                    best_score = score + 200;
                    best_indices = indices;
                }
            }

            // Match against relative path
            buf.clear();
            let path_utf32 = Utf32Str::new(&rel_str, &mut buf);
            let mut indices = Vec::new();
            if let Some(score) = pattern.indices(path_utf32, &mut self.matcher, &mut indices) {
                if score > best_score {
                    best_score = score;
                    best_indices = indices;
                }
            }

            // Match against Aliases
            for alias in &item.aliases {
                buf.clear();
                let alias_utf32 = Utf32Str::new(alias, &mut buf);
                let mut indices = Vec::new();
                if let Some(score) = pattern.indices(alias_utf32, &mut self.matcher, &mut indices) {
                    let s = score + 150;
                    if s > best_score {
                        best_score = s;
                        best_indices = indices;
                    }
                }
            }

            if best_score > 0 {
                scored.push(FuzzyHit {
                    wiki: item.wiki.clone(),
                    relative: item.relative.clone(),
                    title: item.title.clone(),
                    tags: item.tags.clone(),
                    score: best_score,
                    matched_indices: best_indices,
                });
            }
        }

        scored.sort_by_key(|b| std::cmp::Reverse(b.score));
        scored.truncate(limit);
        scored
    }
}

fn item_has_tags(item: &FuzzyItem, tags: &[String]) -> bool {
    tags.iter().all(|wanted| {
        let wanted = normalize_tag(wanted);
        if wanted.is_empty() {
            return true;
        }
        item.tags.iter().any(|item_tag| {
            let item_tag = normalize_tag(item_tag);
            item_tag == wanted || item_tag.starts_with(&format!("{wanted}/"))
        })
    })
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('#').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dataset() -> FuzzyDataset {
        FuzzyDataset::new(vec![
            FuzzyItem {
                wiki: "vault".into(),
                relative: PathBuf::from("Rust.md"),
                title: "Rust".into(),
                aliases: Vec::new(),
                tags: vec!["programming".into(), "programming/rust".into()],
            },
            FuzzyItem {
                wiki: "vault".into(),
                relative: PathBuf::from("Cooking.md"),
                title: "Cooking".into(),
                aliases: Vec::new(),
                tags: vec!["home".into()],
            },
            FuzzyItem {
                wiki: "vault".into(),
                relative: PathBuf::from("Security.md"),
                title: "Security".into(),
                aliases: Vec::new(),
                tags: vec!["programming/security".into()],
            },
        ])
    }

    #[test]
    fn tag_filter_is_case_insensitive_and_includes_subtags() {
        let mut dataset = dataset();
        let hits = dataset.find_with_tags("", &["#programming".into()], 10);
        let titles: Vec<&str> = hits.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["Rust", "Security"]);
    }

    #[test]
    fn multiple_tags_must_all_match() {
        let mut dataset = dataset();
        let hits = dataset.find_with_tags(
            "",
            &["programming".into(), "programming/security".into()],
            10,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Security");
    }

    #[test]
    fn tag_filter_combines_with_text_query() {
        let mut dataset = dataset();
        let hits = dataset.find_with_tags("rus", &["programming".into()], 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Rust");
        assert_eq!(hits[0].tags, ["programming", "programming/rust"]);
    }
}
