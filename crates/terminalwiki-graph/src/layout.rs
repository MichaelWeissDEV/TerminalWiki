//! Graph layout algorithms and export.
use crate::graph::{SubGraph, WikiGraph};
use std::collections::{HashMap, VecDeque};

pub struct LayoutEngine;

impl LayoutEngine {
    /// Compute a force-directed or hierarchical layout
    /// Returns node positions (x, y) indexed by the subgraph node indices
    pub fn compute_layout(
        graph: &WikiGraph,
        sub: &SubGraph,
    ) -> HashMap<usize, (f64, f64)> {
        let n = sub.nodes.len();
        if n == 0 {
            return HashMap::new();
        }
        if n == 1 {
            let mut pos = HashMap::new();
            pos.insert(sub.nodes[0], (0.0, 0.0));
            return pos;
        }

        if n < 200 {
            Self::force_directed(graph, sub)
        } else {
            Self::bfs_ring_layout(graph, sub)
        }
    }

    fn force_directed(graph: &WikiGraph, sub: &SubGraph) -> HashMap<usize, (f64, f64)> {
        let mut pos = HashMap::new();
        // Initialize positions in a circle
        let n = sub.nodes.len();
        for (i, &node_idx) in sub.nodes.iter().enumerate() {
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            pos.insert(node_idx, (angle.cos() * 10.0, angle.sin() * 10.0));
        }

        let k = 1.0;
        let iter_count = 100;
        let dt = 0.1;

        let node_set: std::collections::HashSet<_> = sub.nodes.iter().copied().collect();

        for _ in 0..iter_count {
            let mut forces: HashMap<usize, (f64, f64)> = sub.nodes.iter().map(|&id| (id, (0.0, 0.0))).collect();

            // Repulsion
            for &u in &sub.nodes {
                for &v in &sub.nodes {
                    if u == v { continue; }
                    let pu = pos[&u];
                    let pv = pos[&v];
                    let dx = pu.0 - pv.0;
                    let dy = pu.1 - pv.1;
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq > 0.0001 {
                        let f = k * k / dist_sq.sqrt();
                        let fx = f * dx;
                        let fy = f * dy;
                        let cur = forces[&u];
                        forces.insert(u, (cur.0 + fx, cur.1 + fy));
                    }
                }
            }

            // Attraction
            for &edge_idx in &sub.edges {
                let edge = &graph.edges[edge_idx];
                if let Some(to) = edge.to {
                    if node_set.contains(&edge.from) && node_set.contains(&to) {
                        let pu = pos[&edge.from];
                        let pv = pos[&to];
                        let dx = pv.0 - pu.0;
                        let dy = pv.1 - pu.1;
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist > 0.0001 {
                            let f = dist * dist / k;
                            let fx = f * dx / dist;
                            let fy = f * dy / dist;
                            
                            let cur_u = forces[&edge.from];
                            forces.insert(edge.from, (cur_u.0 + fx, cur_u.1 + fy));
                            
                            let cur_v = forces[&to];
                            forces.insert(to, (cur_v.0 - fx, cur_v.1 - fy));
                        }
                    }
                }
            }

            // Apply forces
            for &u in &sub.nodes {
                let p = pos[&u];
                let f = forces[&u];
                // simple cap on force
                let f_mag = (f.0 * f.0 + f.1 * f.1).sqrt();
                let max_disp = 5.0;
                let scale = if f_mag > max_disp { max_disp / f_mag } else { 1.0 };
                pos.insert(u, (p.0 + f.0 * scale * dt, p.1 + f.1 * scale * dt));
            }
        }
        pos
    }

    fn bfs_ring_layout(graph: &WikiGraph, sub: &SubGraph) -> HashMap<usize, (f64, f64)> {
        let mut pos = HashMap::new();
        let center = sub.center;
        
        let mut queue = VecDeque::new();
        let mut depths = HashMap::new();
        queue.push_back(center);
        depths.insert(center, 0);

        let node_set: std::collections::HashSet<_> = sub.nodes.iter().copied().collect();
        let mut visited = std::collections::HashSet::new();
        visited.insert(center);

        while let Some(u) = queue.pop_front() {
            let d = depths[&u];
            // simplified: we just get depth of each node relative to center
            
            // neighbors
            // outgoing
            for &e_idx in &graph.outgoing[u] {
                if sub.edges.contains(&e_idx) {
                    if let Some(to) = graph.edges[e_idx].to {
                        if node_set.contains(&to) && visited.insert(to) {
                            depths.insert(to, d + 1);
                            queue.push_back(to);
                        }
                    }
                }
            }
            // incoming
            for &e_idx in &graph.incoming[u] {
                if sub.edges.contains(&e_idx) {
                    let from = graph.edges[e_idx].from;
                    if node_set.contains(&from) && visited.insert(from) {
                        depths.insert(from, d + 1);
                        queue.push_back(from);
                    }
                }
            }
        }

        // assign positions by depth
        let mut depth_groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for &u in &sub.nodes {
            let d = depths.get(&u).copied().unwrap_or(0);
            depth_groups.entry(d).or_default().push(u);
        }

        for (d, nodes_at_d) in depth_groups {
            if d == 0 {
                for &u in &nodes_at_d {
                    pos.insert(u, (0.0, 0.0));
                }
            } else {
                let r = (d as f64) * 15.0;
                let k = nodes_at_d.len();
                for (i, &u) in nodes_at_d.iter().enumerate() {
                    let angle = 2.0 * std::f64::consts::PI * (i as f64) / (k as f64);
                    pos.insert(u, (angle.cos() * r, angle.sin() * r));
                }
            }
        }
        pos
    }
}

pub fn export_dot(graph: &WikiGraph) -> String {
    let mut out = String::from("digraph G {\n");
    for node in &graph.nodes {
        out.push_str(&format!("  \"{}\" [label=\"{}\"];\n", node.id.relative.display(), node.title));
    }
    for edge in &graph.edges {
        if let Some(to) = edge.to {
            let from_node = &graph.nodes[edge.from];
            let to_node = &graph.nodes[to];
            out.push_str(&format!("  \"{}\" -> \"{}\";\n", from_node.id.relative.display(), to_node.id.relative.display()));
        }
    }
    out.push_str("}\n");
    out
}

pub fn export_dot_subgraph(graph: &WikiGraph, sub: &SubGraph) -> String {
    let mut out = String::from("digraph G {\n");
    let node_set: std::collections::HashSet<_> = sub.nodes.iter().copied().collect();

    for &n_idx in &sub.nodes {
        let node = &graph.nodes[n_idx];
        out.push_str(&format!("  \"{}\" [label=\"{}\"];\n", node.id.relative.display(), node.title));
    }
    for &e_idx in &sub.edges {
        let edge = &graph.edges[e_idx];
        if let Some(to) = edge.to {
            if node_set.contains(&edge.from) && node_set.contains(&to) {
                let from_node = &graph.nodes[edge.from];
                let to_node = &graph.nodes[to];
                out.push_str(&format!("  \"{}\" -> \"{}\";\n", from_node.id.relative.display(), to_node.id.relative.display()));
            }
        }
    }
    out.push_str("}\n");
    out
}
