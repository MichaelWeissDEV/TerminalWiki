//! LaTeX integration for TerminalWiki using the Markdown (`pulldown-cmark`) pipeline
//! and external LaTeX engines (`pdflatex`, `xelatex`, `lualatex`, `tectonic`, `latexmk`, `pandoc`).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ansi::ColorMode;
use crate::document::RenderedDocument;
use crate::markdown;
use crate::render::Renderer;
use crate::theme::Theme;

/// External LaTeX engines supported for compiling or converting `.tex` documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatexEngine {
    Tectonic,
    Latexmk,
    Xelatex,
    Lualatex,
    Pdflatex,
    Pandoc,
}

impl LatexEngine {
    pub const ALL: &'static [LatexEngine] = &[
        LatexEngine::Tectonic,
        LatexEngine::Latexmk,
        LatexEngine::Xelatex,
        LatexEngine::Lualatex,
        LatexEngine::Pdflatex,
        LatexEngine::Pandoc,
    ];

    pub fn binary_name(self) -> &'static str {
        match self {
            LatexEngine::Tectonic => "tectonic",
            LatexEngine::Latexmk => "latexmk",
            LatexEngine::Xelatex => "xelatex",
            LatexEngine::Lualatex => "lualatex",
            LatexEngine::Pdflatex => "pdflatex",
            LatexEngine::Pandoc => "pandoc",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "tectonic" => Some(LatexEngine::Tectonic),
            "latexmk" => Some(LatexEngine::Latexmk),
            "xelatex" => Some(LatexEngine::Xelatex),
            "lualatex" => Some(LatexEngine::Lualatex),
            "pdflatex" | "tex" | "latex" => Some(LatexEngine::Pdflatex),
            "pandoc" => Some(LatexEngine::Pandoc),
            _ => None,
        }
    }

    pub fn is_available(self) -> bool {
        Command::new(self.binary_name())
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// Result of compiling a `.tex` document with an external LaTeX engine.
#[derive(Debug, Clone)]
pub struct LatexCompileReport {
    pub engine: LatexEngine,
    pub output_path: PathBuf,
    pub summary: String,
}

/// Detects all installed external LaTeX engines in priority order.
pub fn detect_latex_engines() -> Vec<LatexEngine> {
    LatexEngine::ALL
        .iter()
        .copied()
        .filter(|e| e.is_available())
        .collect()
}

/// Compiles a `.tex` file using an external LaTeX engine (`tectonic`, `latexmk`,
/// `xelatex`, `lualatex`, `pdflatex`, or `pandoc`).
pub fn compile_latex_file(
    path: &Path,
    preferred_engine: Option<&str>,
) -> Result<LatexCompileReport, String> {
    if !path.exists() {
        return Err(format!("LaTeX file does not exist: {}", path.display()));
    }

    let engine = if let Some(pref) = preferred_engine.filter(|s| !s.trim().is_empty()) {
        let parsed =
            LatexEngine::parse(pref).ok_or_else(|| format!("Unknown LaTeX engine '{pref}'"))?;
        if !parsed.is_available() {
            return Err(format!(
                "LaTeX engine '{}' is not installed on PATH",
                parsed.binary_name()
            ));
        }
        parsed
    } else {
        detect_latex_engines()
            .into_iter()
            .next()
            .ok_or_else(|| {
                "No LaTeX engine found (install tectonic, latexmk, pdflatex, xelatex, lualatex, or pandoc)"
                    .to_string()
            })?
    };

    let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| "Invalid LaTeX file path".to_string())?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());

    let mut cmd = Command::new(engine.binary_name());
    cmd.current_dir(parent_dir);

    let output_path = match engine {
        LatexEngine::Tectonic => {
            cmd.arg(file_name);
            parent_dir.join(format!("{stem}.pdf"))
        }
        LatexEngine::Latexmk => {
            cmd.args(["-pdf", "-interaction=nonstopmode", "-halt-on-error"])
                .arg(file_name);
            parent_dir.join(format!("{stem}.pdf"))
        }
        LatexEngine::Xelatex | LatexEngine::Lualatex | LatexEngine::Pdflatex => {
            cmd.args(["-interaction=nonstopmode", "-halt-on-error"])
                .arg(file_name);
            parent_dir.join(format!("{stem}.pdf"))
        }
        LatexEngine::Pandoc => {
            let md_out = parent_dir.join(format!("{stem}.rendered.md"));
            cmd.args(["-f", "latex", "-t", "gfm"])
                .arg(file_name)
                .arg("-o")
                .arg(&md_out);
            md_out
        }
    };

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to launch {}: {e}", engine.binary_name()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = stderr
            .lines()
            .chain(stdout.lines())
            .find(|l| l.starts_with('!') || l.to_ascii_lowercase().contains("error"))
            .unwrap_or("compilation failed");
        return Err(format!(
            "{} failed: {}",
            engine.binary_name(),
            detail.trim()
        ));
    }

    Ok(LatexCompileReport {
        engine,
        summary: format!(
            "Compiled {} with {}",
            output_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| output_path.display().to_string()),
            engine.binary_name()
        ),
        output_path,
    })
}

/// Renders a LaTeX source document into a `RenderedDocument` by converting it
/// into Markdown and passing it through `pulldown-cmark` and `Renderer`.
pub fn render_latex(
    path: Option<&Path>,
    source: &str,
    config: &terminalwiki_core::config::Config,
    theme: &Theme,
    color_mode: ColorMode,
) -> RenderedDocument {
    let md = latex_to_markdown(source);
    let doc = markdown::parse_markdown(&md);
    let base_dir = path.and_then(|p| p.parent());
    Renderer::new(config.render.max_content_width, theme, color_mode)
        .with_base_dir(base_dir)
        .render_document(&doc)
}

/// Renders a BibTeX (`.bib`) bibliography file as a structured Markdown document.
pub fn render_bibtex(
    path: Option<&Path>,
    source: &str,
    config: &terminalwiki_core::config::Config,
    theme: &Theme,
    color_mode: ColorMode,
) -> RenderedDocument {
    let md = bibtex_to_markdown(path, source);
    let doc = markdown::parse_markdown(&md);
    let base_dir = path.and_then(|p| p.parent());
    Renderer::new(config.render.max_content_width, theme, color_mode)
        .with_base_dir(base_dir)
        .render_document(&doc)
}

/// Converts a `.bib` file into structured Markdown with headings and clickable links.
pub fn bibtex_to_markdown(path: Option<&Path>, source: &str) -> String {
    let file_title = path
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Bibliography.bib".to_string());
    let mut out = format!("# 📚 Bibliography: {file_title}\n\n");

    let mut current_type = String::new();
    let mut current_key = String::new();
    let mut fields: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut in_entry = false;

    let flush_entry = |out: &mut String,
                       entry_type: &str,
                       cite_key: &str,
                       fields: &std::collections::BTreeMap<String, String>| {
        if cite_key.is_empty() {
            return;
        }
        out.push_str(&format!("## @{entry_type} `{cite_key}`\n\n"));
        if let Some(title) = fields.get("title") {
            out.push_str(&format!("- **Title:** {}\n", convert_inline_latex(title)));
        }
        if let Some(author) = fields.get("author") {
            out.push_str(&format!("- **Author:** {}\n", convert_inline_latex(author)));
        }
        if let Some(year) = fields.get("year") {
            out.push_str(&format!("- **Year:** {year}\n"));
        }
        if let Some(venue) = fields.get("journal").or_else(|| fields.get("booktitle")) {
            out.push_str(&format!("- **Venue:** {}\n", convert_inline_latex(venue)));
        }
        if let Some(url) = fields.get("url") {
            out.push_str(&format!("- **URL:** [{url}]({url})\n"));
        } else if let Some(doi) = fields.get("doi") {
            let doi_url = if doi.starts_with("http") {
                doi.clone()
            } else {
                format!("https://doi.org/{doi}")
            };
            out.push_str(&format!("- **DOI:** [{doi}]({doi_url})\n"));
        }
        if let Some(file) = fields.get("file") {
            let clean_file = file
                .split(':')
                .find(|p| p.ends_with(".pdf"))
                .unwrap_or(file);
            out.push_str(&format!("- **File:** [[{clean_file}]]\n"));
        }
        out.push('\n');
    };

    for line in source.lines() {
        let trimmed = line.split('%').next().unwrap_or(line).trim();
        if let Some(after_at) = trimmed.strip_prefix('@') {
            if in_entry {
                flush_entry(&mut out, &current_type, &current_key, &fields);
                fields.clear();
            }
            if let Some(brace_pos) = after_at.find('{') {
                current_type = after_at[..brace_pos].trim().to_ascii_lowercase();
                let after_brace = &after_at[brace_pos + 1..];
                current_key = after_brace
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                in_entry = !current_key.is_empty();
            }
            continue;
        }
        if in_entry {
            if trimmed == "}" {
                flush_entry(&mut out, &current_type, &current_key, &fields);
                fields.clear();
                in_entry = false;
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                let key = k.trim().to_ascii_lowercase();
                let val = v
                    .trim()
                    .trim_end_matches(',')
                    .trim()
                    .trim_matches(|c| c == '{' || c == '}' || c == '"')
                    .trim()
                    .to_string();
                if !key.is_empty() && !val.is_empty() {
                    fields.insert(key, val);
                }
            }
        }
    }
    if in_entry {
        flush_entry(&mut out, &current_type, &current_key, &fields);
    }

    out
}

/// Metadata extracted from a LaTeX document for wiki indexing (`terminalwiki-index`).
#[derive(Debug, Default, Clone)]
pub struct LatexIndexMetadata {
    pub title: Option<String>,
    pub headings: Vec<String>,
    pub tags: Vec<String>,
    pub wiki_links: Vec<String>,
    pub body_text: String,
}

/// Extracts title, headings, tags, wiki links, and body text from a `.tex` document.
pub fn extract_latex_metadata(source: &str) -> LatexIndexMetadata {
    let mut meta = LatexIndexMetadata::default();

    // Extract tags from `% #tag` comments before stripping comments
    for line in source.lines() {
        if let Some((_, comment)) = line.split_once('%') {
            for word in comment.split_whitespace() {
                if let Some(tag) = word.strip_prefix('#') {
                    let clean: String = tag
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                        .collect();
                    if !clean.is_empty()
                        && !meta
                            .tags
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(&clean))
                    {
                        meta.tags.push(clean);
                    }
                }
            }
        }
    }

    let md = latex_to_markdown(source);
    for line in md.lines() {
        let trimmed = line.trim();
        if let Some(h1) = trimmed.strip_prefix("# ") {
            let clean = h1.trim().to_string();
            if meta.title.is_none() && !clean.is_empty() {
                meta.title = Some(clean.clone());
            }
            if !clean.is_empty() {
                meta.headings.push(clean);
            }
        } else if let Some(rest) = trimmed
            .strip_prefix("## ")
            .or_else(|| trimmed.strip_prefix("### "))
            .or_else(|| trimmed.strip_prefix("#### "))
        {
            let clean = rest.trim().to_string();
            if !clean.is_empty() {
                meta.headings.push(clean);
            }
        }
    }

    // Extract [[WikiLinks]] produced from \input, \include, \ref, \cite, or raw [[...]]
    let mut rest = md.as_str();
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            break;
        };
        let inner = &after[..end];
        let target = inner.split('|').next().unwrap_or(inner).trim();
        let target = target.split('#').next().unwrap_or(target).trim();
        if !target.is_empty() && !meta.wiki_links.iter().any(|l| l == target) {
            meta.wiki_links.push(target.to_string());
        }
        rest = &after[end + 2..];
    }

    meta.body_text = md;
    meta
}

/// Converts a LaTeX document into GitHub-Flavored Markdown with `$...$` and `$$...$$` math
/// so `pulldown-cmark` and TerminalWiki's renderer can display it natively.
pub fn latex_to_markdown(source: &str) -> String {
    let cleaned = strip_latex_comments(source);
    let title = extract_command_arg(&cleaned, "\\title");
    let author = extract_command_arg(&cleaned, "\\author");
    let date = extract_command_arg(&cleaned, "\\date");

    // If \begin{document} exists, focus on the document body
    let body = if let Some(start) = cleaned.find("\\begin{document}") {
        let after = &cleaned[start + "\\begin{document}".len()..];
        if let Some(end) = after.find("\\end{document}") {
            &after[..end]
        } else {
            after
        }
    } else {
        cleaned.as_str()
    };

    let mut out = String::new();
    if let Some(t) = &title {
        let converted_title = convert_inline_latex(t.trim());
        if !converted_title.is_empty() {
            out.push_str("# ");
            out.push_str(&converted_title);
            out.push_str("\n\n");
        }
    }
    if author.is_some() || date.is_some() {
        let mut meta_parts = Vec::new();
        if let Some(a) = &author {
            let ca = convert_inline_latex(a.trim());
            if !ca.is_empty() {
                meta_parts.push(format!("**Author:** {ca}"));
            }
        }
        if let Some(d) = &date {
            let cd = convert_inline_latex(d.trim());
            if !cd.is_empty() && cd != "\\today" {
                meta_parts.push(format!("**Date:** {cd}"));
            }
        }
        if !meta_parts.is_empty() {
            out.push_str(&meta_parts.join(" · "));
            out.push_str("\n\n");
        }
    }

    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    let mut list_stack: Vec<bool> = Vec::new(); // false = itemize, true = enumerate
    let mut enum_counters: Vec<usize> = Vec::new();

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Skip preamble / layout-only commands
        if is_ignored_line(trimmed) {
            i += 1;
            continue;
        }

        // Handle display math \[ ... \]
        if let Some(after_open) = trimmed.strip_prefix("\\[") {
            if let Some(end_idx) = after_open.find("\\]") {
                let math_inner = after_open[..end_idx].trim();
                out.push_str("\n$$\n");
                out.push_str(math_inner);
                out.push_str("\n$$\n\n");
                i += 1;
                continue;
            }
            let mut math_lines = Vec::new();
            if !after_open.trim().is_empty() {
                math_lines.push(after_open.trim());
            }
            i += 1;
            while i < lines.len() {
                let l = lines[i].trim();
                if let Some(before_close) = l.strip_suffix("\\]") {
                    if !before_close.trim().is_empty() {
                        math_lines.push(before_close.trim());
                    }
                    i += 1;
                    break;
                }
                math_lines.push(l);
                i += 1;
            }
            out.push_str("\n$$\n");
            out.push_str(&math_lines.join("\n"));
            out.push_str("\n$$\n\n");
            continue;
        }

        // Handle math environments: equation, align, gather, multline, displaymath
        if let Some(env_name) = match_begin_env(trimmed) {
            let base_env = env_name.trim_end_matches('*');
            if matches!(
                base_env,
                "equation" | "align" | "gather" | "multline" | "displaymath" | "eqnarray"
            ) {
                let end_marker = format!("\\end{{{env_name}}}");
                let mut math_body = Vec::new();
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    if l.starts_with(&end_marker) {
                        i += 1;
                        break;
                    }
                    // Strip \label{...} inside math environments
                    let cleaned_math = strip_command_with_arg(l, "\\label");
                    if !cleaned_math.trim().is_empty() {
                        math_body.push(cleaned_math);
                    }
                    i += 1;
                }
                out.push_str("\n$$\n");
                if base_env == "align" || base_env == "eqnarray" {
                    out.push_str("\\begin{aligned}\n");
                    out.push_str(&math_body.join("\n"));
                    out.push_str("\n\\end{aligned}");
                } else {
                    out.push_str(&math_body.join("\n"));
                }
                out.push_str("\n$$\n\n");
                continue;
            }

            // Code environments: verbatim, lstlisting, minted
            if matches!(base_env, "verbatim" | "lstlisting" | "minted") {
                let lang = if base_env == "minted" {
                    let after_env = &trimmed["\\begin{minted}".len()..];
                    extract_braced_group(after_env.trim_start())
                        .map(|(g, _)| g.to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let end_marker = format!("\\end{{{env_name}}}");
                let mut code_lines = Vec::new();
                i += 1;
                while i < lines.len() {
                    if lines[i].trim().starts_with(&end_marker) {
                        i += 1;
                        break;
                    }
                    code_lines.push(lines[i]);
                    i += 1;
                }
                out.push_str("\n```");
                out.push_str(&lang);
                out.push('\n');
                out.push_str(&code_lines.join("\n"));
                out.push_str("\n```\n\n");
                continue;
            }

            // Tabular environment -> GFM Markdown table
            if base_env == "tabular" || base_env == "array" {
                let end_marker = format!("\\end{{{env_name}}}");
                let mut table_raw = String::new();
                // Check if there is content after \begin{tabular}{cols} on the same line
                if let Some(after_begin) = trimmed.strip_prefix(&format!("\\begin{{{env_name}}}")) {
                    let rest = skip_optional_and_braced(after_begin);
                    if !rest.trim().is_empty() {
                        table_raw.push_str(rest.trim());
                        table_raw.push('\n');
                    }
                }
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    if let Some(before_end) = l.strip_suffix(&end_marker) {
                        if !before_end.trim().is_empty() {
                            table_raw.push_str(before_end.trim());
                        }
                        i += 1;
                        break;
                    }
                    if l.starts_with(&end_marker) {
                        i += 1;
                        break;
                    }
                    table_raw.push_str(l);
                    table_raw.push('\n');
                    i += 1;
                }
                let md_table = convert_tabular_to_markdown(&table_raw);
                if !md_table.is_empty() {
                    out.push('\n');
                    out.push_str(&md_table);
                    out.push('\n');
                }
                continue;
            }

            // Lists: itemize, enumerate, description
            if base_env == "itemize" || base_env == "description" {
                list_stack.push(false);
                i += 1;
                continue;
            }
            if base_env == "enumerate" {
                list_stack.push(true);
                enum_counters.push(0);
                i += 1;
                continue;
            }

            // Callout / theorem / abstract / quote environments
            if let Some((callout_kind, label)) = callout_for_env(base_env) {
                let end_marker = format!("\\end{{{env_name}}}");
                // Optional title in \begin{theorem}[Pythagoras]
                let opt_title = trimmed
                    .strip_prefix(&format!("\\begin{{{env_name}}}"))
                    .and_then(|rest| extract_bracket_group(rest.trim_start()))
                    .map(|(opt, _)| convert_inline_latex(opt));
                out.push_str(&format!(
                    "\n> [!{}]{}\n",
                    callout_kind,
                    match opt_title {
                        Some(t) if !t.is_empty() => format!(" {label} ({t})"),
                        _ if !label.is_empty() => format!(" {label}"),
                        _ => String::new(),
                    }
                ));
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    if l.starts_with(&end_marker) {
                        i += 1;
                        break;
                    }
                    if !is_ignored_line(l) {
                        let converted = convert_inline_latex(l);
                        if converted.is_empty() {
                            out.push_str(">\n");
                        } else {
                            out.push_str("> ");
                            out.push_str(&converted);
                            out.push('\n');
                        }
                    }
                    i += 1;
                }
                out.push('\n');
                continue;
            }

            if matches!(base_env, "quote" | "quotation") {
                let end_marker = format!("\\end{{{env_name}}}");
                out.push('\n');
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    if l.starts_with(&end_marker) {
                        i += 1;
                        break;
                    }
                    let converted = convert_inline_latex(l);
                    out.push_str("> ");
                    out.push_str(&converted);
                    out.push('\n');
                    i += 1;
                }
                out.push('\n');
                continue;
            }

            // Container environments like figure, table, center, document: just step inside
            if matches!(base_env, "figure" | "table" | "center" | "minipage") {
                i += 1;
                continue;
            }
        }

        if let Some(env_name) = match_end_env(trimmed) {
            let base_env = env_name.trim_end_matches('*');
            if base_env == "itemize" || base_env == "description" {
                list_stack.pop();
                if list_stack.is_empty() {
                    out.push('\n');
                }
                i += 1;
                continue;
            }
            if base_env == "enumerate" {
                list_stack.pop();
                enum_counters.pop();
                if list_stack.is_empty() {
                    out.push('\n');
                }
                i += 1;
                continue;
            }
            if matches!(base_env, "figure" | "table" | "center" | "minipage") {
                i += 1;
                continue;
            }
        }

        // Handle sectioning commands
        if let Some((level, heading_text)) = parse_section_heading(trimmed) {
            let hashes = "#".repeat(level);
            out.push('\n');
            out.push_str(&hashes);
            out.push(' ');
            out.push_str(&convert_inline_latex(&heading_text));
            out.push_str("\n\n");
            i += 1;
            continue;
        }

        // Handle \item inside lists
        if let Some(after_item) = trimmed.strip_prefix("\\item") {
            if after_item.is_empty()
                || after_item.starts_with(char::is_whitespace)
                || after_item.starts_with('[')
            {
                let depth = list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let is_ordered = list_stack.last().copied().unwrap_or(false);
                let rest = after_item.trim_start();
                let (opt_label, item_body) = if let Some((lbl, rem)) = extract_bracket_group(rest) {
                    (Some(convert_inline_latex(lbl)), rem.trim_start())
                } else {
                    (None, rest)
                };

                out.push_str(&indent);
                if let Some(lbl) = opt_label {
                    out.push_str(&format!("- **{lbl}**: {}", convert_inline_latex(item_body)));
                } else if is_ordered {
                    let count = enum_counters
                        .last_mut()
                        .map(|c| {
                            *c += 1;
                            *c
                        })
                        .unwrap_or(1);
                    out.push_str(&format!("{count}. {}", convert_inline_latex(item_body)));
                } else {
                    out.push_str(&format!("- {}", convert_inline_latex(item_body)));
                }
                out.push('\n');
                i += 1;
                continue;
            }
        }

        // Handle standalone \includegraphics or \caption inside figure/table
        if let Some(caption) = extract_command_arg(trimmed, "\\caption") {
            out.push_str(&format!("*{}*\n\n", convert_inline_latex(&caption)));
            i += 1;
            continue;
        }

        if trimmed.is_empty() {
            out.push('\n');
            i += 1;
            continue;
        }

        let converted = convert_inline_latex(trimmed);
        if !converted.trim().is_empty() {
            if !list_stack.is_empty() {
                let indent = "  ".repeat(list_stack.len());
                out.push_str(&indent);
            }
            out.push_str(&converted);
            out.push('\n');
        }
        i += 1;
    }

    out
}

fn strip_latex_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let chars: Vec<char> = line.chars().collect();
        let mut idx = 0;
        while idx < chars.len() {
            if chars[idx] == '%' && (idx == 0 || chars[idx - 1] != '\\') {
                break;
            }
            out.push(chars[idx]);
            idx += 1;
        }
        out.push('\n');
    }
    out
}

fn is_ignored_line(trimmed: &str) -> bool {
    const IGNORED_PREFIXES: &[&str] = &[
        "\\documentclass",
        "\\usepackage",
        "\\usetikzlibrary",
        "\\newcommand",
        "\\renewcommand",
        "\\providecommand",
        "\\DeclareMathOperator",
        "\\setlength",
        "\\pagestyle",
        "\\thispagestyle",
        "\\bibliographystyle",
        "\\maketitle",
        "\\tableofcontents",
        "\\centering",
        "\\noindent",
        "\\clearpage",
        "\\newpage",
        "\\pagebreak",
        "\\bigskip",
        "\\medskip",
        "\\smallskip",
        "\\vspace",
        "\\hspace",
        "\\label{",
        "\\title{",
        "\\author{",
        "\\date{",
    ];
    IGNORED_PREFIXES.iter().any(|p| trimmed.starts_with(p))
}

fn match_begin_env(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("\\begin{")?;
    let end = rest.find('}')?;
    Some(&rest[..end])
}

fn match_end_env(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("\\end{")?;
    let end = rest.find('}')?;
    Some(&rest[..end])
}

fn callout_for_env(base_env: &str) -> Option<(&'static str, &'static str)> {
    match base_env {
        "abstract" => Some(("ABSTRACT", "Abstract")),
        "theorem" | "thm" => Some(("NOTE", "Theorem")),
        "lemma" | "lem" => Some(("NOTE", "Lemma")),
        "proposition" | "prop" => Some(("NOTE", "Proposition")),
        "corollary" | "cor" => Some(("NOTE", "Corollary")),
        "definition" | "defn" => Some(("INFO", "Definition")),
        "example" | "ex" => Some(("TIP", "Example")),
        "proof" => Some(("TIP", "Proof")),
        "remark" | "note" => Some(("NOTE", "Remark")),
        "warning" | "caution" => Some(("WARNING", "Warning")),
        _ => None,
    }
}

fn parse_section_heading(trimmed: &str) -> Option<(usize, String)> {
    let commands: &[(&str, usize)] = &[
        ("\\part*", 1),
        ("\\part", 1),
        ("\\chapter*", 1),
        ("\\chapter", 1),
        ("\\section*", 2),
        ("\\section", 2),
        ("\\subsection*", 3),
        ("\\subsection", 3),
        ("\\subsubsection*", 4),
        ("\\subsubsection", 4),
        ("\\paragraph*", 5),
        ("\\paragraph", 5),
    ];

    for (cmd, level) in commands {
        if let Some(rest) = trimmed.strip_prefix(cmd) {
            let rest = rest.trim_start();
            // Skip optional short title [short]
            let rest = if let Some((_, after_opt)) = extract_bracket_group(rest) {
                after_opt.trim_start()
            } else {
                rest
            };
            if let Some((arg, _)) = extract_braced_group(rest) {
                return Some((*level, arg.to_string()));
            }
        }
    }
    None
}

fn extract_command_arg(source: &str, cmd: &str) -> Option<String> {
    let mut rest = source;
    while let Some(pos) = rest.find(cmd) {
        let after = &rest[pos + cmd.len()..];
        let after_trim = after.trim_start();
        let after_opt = if let Some((_, rem)) = extract_bracket_group(after_trim) {
            rem.trim_start()
        } else {
            after_trim
        };
        if let Some((arg, _)) = extract_braced_group(after_opt) {
            return Some(arg.to_string());
        }
        rest = after;
    }
    None
}

fn strip_command_with_arg(input: &str, cmd: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(pos) = rest.find(cmd) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + cmd.len()..];
        if let Some((_, rem)) = extract_braced_group(after.trim_start()) {
            rest = rem;
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn extract_braced_group(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('{')?;
    let mut depth = 1usize;
    let mut escaped = false;
    for (idx, ch) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return Some((&s[..idx], &s[idx + 1..]));
            }
        }
    }
    None
}

fn extract_bracket_group(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('[')?;
    let mut depth = 1usize;
    for (idx, ch) in s.char_indices() {
        if ch == '[' {
            depth += 1;
        } else if ch == ']' {
            depth -= 1;
            if depth == 0 {
                return Some((&s[..idx], &s[idx + 1..]));
            }
        }
    }
    None
}

fn skip_optional_and_braced(mut s: &str) -> &str {
    s = s.trim_start();
    if let Some((_, rem)) = extract_bracket_group(s) {
        s = rem.trim_start();
    }
    if let Some((_, rem)) = extract_braced_group(s) {
        s = rem;
    }
    s
}

fn convert_tabular_to_markdown(raw: &str) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for raw_row in raw.split("\\\\") {
        let mut cleaned_row = raw_row.trim();
        while let Some(rest) = cleaned_row
            .strip_prefix("\\hline")
            .or_else(|| cleaned_row.strip_prefix("\\toprule"))
            .or_else(|| cleaned_row.strip_prefix("\\midrule"))
            .or_else(|| cleaned_row.strip_prefix("\\bottomrule"))
        {
            cleaned_row = rest.trim();
        }
        cleaned_row = cleaned_row
            .trim_end_matches("\\hline")
            .trim_end_matches("\\bottomrule")
            .trim();
        if cleaned_row.is_empty() {
            continue;
        }
        let cells: Vec<String> = cleaned_row
            .split('&')
            .map(|c| convert_inline_latex(c.trim()))
            .collect();
        if !cells.is_empty() && cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells);
        }
    }

    if rows.is_empty() {
        return String::new();
    }
    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(1);
    let mut out = String::new();
    for (idx, row) in rows.iter().enumerate() {
        out.push('|');
        for col in 0..col_count {
            let cell = row.get(col).map(|s| s.as_str()).unwrap_or("");
            out.push(' ');
            out.push_str(cell);
            out.push_str(" |");
        }
        out.push('\n');
        if idx == 0 {
            out.push('|');
            for _ in 0..col_count {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out
}

/// Converts inline LaTeX commands (`\textbf`, `\emph`, `\texttt`, `\href`, `\ref`, `\cite`,
/// `\input`, `\include`, `\includegraphics`, `\( ... \)`) into Markdown inline syntax while
/// leaving `$ ... $` math spans untouched.
pub fn convert_inline_latex(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while !rest.is_empty() {
        // Preserve inline $...$ math untouched
        if let Some(after_dollar) = rest.strip_prefix('$') {
            if let Some(end) = after_dollar.find('$') {
                out.push('$');
                out.push_str(&after_dollar[..end + 1]);
                rest = &after_dollar[end + 1..];
                continue;
            }
        }

        // Convert \( ... \) to $ ... $
        if let Some(after_open) = rest.strip_prefix("\\(") {
            if let Some(end) = after_open.find("\\)") {
                out.push('$');
                out.push_str(after_open[..end].trim());
                out.push('$');
                rest = &after_open[end + 2..];
                continue;
            }
        }

        // Escaped characters
        for (esc, rep) in [
            ("\\%", "%"),
            ("\\&", "&"),
            ("\\_", "_"),
            ("\\#", "#"),
            ("\\$", "$"),
            ("\\{", "{"),
            ("\\}", "}"),
            ("---", "—"),
            ("--", "–"),
            ("``", "“"),
            ("''", "”"),
            ("~", " "),
        ] {
            if let Some(rem) = rest.strip_prefix(esc) {
                out.push_str(rep);
                rest = rem;
            }
        }
        if rest.is_empty() {
            break;
        }

        // Inline LaTeX commands starting with '\'
        if rest.starts_with('\\') {
            let cmd_end = rest[1..]
                .find(|c: char| !c.is_ascii_alphabetic() && c != '*')
                .map(|i| i + 1)
                .unwrap_or(rest.len());
            let cmd = &rest[..cmd_end];
            let after_cmd = &rest[cmd_end..];

            match cmd {
                "\\textbf" | "\\mathbf" | "\\underline" => {
                    if let Some((arg, rem)) = extract_braced_group(after_cmd.trim_start()) {
                        out.push_str("**");
                        out.push_str(&convert_inline_latex(arg));
                        out.push_str("**");
                        rest = rem;
                        continue;
                    }
                }
                "\\textit" | "\\emph" | "\\textsc" => {
                    if let Some((arg, rem)) = extract_braced_group(after_cmd.trim_start()) {
                        out.push('*');
                        out.push_str(&convert_inline_latex(arg));
                        out.push('*');
                        rest = rem;
                        continue;
                    }
                }
                "\\texttt" => {
                    if let Some((arg, rem)) = extract_braced_group(after_cmd.trim_start()) {
                        out.push('`');
                        out.push_str(arg);
                        out.push('`');
                        rest = rem;
                        continue;
                    }
                }
                "\\verb" => {
                    let mut chars = after_cmd.chars();
                    if let Some(delim) = chars.next() {
                        let rem_str = chars.as_str();
                        if let Some(end) = rem_str.find(delim) {
                            out.push('`');
                            out.push_str(&rem_str[..end]);
                            out.push('`');
                            rest = &rem_str[end + delim.len_utf8()..];
                            continue;
                        }
                    }
                }
                "\\href" => {
                    if let Some((url, rem1)) = extract_braced_group(after_cmd.trim_start()) {
                        if let Some((label, rem2)) = extract_braced_group(rem1.trim_start()) {
                            out.push('[');
                            out.push_str(&convert_inline_latex(label));
                            out.push_str("](");
                            out.push_str(url.trim());
                            out.push(')');
                            rest = rem2;
                            continue;
                        }
                    }
                }
                "\\url" => {
                    if let Some((url, rem)) = extract_braced_group(after_cmd.trim_start()) {
                        out.push('[');
                        out.push_str(url.trim());
                        out.push_str("](");
                        out.push_str(url.trim());
                        out.push(')');
                        rest = rem;
                        continue;
                    }
                }
                "\\hyperref" => {
                    if let Some((target, rem1)) = extract_bracket_group(after_cmd.trim_start()) {
                        if let Some((label, rem2)) = extract_braced_group(rem1.trim_start()) {
                            out.push_str("[[");
                            out.push_str(target.trim());
                            out.push('|');
                            out.push_str(&convert_inline_latex(label));
                            out.push_str("]]");
                            rest = rem2;
                            continue;
                        }
                    }
                }
                "\\ref" | "\\eqref" | "\\autoref" | "\\nameref" | "\\cite" | "\\citep"
                | "\\citet" | "\\input" | "\\include" | "\\bibliography" | "\\addbibresource" => {
                    let after_opt =
                        if let Some((_, rem)) = extract_bracket_group(after_cmd.trim_start()) {
                            rem.trim_start()
                        } else {
                            after_cmd.trim_start()
                        };
                    if let Some((target, rem)) = extract_braced_group(after_opt) {
                        for (idx, part) in target.split(',').enumerate() {
                            if idx > 0 {
                                out.push_str(", ");
                            }
                            out.push_str("[[");
                            out.push_str(part.trim());
                            out.push_str("]]");
                        }
                        rest = rem;
                        continue;
                    }
                }
                "\\includegraphics" => {
                    let after_opt =
                        if let Some((_, rem)) = extract_bracket_group(after_cmd.trim_start()) {
                            rem.trim_start()
                        } else {
                            after_cmd.trim_start()
                        };
                    if let Some((img_path, rem)) = extract_braced_group(after_opt) {
                        let p = img_path.trim();
                        out.push_str(&format!("![{p}]({p})"));
                        rest = rem;
                        continue;
                    }
                }
                "\\footnote" => {
                    if let Some((note, rem)) = extract_braced_group(after_cmd.trim_start()) {
                        out.push_str(" *(");
                        out.push_str(&convert_inline_latex(note));
                        out.push_str(")*");
                        rest = rem;
                        continue;
                    }
                }
                "\\label" => {
                    if let Some((_, rem)) = extract_braced_group(after_cmd.trim_start()) {
                        rest = rem;
                        continue;
                    }
                }
                "\\LaTeX" => {
                    out.push_str("LaTeX");
                    rest = after_cmd;
                    continue;
                }
                "\\TeX" => {
                    out.push_str("TeX");
                    rest = after_cmd;
                    continue;
                }
                _ => {
                    // If unknown command has a braced group, render the inner text; otherwise treat as inline math symbol
                    if let Some((arg, rem)) = extract_braced_group(after_cmd) {
                        out.push_str(&convert_inline_latex(arg));
                        rest = rem;
                        continue;
                    }
                }
            }
        }

        if let Some(ch) = rest.chars().next() {
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use terminalwiki_core::config::Config;

    #[test]
    fn converts_sections_formatting_and_math_to_markdown() {
        let tex = r#"
\documentclass{article}
\title{Quantum Mechanics Notes}
\author{M. Weiss}
\begin{document}
\maketitle

\section{Introduction}
We study $\psi(x)$ and \textbf{operators} with \emph{eigenvalues}.
See \href{https://example.com}{Reference} and \input{chapter2}.

\begin{equation}
\hat{H}\psi = E\psi
\end{equation}

\begin{theorem}[Spectral]
Every self-adjoint operator has real eigenvalues.
\end{theorem}

\begin{tabular}{ll}
\hline
State & Energy \\
\hline
$\psi_0$ & $E_0$ \\
\hline
\end{tabular}
\end{document}
"#;
        let doc = render_latex(
            None,
            tex,
            &Config::default(),
            &Theme::Dark,
            ColorMode::Never,
        );
        let rendered = doc
            .lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Quantum Mechanics Notes"), "{rendered}");
        assert!(rendered.contains("Introduction"), "{rendered}");
        assert!(rendered.contains("operators"), "{rendered}");
        assert!(rendered.contains("[[chapter2]]"), "{rendered}");
        assert!(rendered.contains("Theorem (Spectral)"), "{rendered}");
        assert!(
            rendered.contains("┌"),
            "tabular should render as Unicode table:\n{rendered}"
        );
    }

    #[test]
    fn extracts_latex_metadata_for_indexer() {
        let tex = r#"
% #physics #quantum
\title{Wave Functions}
\section{Hilbert Space}
See \input{operators} and [[Dirac Notation]].
"#;
        let meta = extract_latex_metadata(tex);
        assert_eq!(meta.title.as_deref(), Some("Wave Functions"));
        assert!(meta.headings.contains(&"Hilbert Space".to_string()));
        assert_eq!(meta.tags, vec!["physics", "quantum"]);
        assert!(meta.wiki_links.contains(&"operators".to_string()));
        assert!(meta.wiki_links.contains(&"Dirac Notation".to_string()));
    }
}
