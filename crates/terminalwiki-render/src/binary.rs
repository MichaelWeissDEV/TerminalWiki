use std::path::Path;
use std::process::Command;

use crate::document::{LinkTarget, RenderedDocument, RenderedHeading, RenderedLink, Span};
use crate::theme::{SemanticColor, Theme};
use terminalwiki_core::filetype::human_size;

pub fn render_binary_info(path: &Path, size: u64, _mtime: u64) -> RenderedDocument {
    let is_pdf = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));

    if is_pdf {
        return render_pdf_document(path, size);
    }

    let mut doc = RenderedDocument {
        lines: Vec::new(),
        headings: Vec::new(),
        links: Vec::new(),
    };
    let theme = Theme::Dark;
    doc.lines.push(vec![Span {
        text: format!("Binary file: {} ({} bytes)", path.display(), size),
        style: theme.style(SemanticColor::Foreground),
    }]);
    doc
}

fn render_pdf_document(path: &Path, size: u64) -> RenderedDocument {
    let theme = Theme::Dark;
    let title = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    let mut doc = RenderedDocument {
        lines: Vec::new(),
        headings: vec![RenderedHeading {
            level: 1,
            text: title.clone(),
            anchor: "pdf-document".to_string(),
            line: 0,
        }],
        links: Vec::new(),
    };

    doc.lines.push(vec![Span {
        text: format!("📕 PDF Document: {title}"),
        style: theme.style(SemanticColor::Heading).bold(),
    }]);
    doc.lines.push(vec![Span {
        text: format!(
            "Size: {} ({} bytes) · Path: {}",
            human_size(size),
            size,
            path.display()
        ),
        style: theme.style(SemanticColor::Muted),
    }]);

    // Register an external/viewer open link on line 2 so Enter / 'x' opens the PDF in the system viewer
    let open_label = format!("[Open {} in PDF Viewer]", title);
    doc.links.push(RenderedLink {
        line: 2,
        start_column: 0,
        end_column: open_label.len(),
        target: LinkTarget::External(path.to_string_lossy().to_string()),
        label: open_label.clone(),
    });
    doc.lines.push(vec![Span {
        text: open_label,
        style: theme.style(SemanticColor::Link).bold().underline(),
    }]);
    doc.lines.push(Vec::new());

    // Try extracting text preview with `pdftotext` if available on the system
    if let Some(preview) = extract_pdf_text_preview(path) {
        doc.lines.push(vec![Span {
            text: "── Extracted Text Preview ──────────────────────────────────────".to_string(),
            style: theme.style(SemanticColor::Muted),
        }]);
        for line in preview.lines().take(200) {
            doc.lines.push(vec![Span {
                text: line.to_string(),
                style: theme.style(SemanticColor::Foreground),
            }]);
        }
    } else {
        doc.lines.push(vec![Span {
            text: "Tip: Press Enter or 'x' to open this PDF in your system viewer (or install 'pdftotext' for inline text preview).".to_string(),
            style: theme.style(SemanticColor::Muted).italic(),
        }]);
    }

    doc
}

fn extract_pdf_text_preview(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let output = Command::new("pdftotext")
        .args(["-l", "5", "-nopgbrk"])
        .arg(path)
        .arg("-")
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}
