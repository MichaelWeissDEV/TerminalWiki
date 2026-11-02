//! Bounded, native Unicode math layout for terminal cells.
//!
//! This deliberately implements a readable TeX subset instead of invoking a
//! TeX engine. Unsupported commands remain visible and are reported as warnings.

use terminalwiki_core::unicode::display_width;

const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathLayout {
    pub lines: Vec<String>,
    pub width_cells: usize,
    pub height_cells: usize,
    pub baseline: usize,
    pub warnings: Vec<String>,
}

impl MathLayout {
    fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            width_cells: display_width(&text),
            height_cells: 1,
            baseline: 0,
            lines: vec![text],
            warnings: Vec::new(),
        }
    }
}

/// Compatibility trait used by callers that only need plain strings.
pub trait MathRenderer {
    fn render_inline(&self, formula: &str) -> String;
    fn render_display(&self, formula: &str) -> Vec<String>;
}

pub struct TerminalTextMathRenderer;

impl MathRenderer for TerminalTextMathRenderer {
    fn render_inline(&self, formula: &str) -> String {
        let layout = layout_math(formula);
        layout.lines.join(" ")
    }

    fn render_display(&self, formula: &str) -> Vec<String> {
        layout_math(formula).lines
    }
}

/// Parses and lays out a bounded subset of mathematical TeX in Unicode cells.
pub fn layout_math(formula: &str) -> MathLayout {
    if formula.len() > MAX_INPUT_BYTES {
        return MathLayout {
            lines: vec!["[math input exceeds 64 KiB limit]".into()],
            width_cells: 32,
            height_cells: 1,
            baseline: 0,
            warnings: vec!["math input limit exceeded".into()],
        };
    }
    if let Some(matrix) = layout_matrix(formula) {
        return matrix;
    }
    let mut parser = Parser::new(formula);
    let mut result = parser.sequence(None, 0);
    result.warnings.append(&mut parser.warnings);
    result
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    warnings: Vec<String>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
            warnings: Vec::new(),
        }
    }

    fn sequence(&mut self, end: Option<u8>, depth: usize) -> MathLayout {
        if depth >= MAX_DEPTH {
            self.warnings.push("math nesting limit exceeded".into());
            return MathLayout::text("…");
        }
        let mut out = MathLayout::text("");
        while self.pos < self.src.len() {
            if Some(self.src[self.pos]) == end {
                self.pos += 1;
                break;
            }
            let mut item = match self.src[self.pos] {
                b'{' => {
                    self.pos += 1;
                    self.sequence(Some(b'}'), depth + 1)
                }
                b'}' => {
                    self.pos += 1;
                    self.warnings.push("unmatched closing math group".into());
                    MathLayout::text("}")
                }
                b'\\' => self.command(depth + 1),
                b'^' | b'_' => {
                    let superscript = self.src[self.pos] == b'^';
                    self.pos += 1;
                    let script = self.argument(depth + 1);
                    let rendered = script.lines.join("");
                    let glyphs = if superscript {
                        to_super(&rendered)
                    } else {
                        to_sub(&rendered)
                    };
                    MathLayout::text(glyphs)
                }
                byte => {
                    self.pos += 1;
                    MathLayout::text((byte as char).to_string())
                }
            };
            out = hjoin(out, &mut item);
        }
        if end.is_some() && self.pos >= self.src.len() && !self.src.ends_with(b"}") {
            self.warnings.push("unclosed math group".into());
        }
        out
    }

    fn argument(&mut self, depth: usize) -> MathLayout {
        if self.pos >= self.src.len() {
            self.warnings.push("missing math argument".into());
            return MathLayout::text("?");
        }
        if self.src[self.pos] == b'{' {
            self.pos += 1;
            self.sequence(Some(b'}'), depth)
        } else {
            let byte = self.src[self.pos];
            self.pos += 1;
            MathLayout::text((byte as char).to_string())
        }
    }

    fn command(&mut self, depth: usize) -> MathLayout {
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_alphabetic() {
            self.pos += 1;
        }
        if start == self.pos {
            return MathLayout::text("\\");
        }
        let command = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("");
        match command {
            "frac" | "dfrac" | "tfrac" => fraction(self.argument(depth), self.argument(depth)),
            "sqrt" => root(self.argument(depth)),
            "sum" => MathLayout::text("∑"),
            "prod" => MathLayout::text("∏"),
            "int" => MathLayout::text("∫"),
            "infty" => MathLayout::text("∞"),
            "le" | "leq" => MathLayout::text("≤"),
            "ge" | "geq" => MathLayout::text("≥"),
            "neq" => MathLayout::text("≠"),
            "to" | "rightarrow" => MathLayout::text("→"),
            "left" | "right" | "big" | "Big" | "quad" | "qquad" => MathLayout::text(""),
            "text" | "mathrm" | "mathbf" | "mathbb" => self.argument(depth),
            "alpha" => MathLayout::text("α"),
            "beta" => MathLayout::text("β"),
            "gamma" => MathLayout::text("γ"),
            "delta" => MathLayout::text("δ"),
            "epsilon" => MathLayout::text("ε"),
            "theta" => MathLayout::text("θ"),
            "lambda" => MathLayout::text("λ"),
            "mu" => MathLayout::text("μ"),
            "pi" => MathLayout::text("π"),
            "sigma" => MathLayout::text("σ"),
            "phi" => MathLayout::text("φ"),
            "omega" => MathLayout::text("ω"),
            "times" => MathLayout::text("×"),
            "cdot" => MathLayout::text("·"),
            "pm" => MathLayout::text("±"),
            "ldots" | "dots" => MathLayout::text("…"),
            " " | "," | "!" => MathLayout::text(" "),
            other => {
                self.warnings
                    .push(format!("unsupported math command \\{other}"));
                MathLayout::text(format!("\\{other}"))
            }
        }
    }
}

fn hjoin(mut left: MathLayout, right: &mut MathLayout) -> MathLayout {
    if left.width_cells == 0 {
        return right.clone();
    }
    if right.width_cells == 0 {
        return left;
    }
    let baseline = left.baseline.max(right.baseline);
    let height = (baseline + 1)
        .max(left.height_cells + baseline - left.baseline)
        .max(right.height_cells + baseline - right.baseline);
    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let l = row
            .checked_sub(baseline - left.baseline)
            .and_then(|i| left.lines.get(i));
        let r = row
            .checked_sub(baseline - right.baseline)
            .and_then(|i| right.lines.get(i));
        lines.push(format!(
            "{}{}",
            l.map_or_else(|| " ".repeat(left.width_cells), Clone::clone),
            r.map_or_else(|| " ".repeat(right.width_cells), Clone::clone)
        ));
    }
    left.lines = lines;
    left.width_cells += right.width_cells;
    left.height_cells = height;
    left.baseline = baseline;
    left.warnings.append(&mut right.warnings);
    left
}

fn fraction(mut numerator: MathLayout, mut denominator: MathLayout) -> MathLayout {
    let width = numerator.width_cells.max(denominator.width_cells).max(1);
    let mut lines = center_lines(&numerator.lines, width);
    lines.push("─".repeat(width));
    lines.extend(center_lines(&denominator.lines, width));
    let mut warnings = Vec::new();
    warnings.append(&mut numerator.warnings);
    warnings.append(&mut denominator.warnings);
    MathLayout {
        height_cells: lines.len(),
        width_cells: width,
        baseline: numerator.height_cells,
        lines,
        warnings,
    }
}

fn root(mut inner: MathLayout) -> MathLayout {
    let mut lines = Vec::new();
    for (i, line) in inner.lines.iter().enumerate() {
        lines.push(if i == 0 {
            format!("√{}", "‾".repeat(inner.width_cells))
        } else {
            format!(" {}", line)
        });
    }
    MathLayout {
        width_cells: inner.width_cells + 1,
        height_cells: lines.len(),
        baseline: inner.baseline,
        lines,
        warnings: std::mem::take(&mut inner.warnings),
    }
}

fn center_lines(lines: &[String], width: usize) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            let gap = width.saturating_sub(display_width(line));
            format!(
                "{}{}{}",
                " ".repeat(gap / 2),
                line,
                " ".repeat(gap - gap / 2)
            )
        })
        .collect()
}

fn layout_matrix(formula: &str) -> Option<MathLayout> {
    let (open, close) = if formula.contains("\\begin{bmatrix}") {
        ("[", "]")
    } else if formula.contains("\\begin{pmatrix}") {
        ("(", ")")
    } else if formula.contains("\\begin{cases}") {
        ("{", "}")
    } else {
        return None;
    };
    let body = formula.split('}').nth(1)?.split("\\end").next()?.trim();
    let rows: Vec<Vec<String>> = body
        .split("\\\\")
        .map(|row| {
            row.split('&')
                .map(|cell| layout_math(cell.trim()).lines.join(" "))
                .collect()
        })
        .collect();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|r| r.get(column))
                .map(|c| display_width(c))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = widths
                .iter()
                .enumerate()
                .map(|(i, width)| {
                    format!(
                        "{:width$}",
                        row.get(i).map(String::as_str).unwrap_or(""),
                        width = *width
                    )
                })
                .collect();
            format!("{open} {} {close}", cells.join("  "))
        })
        .collect();
    let width_cells = lines
        .iter()
        .map(|line| display_width(line))
        .max()
        .unwrap_or(0);
    Some(MathLayout {
        height_cells: lines.len(),
        width_cells,
        baseline: lines.len().saturating_sub(1) / 2,
        lines,
        warnings: Vec::new(),
    })
}

fn to_super(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '+' => '⁺',
            '-' => '⁻',
            '=' => '⁼',
            '(' => '⁽',
            ')' => '⁾',
            'n' => 'ⁿ',
            'i' => 'ⁱ',
            other => other,
        })
        .collect()
}
fn to_sub(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            '+' => '₊',
            '-' => '₋',
            '=' => '₌',
            '(' => '₍',
            ')' => '₎',
            'a' => 'ₐ',
            'e' => 'ₑ',
            'i' => 'ᵢ',
            'j' => 'ⱼ',
            'n' => 'ₙ',
            'o' => 'ₒ',
            'r' => 'ᵣ',
            's' => 'ₛ',
            't' => 'ₜ',
            'x' => 'ₓ',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::layout_math;
    #[test]
    fn fractions_and_roots_are_two_dimensional() {
        assert_eq!(layout_math("\\frac{a+b}{c}").lines, ["a+b", "───", " c "]);
        assert!(layout_math("\\sqrt{x^2+1}").lines[0].starts_with("√"));
    }
    #[test]
    fn greek_and_matrices_are_visible() {
        assert_eq!(layout_math("\\alpha \\le \\beta").lines, ["α ≤ β"]);
        assert_eq!(
            layout_math("\\begin{bmatrix}1&2\\\\3&4\\end{bmatrix}").lines,
            ["[ 1  2 ]", "[ 3  4 ]"]
        );
    }
    #[test]
    fn unknown_commands_warn_without_panicking() {
        let layout = layout_math("\\mystery{x}");
        assert!(layout.lines.join("").contains("\\mystery"));
        assert!(!layout.warnings.is_empty());
    }
}
