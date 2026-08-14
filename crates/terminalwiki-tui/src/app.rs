use std::fs;
use std::path::PathBuf;

use terminalwiki_core::fuzzy;
use terminalwiki_core::resolve;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_graph::{BacklinkInfo, GraphEntry, WikiGraph};
use terminalwiki_render::{
    detect_color_mode, render_code_file, render_markdown, ColorMode, RenderedDocument, RenderedLine,
    Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Finder,
    InPageSearch,
    Backlinks,
    Help,
}

pub struct App<'a> {
    pub wikis: &'a WikiSet,
    pub config: &'a Config,
    pub theme: Theme,
    pub color_mode: ColorMode,

    pub current_wiki: String,
    pub current_path: PathBuf,
    pub current_title: String,
    pub lines: Vec<RenderedLine>,
    pub raw_content: String,
    pub scroll: usize,

    pub history_back: Vec<(String, PathBuf, usize)>,
    pub history_forward: Vec<(String, PathBuf, usize)>,

    pub extracted_links: Vec<String>,
    pub selected_link_idx: Option<usize>,

    pub mode: Mode,

    // Finder
    pub finder_query: String,
    pub finder_candidates: Vec<(String, String, String)>, // (wiki, rel_path, title)
    pub finder_filtered: Vec<(String, String, String)>,
    pub finder_selected: usize,

    // Backlinks
    pub backlinks: Vec<BacklinkInfo>,
    pub backlinks_selected: usize,

    // In-page search
    pub in_page_query: String,
    pub search_matches: Vec<usize>, // line indices
    pub search_match_idx: usize,

    pub status_message: Option<String>,
    pub should_quit: bool,
    pub should_suspend_for_editor: Option<PathBuf>,
}

impl<'a> App<'a> {
    pub fn new(
        wikis: &'a WikiSet,
        config: &'a Config,
        initial_wiki: Option<String>,
        initial_page: Option<String>,
    ) -> Result<Self> {
        let theme = match config.theme {
            terminalwiki_core::config::Theme::Light => Theme::Light,
            terminalwiki_core::config::Theme::Mono => Theme::Mono,
            _ => Theme::Dark,
        };
        let color_mode = detect_color_mode();

        let default_wiki = initial_wiki
            .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
            .ok_or(Error::NoWikiConfigured)?;

        // Preload candidates for finder
        let mut finder_candidates = Vec::new();
        for wiki in wikis.iter() {
            if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
                for e in &idx.entries {
                    finder_candidates.push((
                        wiki.name.clone(),
                        e.relative.to_string_lossy().into_owned(),
                        e.title.clone(),
                    ));
                }
            } else {
                let files = terminalwiki_core::scan::scan(
                    wiki,
                    &terminalwiki_core::config::IndexConfig::default(),
                );
                for f in files {
                    let rel = f.relative.to_string_lossy().into_owned();
                    let title = f
                        .relative
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    finder_candidates.push((wiki.name.clone(), rel, title));
                }
            }
        }

        let mut app = Self {
            wikis,
            config,
            theme,
            color_mode,
            current_wiki: default_wiki.clone(),
            current_path: PathBuf::new(),
            current_title: String::new(),
            lines: Vec::new(),
            raw_content: String::new(),
            scroll: 0,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            extracted_links: Vec::new(),
            selected_link_idx: None,
            mode: Mode::Normal,
            finder_query: String::new(),
            finder_filtered: finder_candidates.clone(),
            finder_candidates,
            finder_selected: 0,
            backlinks: Vec::new(),
            backlinks_selected: 0,
            in_page_query: String::new(),
            search_matches: Vec::new(),
            search_match_idx: 0,
            status_message: None,
            should_quit: false,
            should_suspend_for_editor: None,
        };

        if let Some(page) = initial_page {
            if !page.is_empty() {
                let _ = app.load_page(&default_wiki, &page, true);
            } else {
                let _ = app.load_home(&default_wiki);
            }
        } else if !app.finder_candidates.is_empty() {
            let _ = app.load_home(&default_wiki);
        }

        Ok(app)
    }

    pub fn load_home(&mut self, wiki_name: &str) -> Result<()> {
        let candidates = ["index.md", "README.md", "Home.md", "index"];
        for c in &candidates {
            if self.load_page(wiki_name, c, false).is_ok() {
                return Ok(());
            }
        }
        if let Some(first) = self.finder_candidates.iter().find(|(w, _, _)| w == wiki_name) {
            let p = first.1.clone();
            return self.load_page(wiki_name, &p, false);
        }
        self.mode = Mode::Finder;
        Ok(())
    }

    pub fn load_page(&mut self, wiki_name: &str, page_str: &str, record_history: bool) -> Result<()> {
        let resolution = resolve::resolve(self.wikis, wiki_name, page_str, &self.config.index)?;

        if record_history && !self.current_path.as_os_str().is_empty() {
            self.history_back.push((
                self.current_wiki.clone(),
                self.current_path.clone(),
                self.scroll,
            ));
            self.history_forward.clear();
        }

        let bytes = fs::read(&resolution.path).map_err(|e| Error::io(&resolution.path, e))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();

        let content_type = terminalwiki_core::filetype::classify(&resolution.path, &bytes);
        let doc: RenderedDocument = if content_type == terminalwiki_core::filetype::ContentType::Code {
            let lang = terminalwiki_core::filetype::language_for(&resolution.path);
            render_code_file(
                &text,
                lang,
                &resolution.path,
                self.config,
                &self.theme,
                self.color_mode,
                None,
            )
        } else {
            render_markdown(&text, self.config, &self.theme, self.color_mode)
        };

        let mut extracted_links = Vec::new();
        for (_range, link) in terminalwiki_core::link::find_links(&text) {
            if let terminalwiki_core::link::LinkTarget::Page { name, .. } = link.target {
                extracted_links.push(name);
            }
        }

        self.current_wiki = resolution.wiki.clone();
        self.current_path = resolution.relative.clone();
        self.current_title = resolution
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| page_str.to_string());
        self.lines = doc.lines;
        self.raw_content = text;
        self.scroll = 0;
        self.extracted_links = extracted_links;
        self.selected_link_idx = if !self.extracted_links.is_empty() {
            Some(0)
        } else {
            None
        };
        self.status_message = None;

        Ok(())
    }

    pub fn go_back(&mut self) {
        if let Some((wiki, path, scroll)) = self.history_back.pop() {
            self.history_forward.push((
                self.current_wiki.clone(),
                self.current_path.clone(),
                self.scroll,
            ));
            let path_str = path.to_string_lossy().into_owned();
            if self.load_page(&wiki, &path_str, false).is_ok() {
                self.scroll = scroll;
            }
        } else {
            self.status_message = Some("Already at oldest history entry".to_string());
        }
    }

    pub fn go_forward(&mut self) {
        if let Some((wiki, path, scroll)) = self.history_forward.pop() {
            self.history_back.push((
                self.current_wiki.clone(),
                self.current_path.clone(),
                self.scroll,
            ));
            let path_str = path.to_string_lossy().into_owned();
            if self.load_page(&wiki, &path_str, false).is_ok() {
                self.scroll = scroll;
            }
        } else {
            self.status_message = Some("Already at newest history entry".to_string());
        }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize, view_height: usize) {
        let max_scroll = self.lines.len().saturating_sub(view_height);
        self.scroll = (self.scroll + amount).min(max_scroll);
    }

    pub fn update_finder_filter(&mut self) {
        if self.finder_query.is_empty() {
            self.finder_filtered = self.finder_candidates.clone();
            self.finder_selected = 0;
            return;
        }

        let query = &self.finder_query;
        let mut scored: Vec<(i32, (String, String, String))> = self
            .finder_candidates
            .iter()
            .filter_map(|c| {
                let s1 = fuzzy::score(query, &c.2).map(|m| m.score + 20);
                let s2 = fuzzy::score(query, &c.1).map(|m| m.score);
                let best = s1.max(s2)?;
                Some((best, c.clone()))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.finder_filtered = scored.into_iter().map(|s| s.1).collect();
        self.finder_selected = 0;
    }

    pub fn load_backlinks(&mut self) {
        let mut entries = Vec::new();
        for wiki in self.wikis.iter() {
            if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
                for e in idx.entries {
                    entries.push(GraphEntry {
                        wiki: e.wiki,
                        relative: e.relative,
                        content_type: e.content_type.as_str().to_string(),
                        title: e.title,
                        tags: e.tags,
                        wiki_links: e.wiki_links,
                        image_links: Vec::new(),
                    });
                }
            }
        }
        let graph = WikiGraph::from_entries(&entries);
        self.backlinks = graph.backlinks(&self.current_wiki, &self.current_path);
        self.backlinks_selected = 0;
    }

    pub fn open_current_in_editor(&mut self) {
        if let Some(wiki) = self.wikis.get(&self.current_wiki) {
            let full_path = wiki.root.join(&self.current_path);
            self.should_suspend_for_editor = Some(full_path);
        }
    }
}
