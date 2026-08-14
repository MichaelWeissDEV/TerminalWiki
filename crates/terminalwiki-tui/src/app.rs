//! TUI Application state and business logic (spec §40-§53).

use std::fs;
use std::path::PathBuf;

use terminalwiki_core::resolve;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_graph::{BacklinkInfo, GraphEntry, WikiGraph};
use terminalwiki_index::{FuzzyDataset, FuzzyHit, FuzzyItem};
use terminalwiki_render::{
    detect_color_mode, render_markdown, render_path, ColorMode, RenderedDocument, RenderedHeading,
    RenderedLine, RenderedLink, Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Finder,
    Outline,
    InPageSearch,
    Backlinks,
    Command,
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
    pub headings: Vec<RenderedHeading>,
    pub links: Vec<RenderedLink>,
    pub raw_content: String,
    pub scroll: usize,
    pub h_scroll: usize,

    pub history_back: Vec<(String, PathBuf, usize, usize)>,
    pub history_forward: Vec<(String, PathBuf, usize, usize)>,

    pub selected_link_idx: Option<usize>,

    pub mode: Mode,

    // Finder (Nucleo-powered inline view)
    pub finder_query: String,
    pub fuzzy_dataset: FuzzyDataset,
    pub finder_filtered: Vec<FuzzyHit>,
    pub finder_selected: usize,

    // Outline (inline view)
    pub outline_selected: usize,

    // Backlinks (inline view)
    pub backlinks: Vec<BacklinkInfo>,
    pub backlinks_selected: usize,

    // Command Palette (':')
    pub command_input: String,
    pub command_suggestions: Vec<String>,
    pub command_selected: usize,

    // In-page search
    pub in_page_query: String,
    pub search_matches: Vec<usize>,

    pub status_message: Option<String>,
    pub should_quit: bool,
    pub should_suspend_for_editor: Option<PathBuf>,
}

const COMMAND_LIST: &[&str] = &[
    "open",
    "search",
    "find",
    "backlinks",
    "outline",
    "graph",
    "edit",
    "wiki",
    "reload",
    "quit",
];

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

        // Build Nucleo fuzzy dataset from index state
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
            links: Vec::new(),
            raw_content: String::new(),
            scroll: 0,
            h_scroll: 0,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            selected_link_idx: None,
            mode: Mode::Normal,
            finder_query: String::new(),
            fuzzy_dataset: dataset,
            finder_filtered: initial_hits,
            finder_selected: 0,
            outline_selected: 0,
            backlinks: Vec::new(),
            backlinks_selected: 0,
            command_input: String::new(),
            command_suggestions: Vec::new(),
            command_selected: 0,
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
        let wiki = self.wikis.require(wiki_name)?;

        if let Some(home_path) = wiki.home_page() {
            let path_str = home_path.to_string_lossy().into_owned();
            return self.load_page(wiki_name, &path_str, false);
        }

        // Virtual Home Overview
        let mut overview = format!(
            "# {}\n\n*Knowledge base overview*\n\n## Pages\n\n",
            wiki.name
        );
        let files = terminalwiki_core::scan::scan(wiki, &self.config.index);
        for file in files {
            if file.content_type.is_page() {
                let path_str = file.relative.to_string_lossy();
                overview.push_str(&format!("- [[{}]]\n", path_str));
            }
        }

        let doc: RenderedDocument =
            render_markdown(&overview, self.config, &self.theme, self.color_mode);
        self.current_wiki = wiki_name.to_string();
        self.current_path = PathBuf::from("index.md");
        self.current_title = wiki.name.clone();
        self.lines = doc.lines;
        self.headings = doc.headings;
        self.links = doc.links;
        self.raw_content = overview;
        self.scroll = 0;
        self.h_scroll = 0;
        self.selected_link_idx = if !self.links.is_empty() {
            Some(0)
        } else {
            None
        };

        Ok(())
    }

    pub fn load_page(
        &mut self,
        wiki_name: &str,
        page_str: &str,
        record_history: bool,
    ) -> Result<()> {
        let resolution = resolve::resolve(self.wikis, wiki_name, page_str, &self.config.index)?;

        if record_history && !self.current_path.as_os_str().is_empty() {
            self.history_back.push((
                self.current_wiki.clone(),
                self.current_path.clone(),
                self.scroll,
                self.h_scroll,
            ));
            self.history_forward.clear();
        }

        let bytes = fs::read(&resolution.path).map_err(|e| Error::io(&resolution.path, e))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();

        let doc: RenderedDocument = render_path(
            &resolution.path,
            &bytes,
            self.config,
            &self.theme,
            self.color_mode,
        );

        self.current_wiki = resolution.wiki.clone();
        self.current_path = resolution.relative.clone();
        self.current_title = resolution
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| page_str.to_string());
        self.lines = doc.lines;
        self.headings = doc.headings;
        self.links = doc.links;
        self.raw_content = text;
        self.scroll = 0;
        self.h_scroll = 0;
        self.selected_link_idx = if !self.links.is_empty() {
            Some(0)
        } else {
            None
        };
        self.status_message = None;

        Ok(())
    }

    pub fn go_back(&mut self) {
        if let Some((wiki, path, scroll, h_scroll)) = self.history_back.pop() {
            self.history_forward.push((
                self.current_wiki.clone(),
                self.current_path.clone(),
                self.scroll,
                self.h_scroll,
            ));
            let path_str = path.to_string_lossy().into_owned();
            if self.load_page(&wiki, &path_str, false).is_ok() {
                self.scroll = scroll;
                self.h_scroll = h_scroll;
            }
        } else {
            self.status_message = Some("Already at oldest history entry".to_string());
        }
    }

    pub fn go_forward(&mut self) {
        if let Some((wiki, path, scroll, h_scroll)) = self.history_forward.pop() {
            self.history_back.push((
                self.current_wiki.clone(),
                self.current_path.clone(),
                self.scroll,
                self.h_scroll,
            ));
            let path_str = path.to_string_lossy().into_owned();
            if self.load_page(&wiki, &path_str, false).is_ok() {
                self.scroll = scroll;
                self.h_scroll = h_scroll;
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

    pub fn scroll_left(&mut self, amount: usize) {
        self.h_scroll = self.h_scroll.saturating_sub(amount);
    }

    pub fn scroll_right(&mut self, amount: usize) {
        self.h_scroll = self.h_scroll.saturating_add(amount);
    }

    pub fn update_finder_filter(&mut self) {
        self.finder_filtered = self.fuzzy_dataset.find(&self.finder_query, 30);
        self.finder_selected = 0;
    }

    pub fn update_command_suggestions(&mut self) {
        let trimmed = self.command_input.trim();
        if trimmed.is_empty() {
            self.command_suggestions = COMMAND_LIST.iter().map(|s| s.to_string()).collect();
        } else {
            self.command_suggestions = COMMAND_LIST
                .iter()
                .filter(|s| s.starts_with(trimmed))
                .map(|s| s.to_string())
                .collect();
        }
        self.command_selected = 0;
    }

    pub fn execute_command(&mut self) {
        let cmd = self.command_input.trim().to_string();
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "q" | "quit" => self.should_quit = true,
            "open" => {
                if parts.len() > 1 {
                    let page = parts[1..].join(" ");
                    let _ = self.load_page(&self.current_wiki.clone(), &page, true);
                }
            }
            "search" | "find" => {
                if parts.len() > 1 {
                    self.mode = Mode::Finder;
                    self.finder_query = parts[1..].join(" ");
                    self.update_finder_filter();
                } else {
                    self.mode = Mode::Finder;
                    self.finder_query.clear();
                    self.update_finder_filter();
                }
            }
            "backlinks" => {
                self.load_backlinks();
                self.mode = Mode::Backlinks;
            }
            "outline" => {
                if !self.headings.is_empty() {
                    self.mode = Mode::Outline;
                    self.outline_selected = 0;
                } else {
                    self.status_message = Some("No headings in current document".to_string());
                }
            }
            "edit" => self.open_current_in_editor(),
            "reload" => {
                let p = self.current_path.to_string_lossy().into_owned();
                let _ = self.load_page(&self.current_wiki.clone(), &p, false);
                self.status_message = Some("Page reloaded".to_string());
            }
            "wiki" => {
                if parts.len() > 1 {
                    let target_wiki = parts[1];
                    if self.wikis.get(target_wiki).is_some() {
                        let _ = self.load_home(target_wiki);
                    } else {
                        self.status_message = Some(format!("Unknown wiki: {target_wiki}"));
                    }
                }
            }
            _ => {
                self.status_message = Some(format!("Unknown command: {}", parts[0]));
            }
        }
    }

    pub fn load_backlinks(&mut self) {
        let mut entries = Vec::new();
        for wiki in self.wikis.iter() {
            if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
                for e in idx.entries {
                    entries.push(GraphEntry {
                        wiki: e.wiki,
                        relative: e.relative,
                        content_type: terminalwiki_core::filetype::ContentType::from(
                            e.content_type,
                        )
                        .as_str()
                        .to_string(),
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
