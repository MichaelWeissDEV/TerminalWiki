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

    pub fn find(&mut self, query: &str, limit: usize) -> Vec<FuzzyHit> {
        if query.trim().is_empty() {
            return self
                .items
                .iter()
                .take(limit)
                .map(|item| FuzzyHit {
                    wiki: item.wiki.clone(),
                    relative: item.relative.clone(),
                    title: item.title.clone(),
                    score: 0,
                    matched_indices: Vec::new(),
                })
                .collect();
        }

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut scored = Vec::new();

        let mut buf = Vec::new();
        for item in &self.items {
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
