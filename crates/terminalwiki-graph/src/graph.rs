//! Wiki link graph implementation.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Node types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Page,
    CodeFile,
    Asset,
    Wiki,
}

/// Edge types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    LinksTo,
    Embeds,
    References,
    Mounts,
}

/// Identifier for a page
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageId {
    pub wiki: String,
    pub relative: PathBuf,
}

/// A node in the graph
#[derive(Debug, Clone)]
pub struct Node {
    pub id: PageId,
    pub kind: NodeKind,
    pub title: String,
    pub tags: Vec<String>,
}

/// An edge in the graph
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: usize,
    pub to: Option<usize>,
    pub kind: EdgeKind,
    pub broken: bool,
    pub target: String,
}

/// Information about a backlink
#[derive(Debug, Clone)]
pub struct BacklinkInfo {
    pub from_wiki: String,
    pub from_relative: PathBuf,
    pub from_title: String,
    pub context: String,
}

/// Information about a broken link
#[derive(Debug, Clone)]
pub struct BrokenLink {
    pub from_wiki: String,
    pub from_relative: PathBuf,
    pub target: String,
}

/// Global stats for the graph
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub broken_links: usize,
}

/// Input format for building the graph
#[derive(Debug, Clone)]
pub struct GraphEntry {
    pub wiki: String,
    pub relative: PathBuf,
    pub content_type: String,
    pub title: String,
    pub tags: Vec<String>,
    pub wiki_links: Vec<String>,
    pub image_links: Vec<String>,
}

/// A subset of the graph
#[derive(Debug, Clone)]
pub struct SubGraph {
    pub nodes: Vec<usize>,
    pub edges: Vec<usize>,
    pub center: usize,
}

/// The entire wiki graph
pub struct WikiGraph {
    pub(crate) nodes: Vec<Node>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) id_to_index: HashMap<String, usize>,
    pub(crate) outgoing: Vec<Vec<usize>>,
    pub(crate) incoming: Vec<Vec<usize>>,
}

impl WikiGraph {
    /// Format key for id_to_index map
    pub(crate) fn make_key(wiki: &str, relative: &Path) -> String {
        format!("{}:{}", wiki, relative.display())
    }

    /// Build the graph from a set of entries.
    pub fn from_entries(entries: &[GraphEntry]) -> Self {
        let mut nodes = Vec::new();
        let mut id_to_index = HashMap::new();

        // Pass 1: create all nodes
        for entry in entries {
            let key = Self::make_key(&entry.wiki, &entry.relative);
            let kind = match entry.content_type.as_str() {
                "markdown" => NodeKind::Page,
                "code" => NodeKind::CodeFile,
                _ => NodeKind::Asset, // image, etc.
            };

            let node_idx = nodes.len();
            nodes.push(Node {
                id: PageId {
                    wiki: entry.wiki.clone(),
                    relative: entry.relative.clone(),
                },
                kind,
                title: entry.title.clone(),
                tags: entry.tags.clone(),
            });
            id_to_index.insert(key, node_idx);
        }

        let num_nodes = nodes.len();
        let mut edges = Vec::new();
        let mut outgoing = vec![Vec::new(); num_nodes];
        let mut incoming = vec![Vec::new(); num_nodes];

        // Pass 2: create edges
        for (from_idx, entry) in entries.iter().enumerate() {
            // Process wiki links
            for link in &entry.wiki_links {
                // simple target resolution: check if link exactly matches the file name (without ext) or relative path
                // For simplicity here, we assume link is relative path without extension or exact
                // In a real wiki we'd check all wikis if wiki name not specified
                // Let's just find a matching node
                let target_path = PathBuf::from(format!("{}.md", link));
                let exact_key = Self::make_key(&entry.wiki, &target_path);
                
                // For this mock implementation we'll look it up by checking title or path matches
                let mut target_idx = id_to_index.get(&exact_key).copied();
                if target_idx.is_none() {
                    // Try to find any node matching the link as stem or title
                    target_idx = nodes.iter().position(|n| {
                        let n_stem = n.id.relative.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        n_stem == link || n.title == *link
                    });
                }

                let edge_idx = edges.len();
                edges.push(Edge {
                    from: from_idx,
                    to: target_idx,
                    kind: EdgeKind::LinksTo,
                    broken: target_idx.is_none(),
                    target: link.clone(),
                });

                outgoing[from_idx].push(edge_idx);
                if let Some(to) = target_idx {
                    incoming[to].push(edge_idx);
                }
            }

            // Process image links
            for link in &entry.image_links {
                // find asset
                let target_idx = nodes.iter().position(|n| {
                    n.id.relative.to_string_lossy().contains(link)
                });

                let edge_idx = edges.len();
                edges.push(Edge {
                    from: from_idx,
                    to: target_idx,
                    kind: EdgeKind::Embeds,
                    broken: target_idx.is_none(),
                    target: link.clone(),
                });

                outgoing[from_idx].push(edge_idx);
                if let Some(to) = target_idx {
                    incoming[to].push(edge_idx);
                }
            }
        }

        Self {
            nodes,
            edges,
            id_to_index,
            outgoing,
            incoming,
        }
    }

    /// All outgoing link targets from a page
    pub fn outgoing_links(&self, wiki: &str, relative: &Path) -> Vec<&Edge> {
        let key = Self::make_key(wiki, relative);
        if let Some(&idx) = self.id_to_index.get(&key) {
            self.outgoing[idx].iter().map(|&e| &self.edges[e]).collect()
        } else {
            Vec::new()
        }
    }

    /// All pages that link TO this page
    pub fn backlinks(&self, wiki: &str, relative: &Path) -> Vec<BacklinkInfo> {
        let key = Self::make_key(wiki, relative);
        if let Some(&idx) = self.id_to_index.get(&key) {
            self.incoming[idx]
                .iter()
                .map(|&e| {
                    let edge = &self.edges[e];
                    let from_node = &self.nodes[edge.from];
                    BacklinkInfo {
                        from_wiki: from_node.id.wiki.clone(),
                        from_relative: from_node.id.relative.clone(),
                        from_title: from_node.title.clone(),
                        context: String::new(), // Not implemented yet
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Broken links
    pub fn broken_links(&self) -> Vec<BrokenLink> {
        self.edges
            .iter()
            .filter(|e| e.broken)
            .map(|e| {
                let from_node = &self.nodes[e.from];
                BrokenLink {
                    from_wiki: from_node.id.wiki.clone(),
                    from_relative: from_node.id.relative.clone(),
                    target: e.target.clone(),
                }
            })
            .collect()
    }

    /// Neighborhood up to `depth` hops
    pub fn neighborhood(&self, wiki: &str, relative: &Path, depth: usize) -> SubGraph {
        let key = Self::make_key(wiki, relative);
        let center = match self.id_to_index.get(&key) {
            Some(&idx) => idx,
            None => return SubGraph { nodes: vec![], edges: vec![], center: 0 },
        };

        let mut visited_nodes = HashSet::new();
        let mut visited_edges = HashSet::new();
        let mut queue = vec![(center, 0)];
        visited_nodes.insert(center);

        while let Some((node_idx, current_depth)) = queue.pop() {
            if current_depth >= depth {
                continue;
            }

            // traverse outgoing
            for &edge_idx in &self.outgoing[node_idx] {
                visited_edges.insert(edge_idx);
                let edge = &self.edges[edge_idx];
                if let Some(to) = edge.to {
                    if visited_nodes.insert(to) {
                        queue.push((to, current_depth + 1));
                    }
                }
            }

            // traverse incoming
            for &edge_idx in &self.incoming[node_idx] {
                visited_edges.insert(edge_idx);
                let edge = &self.edges[edge_idx];
                if visited_nodes.insert(edge.from) {
                    queue.push((edge.from, current_depth + 1));
                }
            }
        }

        let mut nodes: Vec<_> = visited_nodes.into_iter().collect();
        let mut edges: Vec<_> = visited_edges.into_iter().collect();
        nodes.sort_unstable();
        edges.sort_unstable();

        SubGraph { nodes, edges, center }
    }

    /// Global stats
    pub fn stats(&self) -> GraphStats {
        GraphStats {
            total_nodes: self.nodes.len(),
            total_edges: self.edges.len(),
            broken_links: self.edges.iter().filter(|e| e.broken).count(),
        }
    }
}
