//! Deterministic related pages scoring.
use crate::graph::WikiGraph;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Related page result
#[derive(Debug, Clone)]
pub struct RelatedPage {
    pub wiki: String,
    pub relative: PathBuf,
    pub title: String,
    pub score: f32,
    pub reasons: Vec<String>,
}

impl WikiGraph {
    /// Related pages, deterministically ranked
    pub fn related(&self, wiki: &str, relative: &std::path::Path, limit: usize) -> Vec<RelatedPage> {
        let key = Self::make_key(wiki, relative);
        let center_idx = match self.id_to_index.get(&key) {
            Some(&idx) => idx,
            None => return vec![],
        };

        let center_node = &self.nodes[center_idx];
        let mut scores: HashMap<usize, f32> = HashMap::new();
        let mut reasons: HashMap<usize, Vec<String>> = HashMap::new();

        let mut add_score = |idx: usize, score: f32, reason: &str| {
            if idx == center_idx {
                return;
            }
            *scores.entry(idx).or_insert(0.0) += score;
            reasons.entry(idx).or_default().push(reason.to_string());
        };

        // direct links: +10
        for &edge_idx in &self.outgoing[center_idx] {
            if let Some(to) = self.edges[edge_idx].to {
                add_score(to, 10.0, "Direct link");
            }
        }

        // backlinks: +8
        for &edge_idx in &self.incoming[center_idx] {
            let from = self.edges[edge_idx].from;
            add_score(from, 8.0, "Backlink");
        }

        // shared tags: +5 per tag
        let center_tags: HashSet<_> = center_node.tags.iter().collect();
        for (i, node) in self.nodes.iter().enumerate() {
            if i == center_idx {
                continue;
            }
            for tag in &node.tags {
                if center_tags.contains(tag) {
                    add_score(i, 5.0, &format!("Shared tag: {}", tag));
                }
            }
            
            // same directory: +3
            if let (Some(c_dir), Some(n_dir)) = (center_node.id.relative.parent(), node.id.relative.parent()) {
                if c_dir == n_dir && c_dir.as_os_str() != "" {
                    add_score(i, 3.0, "Same directory");
                }
            }
        }

        // common outgoing links: +2 per shared
        let center_outgoing: HashSet<_> = self.outgoing[center_idx].iter()
            .filter_map(|&e| self.edges[e].to)
            .collect();
            
        // common incoming links: +2 per shared
        let center_incoming: HashSet<_> = self.incoming[center_idx].iter()
            .map(|&e| self.edges[e].from)
            .collect();

        for (i, _) in self.nodes.iter().enumerate() {
            if i == center_idx {
                continue;
            }
            
            let i_outgoing: HashSet<_> = self.outgoing[i].iter()
                .filter_map(|&e| self.edges[e].to)
                .collect();
            let shared_out = center_outgoing.intersection(&i_outgoing).count();
            if shared_out > 0 {
                add_score(i, 2.0 * shared_out as f32, &format!("{} shared outgoing links", shared_out));
            }

            let i_incoming: HashSet<_> = self.incoming[i].iter()
                .map(|&e| self.edges[e].from)
                .collect();
            let shared_in = center_incoming.intersection(&i_incoming).count();
            if shared_in > 0 {
                add_score(i, 2.0 * shared_in as f32, &format!("{} shared incoming links", shared_in));
            }
        }

        let mut results: Vec<_> = scores.into_iter().map(|(idx, score)| {
            let node = &self.nodes[idx];
            RelatedPage {
                wiki: node.id.wiki.clone(),
                relative: node.id.relative.clone(),
                title: node.title.clone(),
                score,
                reasons: reasons.remove(&idx).unwrap_or_default(),
            }
        }).collect();

        // Sort by score descending, then by title to be deterministic
        results.sort_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap().then_with(|| a.title.cmp(&b.title))
        });

        results.truncate(limit);
        results
    }
}
