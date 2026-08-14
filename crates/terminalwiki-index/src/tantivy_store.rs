//! Persistent Tantivy full-text search engine (spec §15, §16).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::{AllQuery, BooleanQuery, BoostQuery, Occur, Query as TantivyQuery, TermQuery};
use tantivy::schema::*;
use tantivy::snippet::SnippetGenerator;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, Term};
use terminalwiki_core::{Error, Result};

use crate::entry::IndexEntry;
use crate::query::{Query, QueryTerm};

pub const INDEX_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMeta {
    pub schema_version: u32,
    pub built_at: u64,
    pub document_count: usize,
}

/// Persistent Tantivy-backed search engine for a wiki.
pub struct TantivyStore {
    index: Index,
    reader: IndexReader,
    _schema: Schema,
    f_wiki: Field,
    f_path: Field,
    f_relative: Field,
    f_title: Field,
    f_aliases: Field,
    f_headings: Field,
    f_body: Field,
    f_tags: Field,
    f_extension: Field,
    f_content_type: Field,
    f_mtime: Field,
    f_size: Field,
}

/// A search result hit with BM25 score and contextual snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub wiki: String,
    pub path: PathBuf,
    pub relative: PathBuf,
    pub title: String,
    pub score: f32,
    pub snippet: Option<String>,
}

impl TantivyStore {
    fn build_schema() -> (
        Schema,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
    ) {
        let mut builder = Schema::builder();

        let text_indexing = TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let text_options = TextOptions::default().set_indexing_options(text_indexing).set_stored();

        let f_wiki = builder.add_text_field("wiki", STRING | STORED);
        let f_path = builder.add_text_field("path", STRING | STORED);
        let f_relative = builder.add_text_field("relative", STRING | STORED);
        let f_title = builder.add_text_field("title", text_options.clone());
        let f_aliases = builder.add_text_field("aliases", text_options.clone());
        let f_headings = builder.add_text_field("headings", text_options.clone());
        let f_body = builder.add_text_field("body", text_options);
        let f_tags = builder.add_text_field("tags", STRING | STORED);
        let f_extension = builder.add_text_field("extension", STRING | STORED);
        let f_content_type = builder.add_text_field("content_type", STRING | STORED);
        let f_mtime = builder.add_i64_field("mtime", STORED);
        let f_size = builder.add_u64_field("size", STORED);

        let schema = builder.build();
        (
            schema,
            f_wiki,
            f_path,
            f_relative,
            f_title,
            f_aliases,
            f_headings,
            f_body,
            f_tags,
            f_extension,
            f_content_type,
            f_mtime,
            f_size,
        )
    }

    /// Read-only access to an existing Tantivy index in `dir/tantivy`.
    pub fn open_reader(dir: &Path) -> Result<Self> {
        let tantivy_dir = dir.join("tantivy");
        if !tantivy_dir.exists() {
            return Err(Error::index("Search index is unavailable. Run 'tw index rebuild' first."));
        }

        let (
            schema,
            f_wiki,
            f_path,
            f_relative,
            f_title,
            f_aliases,
            f_headings,
            f_body,
            f_tags,
            f_extension,
            f_content_type,
            f_mtime,
            f_size,
        ) = Self::build_schema();

        let index = Index::open_in_dir(&tantivy_dir)
            .map_err(|e| Error::index(format!("Failed to open index: {e}. Run 'tw index rebuild'.")))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| Error::index(format!("Failed to create index reader: {e}")))?;

        Ok(Self {
            index,
            reader,
            _schema: schema,
            f_wiki,
            f_path,
            f_relative,
            f_title,
            f_aliases,
            f_headings,
            f_body,
            f_tags,
            f_extension,
            f_content_type,
            f_mtime,
            f_size,
        })
    }

    /// Creates or opens index for writing in `dir/tantivy`.
    pub fn open_or_create(dir: &Path) -> Result<Self> {
        let tantivy_dir = dir.join("tantivy");
        fs::create_dir_all(&tantivy_dir).map_err(|e| Error::io(&tantivy_dir, e))?;

        let (
            schema,
            f_wiki,
            f_path,
            f_relative,
            f_title,
            f_aliases,
            f_headings,
            f_body,
            f_tags,
            f_extension,
            f_content_type,
            f_mtime,
            f_size,
        ) = Self::build_schema();

        let index = Index::open_in_dir(&tantivy_dir)
            .unwrap_or_else(|_| Index::create_in_dir(&tantivy_dir, schema.clone()).expect("create tantivy index"));

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| Error::index(format!("Failed to create index reader: {e}")))?;

        Ok(Self {
            index,
            reader,
            _schema: schema,
            f_wiki,
            f_path,
            f_relative,
            f_title,
            f_aliases,
            f_headings,
            f_body,
            f_tags,
            f_extension,
            f_content_type,
            f_mtime,
            f_size,
        })
    }

    /// Indexes entries in the Tantivy index.
    pub fn update_entries(&mut self, _dir: &Path, entries: &[IndexEntry]) -> Result<()> {
        let mut writer: IndexWriter = self
            .index
            .writer(50_000_000) // 50 MB buffer
            .map_err(|e| Error::index(format!("Failed to acquire index writer: {e}")))?;

        // Clear existing docs to prevent duplication on rebuild
        writer
            .delete_all_documents()
            .map_err(|e| Error::index(format!("Failed to clear index: {e}")))?;

        for entry in entries {
            let rel_str = entry.relative.to_string_lossy().to_string();
            let path_str = entry.path.to_string_lossy().to_string();
            let ext_str = entry
                .relative
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            let ct_str = entry.content_type;
            let ct_name = terminalwiki_core::filetype::ContentType::from(ct_str).as_str();

            let mut doc = doc!(
                self.f_wiki => entry.wiki.clone(),
                self.f_path => path_str,
                self.f_relative => rel_str,
                self.f_title => entry.title.clone(),
                self.f_aliases => entry.aliases.join(" "),
                self.f_headings => entry.headings.join(" "),
                self.f_body => entry.body_text.clone(),
                self.f_extension => ext_str,
                self.f_content_type => ct_name,
                self.f_mtime => entry.mtime as i64,
                self.f_size => entry.size,
            );

            for tag in &entry.tags {
                doc.add_text(self.f_tags, tag);
            }

            writer.add_document(doc).map_err(|e| Error::index(e.to_string()))?;
        }

        writer
            .commit()
            .map_err(|e| Error::index(format!("Failed to commit index: {e}")))?;

        self.reader
            .reload()
            .map_err(|e| Error::index(format!("Failed to reload reader: {e}")))?;

        Ok(())
    }

    /// Searches the index with a structured `Query`.
    pub fn search(&self, query: &Query, limit: usize) -> Result<Vec<SearchHit>> {
        let searcher = self.reader.searcher();
        let tantivy_query = self.build_tantivy_query(query)?;

        let top_docs = searcher
            .search(&tantivy_query, &TopDocs::with_limit(limit))
            .map_err(|e| Error::index(format!("Search execution failed: {e}")))?;

        let mut hits = Vec::new();
        let snippet_gen = SnippetGenerator::create(&searcher, &tantivy_query, self.f_body)
            .ok();

        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| Error::index(e.to_string()))?;

            let wiki = doc.get_first(self.f_wiki).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let path_str = doc.get_first(self.f_path).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let rel_str = doc.get_first(self.f_relative).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = doc.get_first(self.f_title).and_then(|v| v.as_str()).unwrap_or("").to_string();

            let snippet = if let Some(ref gen) = snippet_gen {
                let s = gen.snippet_from_doc(&doc);
                let html = s.to_html();
                if !html.is_empty() {
                    let clean = html.replace("<b>", "").replace("</b>", "");
                    Some(clean)
                } else {
                    None
                }
            } else {
                None
            };

            hits.push(SearchHit {
                wiki,
                path: PathBuf::from(path_str),
                relative: PathBuf::from(rel_str),
                title,
                score,
                snippet,
            });
        }

        Ok(hits)
    }

    fn build_tantivy_query(&self, query: &Query) -> Result<Box<dyn TantivyQuery>> {
        if query.terms.is_empty() {
            return Ok(Box::new(AllQuery));
        }

        let mut clauses = Vec::new();

        for term in &query.terms {
            match term {
                QueryTerm::Text(t) => {
                    let mut sub_clauses: Vec<(Occur, Box<dyn TantivyQuery>)> = Vec::new();
                    for word in t.split_whitespace() {
                        let w_lower = word.to_lowercase();
                        let q_title: Box<dyn TantivyQuery> = Box::new(BoostQuery::new(
                            Box::new(TermQuery::new(
                                Term::from_field_text(self.f_title, &w_lower),
                                IndexRecordOption::WithFreqsAndPositions,
                            )),
                            5.0, // Title matches boosted by 5x
                        ));
                        let q_body: Box<dyn TantivyQuery> = Box::new(TermQuery::new(
                            Term::from_field_text(self.f_body, &w_lower),
                            IndexRecordOption::WithFreqsAndPositions,
                        ));
                        sub_clauses.push((Occur::Should, q_title));
                        sub_clauses.push((Occur::Should, q_body));
                    }
                    if !sub_clauses.is_empty() {
                        clauses.push((Occur::Must, Box::new(BooleanQuery::new(sub_clauses)) as Box<dyn TantivyQuery>));
                    }
                }
                QueryTerm::Tag(t) => {
                    let q: Box<dyn TantivyQuery> = Box::new(TermQuery::new(
                        Term::from_field_text(self.f_tags, t),
                        IndexRecordOption::Basic,
                    ));
                    clauses.push((Occur::Must, q));
                }
                QueryTerm::Wiki(w) => {
                    let q: Box<dyn TantivyQuery> = Box::new(TermQuery::new(
                        Term::from_field_text(self.f_wiki, w),
                        IndexRecordOption::Basic,
                    ));
                    clauses.push((Occur::Must, q));
                }
                QueryTerm::Type(t) => {
                    let q: Box<dyn TantivyQuery> = Box::new(TermQuery::new(
                        Term::from_field_text(self.f_content_type, t),
                        IndexRecordOption::Basic,
                    ));
                    clauses.push((Occur::Must, q));
                }
                QueryTerm::Ext(e) => {
                    let q: Box<dyn TantivyQuery> = Box::new(TermQuery::new(
                        Term::from_field_text(self.f_extension, e),
                        IndexRecordOption::Basic,
                    ));
                    clauses.push((Occur::Must, q));
                }
                QueryTerm::Title(t) => {
                    let q: Box<dyn TantivyQuery> = Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(
                            Term::from_field_text(self.f_title, &t.to_lowercase()),
                            IndexRecordOption::WithFreqsAndPositions,
                        )),
                        10.0,
                    ));
                    clauses.push((Occur::Must, q));
                }
                QueryTerm::Path(p) => {
                    let q: Box<dyn TantivyQuery> = Box::new(TermQuery::new(
                        Term::from_field_text(self.f_relative, p),
                        IndexRecordOption::Basic,
                    ));
                    clauses.push((Occur::Must, q));
                }
                QueryTerm::LinksTo(l) => {
                    return Err(Error::invalid_arguments(format!(
                        "Query filter 'linksto:{l}' is only supported in graph queries"
                    )));
                }
                QueryTerm::Backlink(b) => {
                    return Err(Error::invalid_arguments(format!(
                        "Query filter 'backlink:{b}' is only supported in graph queries"
                    )));
                }
                QueryTerm::Not(inner) => {
                    if let QueryTerm::Tag(t) = &**inner {
                        let q: Box<dyn TantivyQuery> = Box::new(TermQuery::new(
                            Term::from_field_text(self.f_tags, t),
                            IndexRecordOption::Basic,
                        ));
                        clauses.push((Occur::MustNot, q));
                    }
                }
            }
        }

        if clauses.is_empty() {
            Ok(Box::new(AllQuery))
        } else {
            Ok(Box::new(BooleanQuery::new(clauses)))
        }
    }
}
