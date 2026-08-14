//! ASCII/Unicode terminal rendering of a layout.
use crate::graph::{SubGraph, WikiGraph};
use std::collections::HashMap;

/// Render graph layout to character grid
pub fn render_graph(
    graph: &WikiGraph,
    sub: &SubGraph,
    pos: &HashMap<usize, (f64, f64)>,
    width: usize,
    height: usize,
) -> Vec<String> {
    if sub.nodes.is_empty() || width == 0 || height == 0 {
        return vec![String::new(); height];
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

    // Draw nodes
    for &n_idx in &sub.nodes {
        if let Some(&(x, y)) = pos.get(&n_idx) {
            let (gx, gy) = to_grid(x, y);
            if gx >= 0 && gx < width as isize && gy >= 0 && gy < height as isize {
                let char_repr = if n_idx == sub.center { '●' } else { '•' };
                grid[gy as usize][gx as usize] = char_repr;

                // Add label if possible
                let node = &graph.nodes[n_idx];
                let label: Vec<char> = node.title.chars().collect();
                let lx = gx + 2;
                if lx >= 0 && lx + label.len() as isize <= width as isize {
                    for (i, &c) in label.iter().enumerate() {
                        let cx = (lx as usize) + i;
                        if grid[gy as usize][cx] == ' ' {
                            grid[gy as usize][cx] = c;
                        }
                    }
                }
            }
        }
    }

    grid.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
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
