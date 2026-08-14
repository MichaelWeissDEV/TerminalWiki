//! Fixture generator for benchmark wikis (spec §34).
//!
//! Generates synthetic wikis with 1,000, 10,000, or 100,000 pages containing
//! realistic frontmatter, headings, wiki links, tags, and code blocks.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let count: usize = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let target_dir: PathBuf = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("target/bench_wiki_{count}")));

    println!("Generating benchmark wiki with {count} pages at {} …", target_dir.display());

    fs::create_dir_all(&target_dir).expect("create target dir");

    // Create index.md
    let index_file = target_dir.join("index.md");
    let mut f = BufWriter::new(File::create(index_file).unwrap());
    writeln!(f, "---\ntitle: Benchmark Wiki\ntags: [benchmark, root]\n---\n\n# Benchmark Knowledge Base\n\nWelcome to the benchmark wiki with {count} pages.\n").unwrap();

    let subdirs = ["security", "kernel", "languages/rust", "languages/c", "algorithms", "systems"];
    for sub in &subdirs {
        fs::create_dir_all(target_dir.join(sub)).unwrap();
    }

    let tags_pool = ["security", "memory", "performance", "linux", "compiler", "network", "database", "concurrency"];

    for i in 1..=count {
        let subdir = subdirs[i % subdirs.len()];
        let tag1 = tags_pool[i % tags_pool.len()];
        let tag2 = tags_pool[(i + 3) % tags_pool.len()];
        let link_target_idx = (i * 7) % count + 1;

        let filename = format!("page_{i:06}.md");
        let path = target_dir.join(subdir).join(&filename);

        let mut out = BufWriter::new(File::create(path).unwrap());
        writeln!(out, "---").unwrap();
        writeln!(out, "title: Document {i:06} - Topic {tag1}").unwrap();
        writeln!(out, "aliases: [Doc{i}, Page{i}]").unwrap();
        writeln!(out, "tags: [{tag1}, {tag2}]").unwrap();
        writeln!(out, "---\n").unwrap();
        writeln!(out, "# Document {i:06}\n").unwrap();
        writeln!(out, "This is synthetic document {i} exploring **{tag1}** and **{tag2}** principles.").unwrap();
        writeln!(out, "Reference to related page: [[page_{link_target_idx:06}]].\n").unwrap();
        writeln!(out, "## Implementation Details\n").unwrap();
        writeln!(out, "```rust\nfn process_item_{i}(val: u64) -> Result<u64, String> {{\n    if val == 0 {{\n        Err(\"zero\".to_string())\n    }} else {{\n        Ok(val * 42 + {i})\n    }}\n}}\n```\n").unwrap();
    }

    println!("Generated {count} pages successfully.");
}
