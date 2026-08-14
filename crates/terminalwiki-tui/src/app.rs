//! TUI Application state and business logic (spec §49-§56).

use std::fs;
use std::path::PathBuf;

use terminalwiki_core::filetype::{classify, ContentType};
use terminalwiki_core::resolve;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_graph::{BacklinkInfo, GraphEntry, WikiGraph};
use terminalwiki_index::{FuzzyDataset, FuzzyHit, FuzzyItem};
use terminalwiki_render::{
    detect_color_mode, render_code_file, render_markdown, ColorMode, RenderedDocument, RenderedLine,
    Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Finder,
    Outline,
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
    pub headings: Vec<(usize, String, usize)>, // (level, text, line_index)
    pub raw_content: String,
    pub scroll: usize,

    pub history_back: Vec<(String, PathBuf, usize)>,
    pub history_forward: Vec<(String, PathBuf, usize)>,

    pub extracted_links: Vec<(usize, String)>, // (line_idx, target)
    pub selected_link_idx: Option<usize>,

    pub mode: Mode,

    // Finder (Nucleo-powered)
    pub finder_query: String,
    pub fuzzy_dataset: FuzzyDataset,
    pub finder_filtered: Vec<FuzzyHit>,
    pub finder_selected: usize,

    // Outline
    pub outline_selected: usize,

    // Backlinks
    pub backlinks: Vec<BacklinkInfo>,
    pub backlinks_selected: usize,

    // In-page search
    pub in_page_query: String,
    pub search_matches: Vec<usize>,

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

        // Build Nucleo fuzzy items from index or file scan
        let mut fuzzy_items = Vec::new();
        for wiki in wikis.iter() {
            if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
                for e in &idx.entries {
                    fuzzy_items.push(FuzzyItem {
                        wiki: wiki.name.clone(),
                        relative: e.relative.clone(),
                        title: e.title.clone(),
                        aliases: e.aliases.clone(),
                        tags: e.tags.clone(),
                    });
                }
            } else {
                let files = terminalwiki_core::scan::scan(
                    wiki,
                    &terminalwiki_core::config::IndexConfig::default(),
                );
                for f in files {
                    let title = f
                        .relative
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    fuzzy_items.push(FuzzyItem {
                        wiki: wiki.name.clone(),
                        relative: f.relative,
                        title,
                        aliases: Vec::new(),
                        tags: Vec::new(),
                    });
                }
            }
        }

        let mut dataset = FuzzyDataset::new(fuzzy_items);
        let initial_hits = dataset.find("", 30);

        let mut app = Self {
            wikis,
            config,
            theme,
            color_mode,
            current_wiki: default_wiki.clone(),
            current_path: PathBuf::new(),
            current_title: String::new(),
            lines: Vec::new(),
            headings: Vec::new(),
            raw_content: String::new(),
            scroll: 0,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            extracted_links: Vec::new(),
            selected_link_idx: None,
            mode: Mode::Normal,
            finder_query: String::new(),
            fuzzy_dataset: dataset,
            finder_filtered: initial_hits,
            finder_selected: 0,
            outline_selected: 0,
            backlinks: Vec::new(),
            backlinks_selected: 0,
            in_page_query: String::new(),
            search_matches: Vec::new(),
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
        } else {
            let _ = app.load_home(&default_wiki);
        }

        Ok(app)
    }

    pub fn load_home(&mut self, wiki_name: &str) -> Result<()> {
        let candidates = ["index.md", "README.md", "Home.md", "home.md"];
        for c in &candidates {
            if self.load_page(wiki_name, c, false).is_ok() {
                return Ok(());
            }
        }
        if let Some(first) = self.finder_filtered.first() {
            let p = first.relative.to_string_lossy().into_owned();
            let w = first.wiki.clone();
            return self.load_page(&w, &p, false);
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
        let content_type = classify(&resolution.path, &bytes);

        let doc: RenderedDocument = if content_type == ContentType::Code {
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

        self.current_wiki = resolution.wiki.clone();
        self.current_path = resolution.relative.clone();
        self.current_title = resolution
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| page_str.to_string());
        self.lines = doc.lines;
        self.headings = doc.headings;
        self.extracted_links = doc.links;
        self.raw_content = text;
        self.scroll = 0;
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
        self.finder_filtered = self.fuzzy_dataset.find(&self.finder_query, 30);
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
                        content_type: terminalwiki_core::filetype::ContentType::from(e.content_type).as_str().to_string(),
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
