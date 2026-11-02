//! Wiki link graph implementation.
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// Node types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Page,
    Latex,
    Bibtex,
    Pdf,
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
    SharedTag(String),
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

    /// All nodes, indexed by the indices used in [`SubGraph::nodes`].
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// All edges, indexed by the indices used in [`SubGraph::edges`].
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Returns the node at `idx`, or `None` if the index is out of range.
    pub fn node(&self, idx: usize) -> Option<&Node> {
        self.nodes.get(idx)
    }

    /// Looks up the node index for a given wiki and relative path.
    pub fn find_node_index(&self, wiki: &str, relative: &Path) -> Option<usize> {
        let key = Self::make_key(wiki, relative);
        self.id_to_index.get(&key).copied()
    }

    /// Build the graph from a set of entries.
    pub fn from_entries(entries: &[GraphEntry]) -> Self {
        let mut nodes = Vec::new();
        let mut id_to_index = HashMap::new();

        // Pass 1: create all nodes
        for entry in entries {
            let key = Self::make_key(&entry.wiki, &entry.relative);
            let ext = entry
                .relative
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            let kind = match entry.content_type.as_str() {
                "markdown" => NodeKind::Page,
                "latex" => {
                    if ext == "bib" {
                        NodeKind::Bibtex
                    } else {
                        NodeKind::Latex
                    }
                }
                "code" => NodeKind::CodeFile,
                _ => {
                    if ext == "pdf" {
                        NodeKind::Pdf
                    } else if ext == "bib" {
                        NodeKind::Bibtex
                    } else if ext == "tex" || ext == "latex" {
                        NodeKind::Latex
                    } else {
                        NodeKind::Asset
                    }
                }
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

        // Pass 2: create edges from links
        for (from_idx, entry) in entries.iter().enumerate() {
            // Process wiki links and document references
            for link in &entry.wiki_links {
                let candidates = [
                    PathBuf::from(link),
                    PathBuf::from(format!("{link}.md")),
                    PathBuf::from(format!("{link}.tex")),
                    PathBuf::from(format!("{link}.pdf")),
                    PathBuf::from(format!("{link}.bib")),
                ];
                let mut target_idx = None;
                for cand in &candidates {
                    let key = Self::make_key(&entry.wiki, cand);
                    if let Some(&idx) = id_to_index.get(&key) {
                        target_idx = Some(idx);
                        break;
                    }
                }
                if target_idx.is_none() {
                    let link_lower = link.to_ascii_lowercase();
                    target_idx = nodes.iter().position(|n| {
                        let n_stem =
                            n.id.relative
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_ascii_lowercase();
                        let n_rel = n.id.relative.to_string_lossy().to_ascii_lowercase();
                        let n_title = n.title.to_ascii_lowercase();
                        n_stem == link_lower || n_rel == link_lower || n_title == link_lower
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

            // Process image and asset embeds
            for link in &entry.image_links {
                let target_idx = nodes
                    .iter()
                    .position(|n| n.id.relative.to_string_lossy().contains(link));

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

        // Pass 3: group nodes by shared tags (semantic tag clustering)
        let mut tag_to_nodes: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, node) in nodes.iter().enumerate() {
            for tag in &node.tags {
                tag_to_nodes
                    .entry(tag.to_ascii_lowercase())
                    .or_default()
                    .push(idx);
            }
        }
        for (tag, tagged_indices) in tag_to_nodes {
            if tagged_indices.len() > 1 && tagged_indices.len() <= 25 {
                for window in tagged_indices.windows(2) {
                    let (u, v) = (window[0], window[1]);
                    if u != v && !outgoing[u].iter().any(|&e| edges[e].to == Some(v)) {
                        let edge_idx = edges.len();
                        edges.push(Edge {
                            from: u,
                            to: Some(v),
                            kind: EdgeKind::SharedTag(tag.clone()),
                            broken: false,
                            target: tag.clone(),
                        });
                        outgoing[u].push(edge_idx);
                        incoming[v].push(edge_idx);
                    }
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
        let Some(&idx) = self.id_to_index.get(&key) else {
            return Vec::new();
        };

        // One backlink per linking page. A page that references the target
        // several times is still a single source, and listing it repeatedly
        // would both read as noise and inflate the backlink count.
        let mut seen = HashSet::new();
        self.incoming[idx]
            .iter()
            .filter_map(|&e| {
                let edge = &self.edges[e];
                if !seen.insert(edge.from) {
                    return None;
                }
                let from_node = &self.nodes[edge.from];
                Some(BacklinkInfo {
                    from_wiki: from_node.id.wiki.clone(),
                    from_relative: from_node.id.relative.clone(),
                    from_title: from_node.title.clone(),
                    context: String::new(), // Not implemented yet
                })
            })
            .collect()
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
        self.neighborhood_limited(wiki, relative, depth, usize::MAX)
    }

    /// Like [`WikiGraph::neighborhood`], but stops once `max_nodes` have been
    /// collected.
    ///
    /// The traversal is breadth-first, so the cap keeps the nodes *nearest* the
    /// centre and discards the outer fringe. Capping here rather than after
    /// layout matters: the layout engine is O(n²) below its 200-node threshold,
    /// so an uncapped hub neighbourhood would pay the full cost only to have the
    /// result thrown away.
    pub fn neighborhood_limited(
        &self,
        wiki: &str,
        relative: &Path,
        depth: usize,
        max_nodes: usize,
    ) -> SubGraph {
        let key = Self::make_key(wiki, relative);
        let center = match self.id_to_index.get(&key) {
            Some(&idx) => idx,
            None => {
                return SubGraph {
                    nodes: vec![],
                    edges: vec![],
                    center: 0,
                }
            }
        };

        let mut visited_nodes = HashSet::new();
        let mut visited_edges = HashSet::new();
        // A queue, not a stack: breadth-first order is what makes `max_nodes`
        // keep the centre's nearest neighbours rather than one deep arm.
        let mut queue = VecDeque::from([(center, 0usize)]);
        visited_nodes.insert(center);

        'outer: while let Some((node_idx, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }

            // traverse outgoing
            for &edge_idx in &self.outgoing[node_idx] {
                visited_edges.insert(edge_idx);
                let edge = &self.edges[edge_idx];
                if let Some(to) = edge.to {
                    if visited_nodes.len() >= max_nodes {
                        break 'outer;
                    }
                    if visited_nodes.insert(to) {
                        queue.push_back((to, current_depth + 1));
                    }
                }
            }

            // traverse incoming
            for &edge_idx in &self.incoming[node_idx] {
                visited_edges.insert(edge_idx);
                let edge = &self.edges[edge_idx];
                if visited_nodes.len() >= max_nodes {
                    break 'outer;
                }
                if visited_nodes.insert(edge.from) {
                    queue.push_back((edge.from, current_depth + 1));
                }
            }
        }

        // Also collect ALL interconnecting edges between all visited nodes
        for &u in &visited_nodes {
            for &edge_idx in &self.outgoing[u] {
                let edge = &self.edges[edge_idx];
                if let Some(to) = edge.to {
                    if visited_nodes.contains(&to) {
                        visited_edges.insert(edge_idx);
                    }
                }
            }
        }

        // Drop edges whose endpoints did not survive the cap.
        visited_edges.retain(|&e_idx| {
            let edge = &self.edges[e_idx];
            visited_nodes.contains(&edge.from)
                && edge.to.is_some_and(|to| visited_nodes.contains(&to))
        });

        let mut nodes: Vec<_> = visited_nodes.into_iter().collect();
        let mut edges: Vec<_> = visited_edges.into_iter().collect();
        nodes.sort_unstable();
        edges.sort_unstable();

        SubGraph {
            nodes,
            edges,
            center,
        }
    }

    /// Global graph containing all nodes in the wiki (or up to `max_nodes`)
    /// and all interconnecting links between them.
    pub fn full_graph(&self, max_nodes: usize, center_opt: Option<usize>) -> SubGraph {
        let center = center_opt.unwrap_or(0);
        let n = self.nodes.len().min(max_nodes);
        let nodes: Vec<usize> = (0..n).collect();
        let node_set: HashSet<usize> = nodes.iter().copied().collect();
        let mut edges = Vec::new();
        for (i, edge) in self.edges.iter().enumerate() {
            if let Some(to) = edge.to {
                if node_set.contains(&edge.from) && node_set.contains(&to) {
                    edges.push(i);
                }
            }
        }
        SubGraph {
            nodes,
            edges,
            center,
        }
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
