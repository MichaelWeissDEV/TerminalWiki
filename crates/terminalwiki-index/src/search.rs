use crate::entry::IndexEntry;
use crate::query::{Query, QueryTerm};

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub entry: IndexEntry,
    pub score: f32,
    pub snippet: Option<String>,
}

fn matches_term(entry: &IndexEntry, term: &QueryTerm) -> bool {
    match term {
        QueryTerm::Text(t) => {
            let t_lower = t.to_lowercase();
            entry.title.to_lowercase().contains(&t_lower)
                || entry.body_text.to_lowercase().contains(&t_lower)
                || entry.relative.to_string_lossy().to_lowercase().contains(&t_lower)
        }
        QueryTerm::Tag(t) => entry.tags.iter().any(|tag| tag.eq_ignore_ascii_case(t)),
        QueryTerm::Wiki(w) => entry.wiki.eq_ignore_ascii_case(w),
        QueryTerm::Type(t) => {
            let t_lower = t.to_lowercase();
            match entry.content_type {
                terminalwiki_core::filetype::ContentType::Markdown => t_lower == "markdown" || t_lower == "md",
                terminalwiki_core::filetype::ContentType::Text => t_lower == "text" || t_lower == "txt",
                terminalwiki_core::filetype::ContentType::Code => t_lower == "code",
                terminalwiki_core::filetype::ContentType::Image => t_lower == "image",
                terminalwiki_core::filetype::ContentType::Latex => t_lower == "latex",
                terminalwiki_core::filetype::ContentType::Binary => t_lower == "binary",
            }
        }
        QueryTerm::Ext(e) => {
            if let Some(ext) = entry.relative.extension() {
                ext.to_string_lossy().eq_ignore_ascii_case(e)
            } else {
                false
            }
        }
        QueryTerm::Path(p) => entry.relative.to_string_lossy().to_lowercase().contains(&p.to_lowercase()),
        QueryTerm::Title(t) => entry.title.to_lowercase().contains(&t.to_lowercase()),
        QueryTerm::LinksTo(l) => entry.wiki_links.iter().any(|link| link.eq_ignore_ascii_case(l)),
        QueryTerm::Backlink(_) => false, // Handled separately or needs backlink index
        QueryTerm::Not(inner) => !matches_term(entry, inner),
    }
}

pub fn search(entries: &[IndexEntry], query: &Query) -> Vec<SearchResult> {
    let mut results = Vec::new();

    for entry in entries {
        let mut all_match = true;
        for term in &query.terms {
            if !matches_term(entry, term) {
                all_match = false;
                break;
            }
        }

        if all_match {
            // Simple scoring for now: 1.0
            results.push(SearchResult {
                entry: entry.clone(),
                score: 1.0,
                snippet: None, // could generate snippet from body_text
            });
        }
    }

    // `total_cmp` rather than `partial_cmp().unwrap()`: a NaN score would panic
    // inside a user-facing search path (spec Gate 28).
    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    results
}
