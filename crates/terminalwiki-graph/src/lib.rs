//! TerminalWiki Graph Layer
//!
//! Models relationships between wiki pages.

pub mod graph;
pub mod layout;
pub mod related;
pub mod render;

pub use graph::{
    BacklinkInfo, BrokenLink, Edge, EdgeKind, GraphEntry, GraphStats, Node, NodeKind, PageId,
    SubGraph, WikiGraph,
};
pub use layout::{export_dot, export_dot_subgraph, LayoutEngine};
pub use related::RelatedPage;
pub use render::render_graph;
