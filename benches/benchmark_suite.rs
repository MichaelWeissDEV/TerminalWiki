//! Benchmark suite measuring core operations (spec §35, §79, §80).

use std::time::Instant;

fn main() {
    println!("─── TerminalWiki Benchmark Suite ───\n");

    // 1. Benchmark: Markdown parsing and rendering
    let sample_markdown = r#"---
title: Memory Management & Exploitation
tags: [security, heap, linux]
aliases: [HeapExploit]
---

# Memory Management & Heap Exploitation

Modern Linux glibc allocators use chunk-based allocations with bins and tcache.

## Key Allocator Structures
- Fastbins
- Tcache bins
- Small and Large bins

See also: [[Linux/Kernel]] and [[Security/UAF]].

```rust
fn allocate_chunk(size: usize) -> *mut u8 {
    unsafe { libc::malloc(size) as *mut u8 }
}
```
"#;

    let config = terminalwiki_core::Config::default();
    let theme = terminalwiki_render::Theme::Dark;
    let color_mode = terminalwiki_render::ColorMode::Never;

    let iters = 5_000;
    let t0 = Instant::now();
    for _ in 0..iters {
        let _doc = terminalwiki_render::render_markdown(sample_markdown, &config, &theme, color_mode);
    }
    let elapsed = t0.elapsed();
    println!(
        "Markdown Parse & Render: {:?} total for {} iterations ({:.2} µs/op)",
        elapsed,
        iters,
        elapsed.as_micros() as f64 / iters as f64
    );

    // 2. Benchmark: Syntax highlighting
    let sample_code = r#"
use std::collections::HashMap;

pub struct Graph<N, E> {
    nodes: Vec<N>,
    edges: Vec<E>,
    adj: HashMap<usize, Vec<usize>>,
}

impl<N, E> Graph<N, E> {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new(), adj: HashMap::new() }
    }
}
"#;

    let t1 = Instant::now();
    for _ in 0..iters {
        let _highlighted = terminalwiki_render::highlight::highlight(sample_code, Some("rs"), &theme);
    }
    let elapsed1 = t1.elapsed();
    println!(
        "Syntect Highlighting (Rust): {:?} total for {} iterations ({:.2} µs/op)",
        elapsed1,
        iters,
        elapsed1.as_micros() as f64 / iters as f64
    );

    // 3. Benchmark: Nucleo Fuzzy matching over 10,000 items
    let mut items = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        items.push(terminalwiki_index::FuzzyItem {
            wiki: "main".into(),
            relative: std::path::PathBuf::from(format!("security/heap_exploit_{i}.md")),
            title: format!("Heap Exploitation Technique {i}"),
            aliases: vec![format!("Heap{i}")],
            tags: vec!["security".into(), "heap".into()],
        });
    }

    let mut dataset = terminalwiki_index::FuzzyDataset::new(items);
    let fuzzy_iters = 1_000;
    let t2 = Instant::now();
    for _ in 0..fuzzy_iters {
        let _hits = dataset.find("heap exploit 999", 20);
    }
    let elapsed2 = t2.elapsed();
    println!(
        "Nucleo Fuzzy Search (10k items): {:?} total for {} iterations ({:.2} µs/query)",
        elapsed2,
        fuzzy_iters,
        elapsed2.as_micros() as f64 / fuzzy_iters as f64
    );

    println!("\nAll benchmark benchmarks finished.");
}
