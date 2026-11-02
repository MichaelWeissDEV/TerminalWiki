//! Native Unicode diagram and chart renderer for Mermaid, DOT, and XY/Bar charts.
//!
//! Converts declarative fenced blocks (`mermaid`, `dot`, `chart`, `plot`, `xychart-beta`)
//! into structured terminal box-and-arrow diagrams and Unicode bar/line charts
//! without executing external processes.

use terminalwiki_core::unicode::{display_width, pad_display_width};

use crate::document::{RenderedLine, Span};
use crate::theme::{SemanticColor, Theme};

/// Returns true if `lang` is a diagram or chart fence language handled natively.
pub fn is_diagram_language(lang: Option<&str>) -> bool {
    let Some(raw) = lang else {
        return false;
    };
    let token = raw
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        token.as_str(),
        "mermaid" | "dot" | "graphviz" | "chart" | "plot" | "xychart" | "xychart-beta"
    )
}

/// Renders a diagram or chart code block into styled terminal lines.
/// Returns `None` if the block does not contain recognizable diagram/chart items,
/// allowing the caller to fall back to syntax-highlighted code.
pub fn render_diagram_block(
    lang: &str,
    content: &str,
    indent: &str,
    max_width: usize,
    theme: &Theme,
) -> Option<Vec<RenderedLine>> {
    let token = lang
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    match token.as_str() {
        "chart" | "plot" | "xychart" | "xychart-beta" => {
            render_chart(trimmed, indent, max_width, theme)
        }
        "mermaid" => {
            if trimmed.starts_with("xychart")
                || trimmed.starts_with("pie")
                || trimmed.starts_with("bar")
            {
                render_chart(trimmed, indent, max_width, theme)
            } else {
                render_flowchart(trimmed, indent, max_width, theme)
            }
        }
        "dot" | "graphviz" => render_flowchart(trimmed, indent, max_width, theme),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct FlowEdge {
    from_id: String,
    from_label: String,
    to_id: String,
    to_label: String,
    edge_label: Option<String>,
}

fn render_flowchart(
    content: &str,
    indent: &str,
    max_width: usize,
    theme: &Theme,
) -> Option<Vec<RenderedLine>> {
    let mut horizontal = false;
    let mut edges = Vec::new();
    let mut standalone_nodes: Vec<(String, String)> = Vec::new();
    let mut labels = std::collections::HashMap::<String, String>::new();

    for raw_line in content.lines() {
        let line = raw_line.trim().trim_end_matches(';').trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with("//") || line == "}" {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("graph ")
            || lower.starts_with("flowchart ")
            || lower.starts_with("digraph")
            || lower.starts_with("strict ")
            || lower.starts_with("subgraph")
            || lower == "end"
        {
            if lower.contains(" lr") || lower.contains(" rl") {
                horizontal = true;
            }
            continue;
        }

        if let Some((left, edge_lbl, right)) = split_arrow(line) {
            let (from_id, from_lbl) = parse_node_spec(left);
            let (to_id, to_lbl) = parse_node_spec(right);
            if !from_id.is_empty() && !to_id.is_empty() {
                let from_label = from_lbl
                    .or_else(|| labels.get(&from_id).cloned())
                    .unwrap_or_else(|| from_id.clone());
                let to_label = to_lbl
                    .or_else(|| labels.get(&to_id).cloned())
                    .unwrap_or_else(|| to_id.clone());
                labels.insert(from_id.clone(), from_label.clone());
                labels.insert(to_id.clone(), to_label.clone());
                edges.push(FlowEdge {
                    from_id,
                    from_label,
                    to_id,
                    to_label,
                    edge_label: edge_lbl,
                });
            }
        } else {
            let (id, lbl) = parse_node_spec(line);
            if let Some(l) = lbl {
                labels.insert(id.clone(), l.clone());
                standalone_nodes.push((id, l));
            }
        }
    }

    if edges.is_empty() && standalone_nodes.is_empty() {
        return None;
    }

    // Refresh labels in edges
    for edge in &mut edges {
        if let Some(l) = labels.get(&edge.from_id) {
            edge.from_label = l.clone();
        }
        if let Some(l) = labels.get(&edge.to_id) {
            edge.to_label = l.clone();
        }
    }

    let mut lines = Vec::new();
    let border_style = theme.style(SemanticColor::Accent);
    let node_style = theme.style(SemanticColor::Heading).bold();
    let muted_style = theme.style(SemanticColor::Muted);

    lines.push(vec![
        Span {
            text: format!("{indent}┌─ "),
            style: muted_style.clone(),
        },
        Span {
            text: "Diagram".to_string(),
            style: border_style.clone().bold(),
        },
    ]);

    if horizontal && edges.len() <= 4 {
        // Compact horizontal chain rendering for LR diagrams
        let mut top_row = vec![Span {
            text: format!("{indent}│  "),
            style: muted_style.clone(),
        }];
        let mut mid_row = vec![Span {
            text: format!("{indent}│  "),
            style: muted_style.clone(),
        }];
        let mut bot_row = vec![Span {
            text: format!("{indent}│  "),
            style: muted_style.clone(),
        }];

        for (idx, edge) in edges.iter().enumerate() {
            if idx == 0 {
                let w = display_width(&edge.from_label) + 2;
                top_row.push(Span {
                    text: format!("╭{}╮", "─".repeat(w)),
                    style: border_style.clone(),
                });
                mid_row.push(Span {
                    text: "│ ".to_string(),
                    style: border_style.clone(),
                });
                mid_row.push(Span {
                    text: edge.from_label.clone(),
                    style: node_style.clone(),
                });
                mid_row.push(Span {
                    text: " │".to_string(),
                    style: border_style.clone(),
                });
                bot_row.push(Span {
                    text: format!("╰{}╯", "─".repeat(w)),
                    style: border_style.clone(),
                });
            }

            let arrow = if let Some(ref lbl) = edge.edge_label {
                format!(" ─[{lbl}]─▶ ")
            } else {
                " ───▶ ".to_string()
            };
            let arrow_w = display_width(&arrow);
            top_row.push(Span {
                text: " ".repeat(arrow_w),
                style: muted_style.clone(),
            });
            mid_row.push(Span {
                text: arrow,
                style: muted_style.clone(),
            });
            bot_row.push(Span {
                text: " ".repeat(arrow_w),
                style: muted_style.clone(),
            });

            let w = display_width(&edge.to_label) + 2;
            top_row.push(Span {
                text: format!("╭{}╮", "─".repeat(w)),
                style: border_style.clone(),
            });
            mid_row.push(Span {
                text: "│ ".to_string(),
                style: border_style.clone(),
            });
            mid_row.push(Span {
                text: edge.to_label.clone(),
                style: node_style.clone(),
            });
            mid_row.push(Span {
                text: " │".to_string(),
                style: border_style.clone(),
            });
            bot_row.push(Span {
                text: format!("╰{}╯", "─".repeat(w)),
                style: border_style.clone(),
            });
        }
        lines.push(top_row);
        lines.push(mid_row);
        lines.push(bot_row);
    } else {
        // Vertical box-and-arrow flow
        for edge in &edges {
            let left_w = display_width(&edge.from_label).clamp(4, max_width.saturating_sub(16));
            let right_w = display_width(&edge.to_label).clamp(4, max_width.saturating_sub(16));
            let arrow = if let Some(ref lbl) = edge.edge_label {
                format!(" ──({lbl})──▶ ")
            } else {
                " ───▶ ".to_string()
            };
            lines.push(vec![
                Span {
                    text: format!("{indent}│  ╭{}╮", "─".repeat(left_w + 2)),
                    style: border_style.clone(),
                },
                Span {
                    text: " ".repeat(display_width(&arrow)),
                    style: muted_style.clone(),
                },
                Span {
                    text: format!("╭{}╮", "─".repeat(right_w + 2)),
                    style: border_style.clone(),
                },
            ]);
            lines.push(vec![
                Span {
                    text: format!("{indent}│  │ "),
                    style: border_style.clone(),
                },
                Span {
                    text: pad_display_width(&edge.from_label, left_w),
                    style: node_style.clone(),
                },
                Span {
                    text: " │".to_string(),
                    style: border_style.clone(),
                },
                Span {
                    text: arrow.clone(),
                    style: muted_style.clone(),
                },
                Span {
                    text: "│ ".to_string(),
                    style: border_style.clone(),
                },
                Span {
                    text: pad_display_width(&edge.to_label, right_w),
                    style: node_style.clone(),
                },
                Span {
                    text: " │".to_string(),
                    style: border_style.clone(),
                },
            ]);
            lines.push(vec![
                Span {
                    text: format!("{indent}│  ╰{}╯", "─".repeat(left_w + 2)),
                    style: border_style.clone(),
                },
                Span {
                    text: " ".repeat(display_width(&arrow)),
                    style: muted_style.clone(),
                },
                Span {
                    text: format!("╰{}╯", "─".repeat(right_w + 2)),
                    style: border_style.clone(),
                },
            ]);
        }
    }

    lines.push(vec![Span {
        text: format!("{indent}└──"),
        style: muted_style,
    }]);

    Some(lines)
}

fn split_arrow(line: &str) -> Option<(&str, Option<String>, &str)> {
    // Support `A -->|label| B`
    for pat in ["-->|", "--->", "==>", "-.->", "-->", "->", "---"] {
        if let Some(pos) = line.find(pat) {
            let left = line[..pos].trim();
            let rest = &line[pos + pat.len()..];
            if pat == "-->|" {
                if let Some(end_pipe) = rest.find('|') {
                    let lbl = rest[..end_pipe].trim().to_string();
                    let right = rest[end_pipe + 1..].trim();
                    return Some((left, (!lbl.is_empty()).then_some(lbl), right));
                }
            }
            return Some((left, None, rest.trim()));
        }
    }
    None
}

fn parse_node_spec(spec: &str) -> (String, Option<String>) {
    let spec = spec.trim();
    for (open, close) in [("[", "]"), ("(", ")"), ("{", "}")] {
        if let Some(start) = spec.find(open) {
            if let Some(end) = spec.rfind(close) {
                if end > start {
                    let id = spec[..start].trim().to_string();
                    let raw_lbl = spec[start + 1..end]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    return (
                        if id.is_empty() { raw_lbl.clone() } else { id },
                        Some(raw_lbl),
                    );
                }
            }
        }
    }
    (spec.trim_matches('"').to_string(), None)
}

fn render_chart(
    content: &str,
    indent: &str,
    max_width: usize,
    theme: &Theme,
) -> Option<Vec<RenderedLine>> {
    let mut title: Option<String> = None;
    let mut x_labels: Vec<String> = Vec::new();
    let mut points: Vec<(String, f64)> = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("xychart") || lower == "pie" || lower == "bar" {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title ") {
            title = Some(rest.trim().trim_matches('"').to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("x-axis ") {
            if let Some(start) = rest.find('[') {
                if let Some(end) = rest.rfind(']') {
                    x_labels = rest[start + 1..end]
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .collect();
                }
            }
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("bar ")
            .or_else(|| line.strip_prefix("line "))
        {
            if let Some(start) = rest.find('[') {
                if let Some(end) = rest.rfind(']') {
                    let vals: Vec<f64> = rest[start + 1..end]
                        .split(',')
                        .filter_map(|s| s.trim().parse::<f64>().ok())
                        .collect();
                    for (i, v) in vals.into_iter().enumerate() {
                        let lbl = x_labels
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| format!("#{}", i + 1));
                        points.push((lbl, v));
                    }
                }
            }
            continue;
        }
        // Key: Value or Label, Value syntax
        if let Some((k, v)) = line.split_once(':').or_else(|| line.split_once(',')) {
            let key = k.trim().trim_matches('"').to_string();
            if let Ok(num) = v.trim().parse::<f64>() {
                points.push((key, num));
            }
        }
    }

    if points.is_empty() {
        return None;
    }

    let max_val = points
        .iter()
        .map(|(_, v)| v.abs())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let label_w = points
        .iter()
        .map(|(k, _)| display_width(k))
        .max()
        .unwrap_or(6)
        .clamp(4, 20);
    let bar_max_w = max_width.saturating_sub(label_w + 18).clamp(10, 40);

    let mut lines = Vec::new();
    let header_title = title.as_deref().unwrap_or("Chart");
    lines.push(vec![
        Span {
            text: format!("{indent}┌─ "),
            style: theme.style(SemanticColor::Muted),
        },
        Span {
            text: format!("📊 {header_title}"),
            style: theme.style(SemanticColor::Heading).bold(),
        },
    ]);

    for (label, value) in &points {
        let ratio = (value.abs() / max_val).clamp(0.0, 1.0);
        let filled = (ratio * bar_max_w as f64).round() as usize;
        let empty = bar_max_w.saturating_sub(filled);
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        let val_str = if (value - value.round()).abs() < 1e-6 {
            format!(" {:.0}", value)
        } else {
            format!(" {:.2}", value)
        };
        lines.push(vec![
            Span {
                text: format!("{indent}│  "),
                style: theme.style(SemanticColor::Muted),
            },
            Span {
                text: pad_display_width(label, label_w),
                style: theme.style(SemanticColor::Foreground),
            },
            Span {
                text: " │ ".to_string(),
                style: theme.style(SemanticColor::Muted),
            },
            Span {
                text: bar,
                style: theme.style(SemanticColor::Accent),
            },
            Span {
                text: val_str,
                style: theme.style(SemanticColor::Number).bold(),
            },
        ]);
    }

    lines.push(vec![Span {
        text: format!("{indent}└──"),
        style: theme.style(SemanticColor::Muted),
    }]);

    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_mermaid_flowchart_into_boxes() {
        let src = "graph LR\n  A[Parser] --> B[AST]\n  B --> C[Terminal]";
        let lines =
            render_diagram_block("mermaid", src, "", 80, &Theme::Dark).expect("diagram rendered");
        let text: String = lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Parser"), "{text}");
        assert!(text.contains("AST"), "{text}");
        assert!(text.contains("Terminal"), "{text}");
        assert!(text.contains("───▶"), "{text}");
    }

    #[test]
    fn renders_chart_bars() {
        let src = "title Benchmarks\nRust: 95\nPython: 30";
        let lines =
            render_diagram_block("chart", src, "", 80, &Theme::Dark).expect("chart rendered");
        let text: String = lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Benchmarks"));
        assert!(text.contains("Rust"));
        assert!(text.contains('█'));
    }
}
