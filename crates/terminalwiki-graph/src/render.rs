//! ASCII/Unicode terminal rendering of a layout.
use crate::graph::{SubGraph, WikiGraph};
use std::collections::HashMap;
use terminalwiki_core::unicode::{display_width, truncate_display_width};

/// Marks the second cell of a double-width grapheme.
///
/// The canvas is a grid of cells that map 1:1 onto terminal columns. A CJK or
/// emoji grapheme occupies two columns, so it is stored in one cell and this
/// sentinel is written into the next; the sentinel is dropped when rows are
/// flattened to strings. Without it, one wide label would shift the rest of its
/// row right by a column and skew every edge drawn through it.
const WIDE_CONTINUATION: char = '\u{0}';

/// Where a node's label was placed on the canvas, in terminal columns.
///
/// The caller needs this to highlight a selected node: the rendered lines alone
/// cannot say which run of characters belongs to which node, and searching for
/// the title text breaks on duplicate or truncated labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelPlacement {
    /// Index into `WikiGraph::nodes`.
    pub node: usize,
    pub row: usize,
    /// First column of the label text.
    pub col: usize,
    /// Display width of the label as drawn (already truncated). Zero when no
    /// label could be placed — the marker is still reported so a caller can
    /// highlight the node itself.
    pub width: usize,
    /// The label text exactly as drawn, so callers can restyle it in place
    /// without re-deriving truncation.
    pub text: String,
    /// Column of the node's `•` / `●` marker.
    pub marker_col: usize,
}

/// A rendered graph canvas plus the label positions within it.
#[derive(Debug, Clone, Default)]
pub struct GraphRender {
    pub lines: Vec<String>,
    pub labels: Vec<LabelPlacement>,
}

/// Render graph layout to a character grid.
pub fn render_graph(
    graph: &WikiGraph,
    sub: &SubGraph,
    pos: &HashMap<usize, (f64, f64)>,
    width: usize,
    height: usize,
) -> GraphRender {
    if sub.nodes.is_empty() || width == 0 || height == 0 {
        return GraphRender {
            lines: vec![String::new(); height],
            labels: Vec::new(),
        };
    }

    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for &(x, y) in pos.values() {
        if x < min_x {
            min_x = x;
        }
        if x > max_x {
            max_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }

    // Add padding
    let pad = 2.0;
    min_x -= pad;
    max_x += pad;
    min_y -= pad;
    max_y += pad;

    let rx = max_x - min_x;
    let ry = max_y - min_y;
    let scale_x = if rx > 0.0 {
        (width as f64 - 1.0) / rx
    } else {
        1.0
    };
    let scale_y = if ry > 0.0 {
        (height as f64 - 1.0) / ry
    } else {
        1.0
    };

    let mut grid = vec![vec![' '; width]; height];

    let to_grid = |x: f64, y: f64| -> (isize, isize) {
        let gx = ((x - min_x) * scale_x).round() as isize;
        let gy = ((y - min_y) * scale_y).round() as isize;
        (gx, gy)
    };

    // Draw edges using Bresenham's
    let node_set: std::collections::HashSet<_> = sub.nodes.iter().copied().collect();
    for &e_idx in &sub.edges {
        let edge = &graph.edges[e_idx];
        if let Some(to) = edge.to {
            if node_set.contains(&edge.from) && node_set.contains(&to) {
                if let (Some(&(x1, y1)), Some(&(x2, y2))) = (pos.get(&edge.from), pos.get(&to)) {
                    let (gx1, gy1) = to_grid(x1, y1);
                    let (gx2, gy2) = to_grid(x2, y2);
                    draw_line(&mut grid, gx1, gy1, gx2, gy2);
                }
            }
        }
    }

    // Draw nodes on top of the edges, then their labels.
    let mut labels = Vec::new();
    for &n_idx in &sub.nodes {
        if let Some(&(x, y)) = pos.get(&n_idx) {
            let (gx, gy) = to_grid(x, y);
            if gx >= 0 && gx < width as isize && gy >= 0 && gy < height as isize {
                let (row, marker_col) = (gy as usize, gx as usize);
                let char_repr = if n_idx == sub.center { '●' } else { '•' };
                grid[row][marker_col] = char_repr;

                // A node with no room for a label still reports its marker, so
                // callers can always highlight the selected node.
                let mut placed = LabelPlacement {
                    node: n_idx,
                    row,
                    col: marker_col,
                    width: 0,
                    text: String::new(),
                    marker_col,
                };

                let lx = marker_col + 2;
                if lx < width {
                    let title = &graph.nodes[n_idx].title;
                    let label = truncate_display_width(title, width - lx);
                    let label_width = display_width(&label);

                    // A label may overwrite edge glyphs — a named node is worth
                    // more than the pixels of a line passing behind it — but
                    // never another label or marker, which would interleave two
                    // titles into unreadable text. The check is all-or-nothing
                    // so a label is either fully drawn or not drawn at all.
                    let span_free = label_width > 0
                        && lx + label_width <= width
                        && (lx..lx + label_width)
                            .all(|c| grid[row][c] == ' ' || is_edge_glyph(grid[row][c]));

                    if span_free {
                        let mut col = lx;
                        for g in split_graphemes(&label) {
                            let w = display_width(g);
                            if w == 0 || col + w > width {
                                break;
                            }
                            grid[row][col] = g.chars().next().unwrap_or(' ');
                            for cont in 1..w {
                                grid[row][col + cont] = WIDE_CONTINUATION;
                            }
                            col += w;
                        }
                        placed.col = lx;
                        placed.width = label_width;
                        placed.text = label;
                    }
                }

                labels.push(placed);
            }
        }
    }

    let lines = grid
        .into_iter()
        .map(|row| {
            row.into_iter()
                .filter(|&c| c != WIDE_CONTINUATION)
                .collect()
        })
        .collect();

    GraphRender { lines, labels }
}

/// True for the characters [`draw_line`] paints, which a label may cover.
fn is_edge_glyph(c: char) -> bool {
    matches!(c, '─' | '│' | '·')
}

/// Splits into grapheme clusters so combining marks and emoji sequences stay
/// intact when a label is drawn cell by cell.
fn split_graphemes(s: &str) -> impl Iterator<Item = &str> {
    use unicode_segmentation::UnicodeSegmentation;
    s.graphemes(true)
}

fn draw_line(grid: &mut [Vec<char>], mut x0: isize, mut y0: isize, x1: isize, y1: isize) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let height = grid.len() as isize;
    let width = if height > 0 {
        grid[0].len() as isize
    } else {
        0
    };

    loop {
        let ch = if dx > -dy {
            '─'
        } else if -dy > dx {
            '│'
        } else {
            '·'
        };

        if x0 >= 0 && x0 < width && y0 >= 0 && y0 < height && grid[y0 as usize][x0 as usize] == ' '
        {
            grid[y0 as usize][x0 as usize] = ch;
        }

        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
