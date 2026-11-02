//! TUI Application state and business logic (spec §40-§53).

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use terminalwiki_core::resolve;
use terminalwiki_core::watch::{ChangeKind, WikiChange};
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_graph::{BacklinkInfo, GraphEntry, LayoutEngine, SubGraph, WikiGraph};
use terminalwiki_index::{FuzzyDataset, FuzzyHit, FuzzyItem};
use terminalwiki_render::{
    detect_color_mode, render_markdown, render_path, ColorMode, LinkTarget, RenderedDocument,
    RenderedHeading, RenderedLine, RenderedLink, Theme,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Finder,
    Outline,
    InPageSearch,
    WikiSearch,
    Backlinks,
    Command,
    Help,
    Graph,
    Sidebar,
    NewPage,
    TagPrompt,
}

/// Reading layout mode for the TUI viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewLayout {
    /// Default clean reader layout with optional sidebar.
    Standard,
    /// Distraction-free centered reading view (no sidebars, comfortable margins).
    Zen,
    /// Multi-pane Wikipedia-style layout (Vault explorer + Article + Inspector with TOC/Backlinks).
    Wikipedia,
}

impl ViewLayout {
    pub fn label(self) -> &'static str {
        match self {
            ViewLayout::Standard => "Standard",
            ViewLayout::Zen => "Zen Reader",
            ViewLayout::Wikipedia => "Wikipedia",
        }
    }
}

/// Link filtering mode for keyboard navigation (Tab / Shift+Tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkFilter {
    #[default]
    All,
    InternalOnly,
}

/// Organization mode for the left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    /// Flat list of pages.
    Flat,
    /// Hierarchical vault folder tree structure.
    Tree,
    /// Document outline (chapters and headings).
    Outline,
    /// Tag explorer.
    Tags,
    /// Bookmarked / starred pages.
    Bookmarks,
}

/// An entry displayed in the sidebar according to the active `SidebarMode`.
#[derive(Debug, Clone)]
pub enum SidebarEntry {
    Page {
        wiki: String,
        relative: PathBuf,
        title: String,
    },
    Heading {
        title: String,
        line: usize,
        level: usize,
    },
    Tag {
        tag: String,
        count: usize,
    },
}

/// Starting content for a page created from inside the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTemplate {
    Blank,
    Todo,
    QuickNote,
}

impl PageTemplate {
    pub fn label(self) -> &'static str {
        match self {
            PageTemplate::Blank => "Blank",
            PageTemplate::Todo => "Todo list",
            PageTemplate::QuickNote => "Quick note (opens $EDITOR)",
        }
    }

    fn toggle(self) -> Self {
        match self {
            PageTemplate::Blank => PageTemplate::Todo,
            PageTemplate::Todo => PageTemplate::QuickNote,
            PageTemplate::QuickNote => PageTemplate::Blank,
        }
    }
}

/// A command entry in the VS Code-style `Ctrl+P` `>` command palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickCommandHit {
    pub id: &'static str,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub description: &'static str,
}

/// A full-text match across the wiki (`Ctrl+W` WikiSearch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiSearchHit {
    pub wiki: String,
    pub relative: PathBuf,
    pub title: String,
    pub line_number: usize,
    pub snippet: String,
    pub score: i64,
}

/// Default neighbourhood depth for the graph view (spec item 42).
pub const GRAPH_DEFAULT_DEPTH: usize = 2;
/// Hard ceiling on nodes fed to the layout engine.
///
/// The layout is O(n²) below its 200-node threshold, and a canvas of ordinary
/// terminal size cannot legibly hold more than a few dozen labels anyway.
pub const GRAPH_MAX_NODES: usize = 120;

/// State of the interactive graph view.
///
/// The layout is computed once when the view opens or its depth changes; the
/// canvas is re-rendered per frame from the cached positions, so a terminal
/// resize costs a redraw rather than a re-layout.
pub struct GraphViewState {
    pub root_wiki: String,
    pub root_path: PathBuf,
    pub depth: usize,
    pub sub: SubGraph,
    pub pos: HashMap<usize, (f64, f64)>,
    /// Selection domain: subgraph node indices ordered top-to-bottom,
    /// left-to-right by layout position, so `j`/`k` move spatially.
    pub ordered_nodes: Vec<usize>,
    pub selected: usize,
    /// Nodes in the neighbourhood before the cap was applied.
    pub capped: bool,
    pub is_global: bool,
}

impl GraphViewState {
    /// The currently selected graph node index, if any.
    pub fn selected_node(&self) -> Option<usize> {
        self.ordered_nodes.get(self.selected).copied()
    }
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
    pub link_filter: LinkFilter,
    pub bookmarks: std::collections::HashSet<PathBuf>,

    pub mode: Mode,
    pub layout_mode: ViewLayout,

    // Sidebar page explorer
    pub sidebar_visible: bool,
    pub sidebar_selected: usize,
    pub sidebar_mode: SidebarMode,

    // New page prompt
    pub new_page_name: String,
    pub new_page_template: PageTemplate,

    // Tag prompt ('t')
    pub tag_input: String,

    // Finder / VS Code Quick Bar (Ctrl+P, '>' for commands)
    pub finder_query: String,
    pub fuzzy_dataset: FuzzyDataset,
    pub finder_filtered: Vec<FuzzyHit>,
    pub finder_commands: Vec<QuickCommandHit>,
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

    // In-page search (Ctrl+F / '/')
    pub in_page_query: String,
    pub search_matches: Vec<usize>,
    pub search_match_idx: usize,

    // Wiki-wide full-text search (Ctrl+W)
    pub wiki_search_query: String,
    pub wiki_search_hits: Vec<WikiSearchHit>,
    pub wiki_search_selected: usize,

    /// Whole-wiki link graph, built lazily and reused.
    ///
    /// Backlinks and the graph view both need it; rebuilding per keypress meant
    /// walking every entry of every wiki on each `b` press.
    pub graph_cache: Option<WikiGraph>,
    pub graph_view: Option<GraphViewState>,
    /// The open page was deleted on disk; the view shows a notice instead of
    /// stale content.
    pub page_missing: bool,

    pub status_message: Option<String>,
    pub should_quit: bool,
    pub should_suspend_for_editor: Option<PathBuf>,
}

const COMMAND_LIST: &[&str] = &[
    "open",
    "search",
    "wsearch",
    "find",
    "backlinks",
    "outline",
    "graph",
    "edit",
    "new",
    "quicknote",
    "qn",
    "todo",
    "tag",
    "sidebar",
    "tree",
    "daily",
    "allgraph",
    "bookmark",
    "bookmarks",
    "linkfilter",
    "todos",
    "zen",
    "wikipedia",
    "layout",
    "latex",
    "compile",
    "wiki",
    "reload",
    "quit",
];

pub const QUICK_COMMANDS: &[QuickCommandHit] = &[
    QuickCommandHit {
        id: "new",
        title: "New Page",
        shortcut: "n",
        description: "Create a new Markdown page in the current wiki",
    },
    QuickCommandHit {
        id: "quicknote",
        title: "Quick Note (Open in Vim/Nvim)",
        shortcut: "Ctrl+N / Q",
        description: "Create a note in QuickNotes/ and immediately open $EDITOR",
    },
    QuickCommandHit {
        id: "todo",
        title: "New Todo List Page",
        shortcut: "T",
        description: "Create a new page pre-filled with a task list and #todo tag",
    },
    QuickCommandHit {
        id: "edit",
        title: "Edit Current Page in $EDITOR",
        shortcut: "e",
        description: "Open the current page in nvim/vim and reload on exit",
    },
    QuickCommandHit {
        id: "wsearch",
        title: "Wiki-Wide Full-Text Search",
        shortcut: "Ctrl+W",
        description: "Search across all pages for words or sentence parts",
    },
    QuickCommandHit {
        id: "inpage",
        title: "Find in Current Page",
        shortcut: "Ctrl+F / /",
        description: "Search and highlight words or phrases in the open page",
    },
    QuickCommandHit {
        id: "graph",
        title: "Open Knowledge Graph",
        shortcut: "g",
        description: "Interactive Obsidian-style link graph around current page",
    },
    QuickCommandHit {
        id: "zen",
        title: "Toggle Zen Reader View",
        shortcut: "z",
        description: "Distraction-free centered reading layout",
    },
    QuickCommandHit {
        id: "wikipedia",
        title: "Toggle Wikipedia Multi-Pane Layout",
        shortcut: "w",
        description: "Show Vault Explorer + Article + TOC/Backlinks Inspector",
    },
    QuickCommandHit {
        id: "sidebar",
        title: "Toggle Sidebar Explorer",
        shortcut: "s",
        description: "Show or hide the left page explorer sidebar",
    },
    QuickCommandHit {
        id: "tree",
        title: "Toggle Vault Folder Tree / Flat List",
        shortcut: "v",
        description: "Switch sidebar between folder tree hierarchy and flat list",
    },
    QuickCommandHit {
        id: "tag",
        title: "Add Tag to Current Page",
        shortcut: "t",
        description: "Append a #tag to the open page and re-index",
    },
    QuickCommandHit {
        id: "latex",
        title: "Compile LaTeX Page (pdflatex/xelatex/tectonic/latexmk)",
        shortcut: ":latex",
        description: "Compile the current .tex page using an installed LaTeX engine",
    },
    QuickCommandHit {
        id: "outline",
        title: "Document Outline (Headings)",
        shortcut: "o",
        description: "Jump to a section heading in the current page",
    },
    QuickCommandHit {
        id: "backlinks",
        title: "Show Backlinks",
        shortcut: "b",
        description: "List all pages linking to the current page",
    },
    QuickCommandHit {
        id: "daily",
        title: "Daily Note (Today)",
        shortcut: "d / Ctrl+D",
        description: "Create or open today's daily note in Daily/YYYY-MM-DD.md",
    },
    QuickCommandHit {
        id: "allgraph",
        title: "Global Knowledge Graph (All Notes)",
        shortcut: "a (in graph)",
        description: "Show the complete vault graph with all notes and connections",
    },
    QuickCommandHit {
        id: "bookmark",
        title: "Toggle Bookmark (Star Page)",
        shortcut: "m",
        description: "Add or remove the current page from bookmarks list",
    },
    QuickCommandHit {
        id: "linkfilter",
        title: "Toggle Link Filter (All vs Internal Wiki Links)",
        shortcut: "L",
        description: "Filter Tab navigation to internal links only or include external URLs",
    },
    QuickCommandHit {
        id: "todos",
        title: "Toggle Todo Checkbox on Current Line",
        shortcut: "x / Space",
        description: "Toggle task state between - [ ] and - [x] directly on disk",
    },
    QuickCommandHit {
        id: "reload",
        title: "Reload Current Page",
        shortcut: ":reload",
        description: "Re-read and re-render the current file from disk",
    },
    QuickCommandHit {
        id: "quit",
        title: "Quit TerminalWiki",
        shortcut: "q",
        description: "Exit the interactive TUI",
    },
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
                    if e.relative.file_name()
                        == Some(std::ffi::OsStr::new(
                            terminalwiki_core::config::WIKI_CONFIG_FILE,
                        ))
                    {
                        continue;
                    }
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
            link_filter: LinkFilter::default(),
            bookmarks: std::collections::HashSet::new(),
            mode: Mode::Normal,
            layout_mode: ViewLayout::Standard,
            sidebar_visible: false,
            sidebar_selected: 0,
            sidebar_mode: SidebarMode::Flat,
            new_page_name: String::new(),
            new_page_template: PageTemplate::Blank,
            tag_input: String::new(),
            finder_query: String::new(),
            fuzzy_dataset: dataset,
            finder_filtered: initial_hits,
            finder_commands: QUICK_COMMANDS.to_vec(),
            finder_selected: 0,
            outline_selected: 0,
            backlinks: Vec::new(),
            backlinks_selected: 0,
            command_input: String::new(),
            command_suggestions: Vec::new(),
            command_selected: 0,
            in_page_query: String::new(),
            search_matches: Vec::new(),
            search_match_idx: 0,
            wiki_search_query: String::new(),
            wiki_search_hits: Vec::new(),
            wiki_search_selected: 0,
            graph_cache: None,
            graph_view: None,
            page_missing: false,
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
        // Resolve the configured home page to a wiki-relative path: link and
        // page resolution deliberately reject absolute paths.
        let home = {
            let wiki = self.wikis.require(wiki_name)?;
            wiki.home_page()
                .map(|path| wiki.relative(&path).to_path_buf())
        };

        if let Some(home_path) = home {
            let path_str = home_path.to_string_lossy().into_owned();
            return self.load_page(wiki_name, &path_str, false);
        }

        let wiki = self.wikis.require(wiki_name)?;
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
        let (resolution_path, resolution_relative, resolution_wiki) =
            if let Some(wiki) = self.wikis.get(wiki_name) {
                let direct = wiki.root.join(page_str);
                if direct.is_file() {
                    let rel = direct
                        .strip_prefix(&wiki.root)
                        .unwrap_or(Path::new(page_str))
                        .to_path_buf();
                    (direct, rel, wiki_name.to_string())
                } else {
                    let r = resolve::resolve(self.wikis, wiki_name, page_str, &self.config.index)?;
                    (r.path, r.relative, r.wiki)
                }
            } else {
                let r = resolve::resolve(self.wikis, wiki_name, page_str, &self.config.index)?;
                (r.path, r.relative, r.wiki)
            };

        if record_history && !self.current_path.as_os_str().is_empty() {
            self.history_back.push((
                self.current_wiki.clone(),
                self.current_path.clone(),
                self.scroll,
                self.h_scroll,
            ));
            self.history_forward.clear();
        }

        let bytes = fs::read(&resolution_path).map_err(|e| Error::io(&resolution_path, e))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();

        let doc: RenderedDocument = render_path(
            &resolution_path,
            &bytes,
            self.config,
            &self.theme,
            self.color_mode,
        );

        self.current_wiki = resolution_wiki;
        self.current_path = resolution_relative;
        self.current_title = resolution_path
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

    pub fn finder_is_command_mode(&self) -> bool {
        self.finder_query.trim_start().starts_with('>')
    }

    pub fn update_finder_filter(&mut self) {
        if self.finder_is_command_mode() {
            let raw = self.finder_query.trim_start();
            let after_gt = raw.strip_prefix('>').unwrap_or(raw).trim();
            let cmd_token = after_gt
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();
            if cmd_token.is_empty() {
                self.finder_commands = QUICK_COMMANDS.to_vec();
            } else {
                self.finder_commands = QUICK_COMMANDS
                    .iter()
                    .filter(|c| {
                        c.id.to_lowercase().contains(&cmd_token)
                            || c.title.to_lowercase().contains(&cmd_token)
                            || c.description.to_lowercase().contains(&cmd_token)
                    })
                    .cloned()
                    .collect();
            }
            self.finder_selected = 0;
        } else {
            let (text, tags) = split_finder_query(&self.finder_query);
            self.finder_filtered = self.fuzzy_dataset.find_with_tags(&text, &tags, 50);
            self.finder_selected = 0;
        }
    }

    pub fn execute_finder_selection(&mut self) {
        if self.finder_is_command_mode() {
            let raw = self.finder_query.trim_start().to_string();
            let after_gt = raw.strip_prefix('>').unwrap_or(&raw).trim();
            let arg_tail: Option<String> = after_gt
                .split_once(char::is_whitespace)
                .map(|(_, rest)| rest.trim().to_string())
                .filter(|s| !s.is_empty());

            if let Some(cmd) = self.finder_commands.get(self.finder_selected).cloned() {
                self.mode = Mode::Normal;
                match cmd.id {
                    "new" => {
                        if let Some(name) = arg_tail {
                            if let Err(err) = self.create_page(&name, PageTemplate::Blank) {
                                self.status_message = Some(err.to_string());
                            }
                        } else {
                            self.begin_new_page(PageTemplate::Blank);
                        }
                    }
                    "quicknote" => {
                        if let Err(err) = self.create_quick_note(arg_tail.as_deref(), true) {
                            self.status_message = Some(err.to_string());
                        }
                    }
                    "todo" => {
                        if let Some(name) = arg_tail {
                            if let Err(err) = self.create_page(&name, PageTemplate::Todo) {
                                self.status_message = Some(err.to_string());
                            }
                        } else {
                            self.begin_new_page(PageTemplate::Todo);
                        }
                    }
                    "edit" => self.open_current_in_editor(),
                    "wsearch" => {
                        self.mode = Mode::WikiSearch;
                        self.wiki_search_query = arg_tail.unwrap_or_default();
                        self.update_wiki_search();
                    }
                    "inpage" => {
                        self.mode = Mode::InPageSearch;
                        self.in_page_query = arg_tail.unwrap_or_default();
                        self.update_in_page_search();
                    }
                    "graph" => self.open_graph(),
                    "zen" => self.toggle_zen_mode(),
                    "wikipedia" => self.toggle_wikipedia_layout(),
                    "sidebar" => self.toggle_sidebar(),
                    "tree" => self.toggle_sidebar_mode(),
                    "tag" => {
                        if let Some(tag) = arg_tail {
                            if let Err(err) = self.add_tag_to_current_page(&tag) {
                                self.status_message = Some(err.to_string());
                            }
                        } else {
                            self.mode = Mode::TagPrompt;
                            self.tag_input.clear();
                        }
                    }
                    "outline" => {
                        if !self.headings.is_empty() {
                            self.mode = Mode::Outline;
                            self.outline_selected = 0;
                        } else {
                            self.status_message =
                                Some("No headings in current document".to_string());
                        }
                    }
                    "backlinks" => {
                        self.load_backlinks();
                        self.mode = Mode::Backlinks;
                    }
                    "latex" => self.compile_current_latex(arg_tail.as_deref()),
                    "daily" => {
                        let _ = self.create_daily_note();
                    }
                    "allgraph" => self.open_global_graph(),
                    "bookmark" => self.toggle_bookmark(),
                    "bookmarks" => self.set_sidebar_mode(SidebarMode::Bookmarks),
                    "linkfilter" => self.toggle_link_filter(),
                    "todos" => {
                        let _ = self.toggle_todo_on_current_line();
                    }
                    "reload" => self.reload_current_page(),
                    "quit" => self.should_quit = true,
                    _ => {}
                }
            } else if !after_gt.is_empty() {
                // Direct command execution fallback
                self.mode = Mode::Normal;
                self.command_input = after_gt.to_string();
                self.execute_command();
            }
        } else if let Some(hit) = self.finder_filtered.get(self.finder_selected).cloned() {
            self.mode = Mode::Normal;
            let path_str = hit.relative.to_string_lossy().into_owned();
            let _ = self.load_page(&hit.wiki, &path_str, true);
        }
    }

    /// Toggles distraction-free Zen Reader layout.
    pub fn toggle_zen_mode(&mut self) {
        self.layout_mode = if self.layout_mode == ViewLayout::Zen {
            ViewLayout::Standard
        } else {
            ViewLayout::Zen
        };
        self.status_message = Some(format!("Layout: {}", self.layout_mode.label()));
    }

    /// Toggles the multi-pane Wikipedia layout (Vault Tree + Article + TOC/Backlinks).
    pub fn toggle_wikipedia_layout(&mut self) {
        self.layout_mode = if self.layout_mode == ViewLayout::Wikipedia {
            ViewLayout::Standard
        } else {
            self.load_backlinks();
            ViewLayout::Wikipedia
        };
        self.status_message = Some(format!("Layout: {}", self.layout_mode.label()));
    }

    /// Switches the sidebar mode between Flat, Tree, Outline, Tags, and Bookmarks.
    pub fn toggle_sidebar_mode(&mut self) {
        self.sidebar_mode = match self.sidebar_mode {
            SidebarMode::Flat => SidebarMode::Tree,
            SidebarMode::Tree => SidebarMode::Outline,
            SidebarMode::Outline => SidebarMode::Tags,
            SidebarMode::Tags => SidebarMode::Bookmarks,
            SidebarMode::Bookmarks => SidebarMode::Flat,
        };
        self.sidebar_selected = 0;
        if !self.sidebar_visible {
            self.sidebar_visible = true;
        }
        let label = match self.sidebar_mode {
            SidebarMode::Flat => "Sidebar: Pages List",
            SidebarMode::Tree => "Sidebar: Vault Folder Tree",
            SidebarMode::Outline => "Sidebar: Document Outline",
            SidebarMode::Tags => "Sidebar: Tags Explorer",
            SidebarMode::Bookmarks => "Sidebar: Bookmarks",
        };
        self.status_message = Some(label.to_string());
    }

    /// Sets the sidebar mode directly.
    pub fn set_sidebar_mode(&mut self, mode: SidebarMode) {
        self.sidebar_mode = mode;
        self.sidebar_selected = 0;
        self.sidebar_visible = true;
        let label = match self.sidebar_mode {
            SidebarMode::Flat => "Sidebar: Pages List",
            SidebarMode::Tree => "Sidebar: Vault Folder Tree",
            SidebarMode::Outline => "Sidebar: Document Outline",
            SidebarMode::Tags => "Sidebar: Tags Explorer",
            SidebarMode::Bookmarks => "Sidebar: Bookmarks",
        };
        self.status_message = Some(label.to_string());
    }

    /// Returns the entries for the sidebar given the current `sidebar_mode`.
    pub fn sidebar_entries(&self) -> Vec<SidebarEntry> {
        match self.sidebar_mode {
            SidebarMode::Flat => self
                .fuzzy_dataset
                .items()
                .iter()
                .map(|item| SidebarEntry::Page {
                    wiki: item.wiki.clone(),
                    relative: item.relative.clone(),
                    title: if item.title.is_empty() {
                        item.relative.to_string_lossy().into_owned()
                    } else {
                        item.title.clone()
                    },
                })
                .collect(),
            SidebarMode::Tree => {
                let mut items: Vec<_> = self.fuzzy_dataset.items().to_vec();
                items.sort_by(|a, b| a.relative.cmp(&b.relative));
                items
                    .into_iter()
                    .map(|item| SidebarEntry::Page {
                        wiki: item.wiki,
                        relative: item.relative,
                        title: item.title,
                    })
                    .collect()
            }
            SidebarMode::Outline => self
                .headings
                .iter()
                .map(|h| SidebarEntry::Heading {
                    title: h.text.clone(),
                    line: h.line,
                    level: h.level,
                })
                .collect(),
            SidebarMode::Tags => {
                let mut tag_counts: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for item in self.fuzzy_dataset.items() {
                    for tag in &item.tags {
                        *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                    }
                }
                tag_counts
                    .into_iter()
                    .map(|(tag, count)| SidebarEntry::Tag { tag, count })
                    .collect()
            }
            SidebarMode::Bookmarks => self
                .bookmarks
                .iter()
                .map(|p| {
                    let title = p
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.to_string_lossy().into_owned());
                    SidebarEntry::Page {
                        wiki: self.current_wiki.clone(),
                        relative: p.clone(),
                        title,
                    }
                })
                .collect(),
        }
    }

    /// Toggles bookmarking / starring for the currently open page.
    pub fn toggle_bookmark(&mut self) {
        if self.current_path.as_os_str().is_empty() {
            return;
        }
        if self.bookmarks.contains(&self.current_path) {
            self.bookmarks.remove(&self.current_path);
            self.status_message = Some(format!("Removed bookmark: {}", self.current_title));
        } else {
            self.bookmarks.insert(self.current_path.clone());
            self.status_message = Some(format!("Bookmarked: ★ {}", self.current_title));
        }
    }

    /// Toggles the link filter mode (All vs Internal Wiki links).
    pub fn toggle_link_filter(&mut self) {
        self.link_filter = match self.link_filter {
            LinkFilter::All => LinkFilter::InternalOnly,
            LinkFilter::InternalOnly => LinkFilter::All,
        };
        let label = match self.link_filter {
            LinkFilter::All => "Link filter: All links (Internal + External)",
            LinkFilter::InternalOnly => "Link filter: Internal only (Wiki notes)",
        };
        self.status_message = Some(label.to_string());
    }

    /// Cycles through links on the page respecting `link_filter`.
    pub fn cycle_links(&mut self, forward: bool, view_h: usize) {
        if self.links.is_empty() {
            self.status_message = Some("No links on this page".to_string());
            return;
        }

        let eligible: Vec<usize> = self
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| match self.link_filter {
                LinkFilter::All => true,
                LinkFilter::InternalOnly => !matches!(l.target, LinkTarget::External(_)),
            })
            .map(|(i, _)| i)
            .collect();

        if eligible.is_empty() {
            self.status_message = Some(match self.link_filter {
                LinkFilter::InternalOnly => {
                    "No internal wiki links on this page (press 'L' to include external links)"
                        .to_string()
                }
                LinkFilter::All => "No links on this page".to_string(),
            });
            return;
        }

        let next_idx = match self.selected_link_idx {
            None => eligible[0],
            Some(cur) => {
                if let Some(pos) = eligible.iter().position(|&i| i == cur) {
                    if forward {
                        eligible[(pos + 1) % eligible.len()]
                    } else if pos == 0 {
                        *eligible.last().unwrap()
                    } else {
                        eligible[pos - 1]
                    }
                } else if forward {
                    eligible
                        .iter()
                        .copied()
                        .find(|&i| i > cur)
                        .unwrap_or(eligible[0])
                } else {
                    eligible
                        .iter()
                        .copied()
                        .rfind(|&i| i < cur)
                        .unwrap_or(*eligible.last().unwrap())
                }
            }
        };

        self.selected_link_idx = Some(next_idx);
        let link = &self.links[next_idx];
        self.scroll = link.line.saturating_sub(view_h / 2);
        let filter_tag = match self.link_filter {
            LinkFilter::All => "",
            LinkFilter::InternalOnly => " [internal only]",
        };
        self.status_message = Some(format!(
            "Link [{}/{}]{filter_tag}: {} (Enter to follow, L toggle filter)",
            next_idx + 1,
            self.links.len(),
            link.label
        ));
    }

    /// Returns today's civil date string formatted as `YYYY-MM-DD`.
    pub fn current_date_string() -> String {
        let secs = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        };
        let mut days = (secs / 86400) as i64;
        days += 719468;
        let era = if days >= 0 { days } else { days - 146096 } / 146097;
        let doe = (days - era * 146097) as u64;
        let yoe = (doe - doe / 1024 + doe / 1461 - doe / 142401) / 365;
        let y = (yoe as i64) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}-{m:02}-{d:02}")
    }

    /// Creates or opens today's daily note in `Daily/YYYY-MM-DD.md`.
    pub fn create_daily_note(&mut self) -> Result<PathBuf> {
        let date_str = Self::current_date_string();
        let rel_path = PathBuf::from("Daily").join(format!("{date_str}.md"));
        let Some(wiki) = self.wikis.get(&self.current_wiki) else {
            return Err(Error::other(format!(
                "Wiki not found: {}",
                self.current_wiki
            )));
        };
        let target_path = wiki.root.join(&rel_path);
        let exists = target_path.exists();
        if !exists {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            let initial_content = format!(
                "# {}\n\n#daily #todo\n\n## Tasks\n- [ ] \n\n## Notes\n- \n",
                date_str
            );
            fs::write(&target_path, initial_content).map_err(|e| Error::io(&target_path, e))?;
            let _ = terminalwiki_index::WikiIndex::open(wiki, self.config);
            self.rebuild_fuzzy_dataset();
            self.invalidate_graph();
        }

        let rel_str = rel_path.to_string_lossy().into_owned();
        self.load_page(&self.current_wiki.clone(), &rel_str, true)?;
        self.mode = Mode::Normal;
        self.status_message = Some(if exists {
            format!("Opened daily note for {date_str}")
        } else {
            format!("Created daily note for {date_str}")
        });
        Ok(target_path)
    }

    /// Toggles a markdown todo checkbox `- [ ]` <-> `- [x]` on the current/nearest line.
    pub fn toggle_todo_on_current_line(&mut self) -> Result<bool> {
        if self.current_path.as_os_str().is_empty() {
            return Ok(false);
        }
        let Some(wiki) = self.wikis.get(&self.current_wiki) else {
            return Err(Error::other(format!(
                "Wiki not found: {}",
                self.current_wiki
            )));
        };
        let file_path = wiki.root.join(&self.current_path);
        if !file_path.is_file() {
            return Ok(false);
        }

        let content = fs::read_to_string(&file_path).map_err(|e| Error::io(&file_path, e))?;
        let mut raw_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        if raw_lines.is_empty() {
            return Ok(false);
        }

        // Choose the line closest to scroll position
        let target_idx = self.scroll.min(raw_lines.len().saturating_sub(1));
        let mut best_line_idx = None;
        for delta in 0..=5 {
            let idx1 = target_idx + delta;
            if idx1 < raw_lines.len() {
                let l = &raw_lines[idx1];
                if l.contains("- [ ]")
                    || l.contains("- [x]")
                    || l.contains("- [X]")
                    || l.contains("* [ ]")
                    || l.contains("* [x]")
                    || l.contains("* [X]")
                    || l.trim_start().starts_with("- ")
                    || l.trim_start().starts_with("* ")
                {
                    best_line_idx = Some(idx1);
                    break;
                }
            }
            if delta > 0 && target_idx >= delta {
                let idx2 = target_idx - delta;
                let l = &raw_lines[idx2];
                if l.contains("- [ ]")
                    || l.contains("- [x]")
                    || l.contains("- [X]")
                    || l.contains("* [ ]")
                    || l.contains("* [x]")
                    || l.contains("* [X]")
                    || l.trim_start().starts_with("- ")
                    || l.trim_start().starts_with("* ")
                {
                    best_line_idx = Some(idx2);
                    break;
                }
            }
        }

        let line_idx = best_line_idx.unwrap_or(target_idx);
        let orig_line = &raw_lines[line_idx];
        let toggled = if orig_line.contains("- [ ]") {
            orig_line.replacen("- [ ]", "- [x]", 1)
        } else if orig_line.contains("- [x]") {
            orig_line.replacen("- [x]", "- [ ]", 1)
        } else if orig_line.contains("- [X]") {
            orig_line.replacen("- [X]", "- [ ]", 1)
        } else if orig_line.contains("* [ ]") {
            orig_line.replacen("* [ ]", "* [x]", 1)
        } else if orig_line.contains("* [x]") {
            orig_line.replacen("* [x]", "* [ ]", 1)
        } else if orig_line.contains("* [X]") {
            orig_line.replacen("* [X]", "* [ ]", 1)
        } else if let Some(stripped) = orig_line.strip_prefix("- ") {
            format!("- [x] {stripped}")
        } else if let Some(stripped) = orig_line.strip_prefix("* ") {
            format!("* [x] {stripped}")
        } else {
            format!("- [ ] {orig_line}")
        };

        let is_completed = toggled.contains("- [x]") || toggled.contains("* [x]");
        raw_lines[line_idx] = toggled;
        let mut new_content = raw_lines.join("\n");
        if content.ends_with('\n') {
            new_content.push('\n');
        }

        fs::write(&file_path, new_content).map_err(|e| Error::io(&file_path, e))?;

        let cur_scroll = self.scroll;
        let rel_str = self.current_path.to_string_lossy().into_owned();
        let _ = self.load_page(&self.current_wiki.clone(), &rel_str, false);
        self.scroll = cur_scroll;

        self.status_message = Some(if is_completed {
            "✓ Todo marked as completed [x]".to_string()
        } else {
            "○ Todo marked as incomplete [ ]".to_string()
        });

        Ok(true)
    }

    /// Recomputes live in-page search matches (`Ctrl+F` / `/`) and jumps to the first match.
    pub fn update_in_page_search(&mut self) {
        self.search_matches.clear();
        self.search_match_idx = 0;
        let q = self.in_page_query.trim().to_lowercase();
        if q.is_empty() {
            return;
        }
        for (i, line) in self.lines.iter().enumerate() {
            let line_str: String = line.iter().map(|s| s.text.as_str()).collect();
            if line_str.to_lowercase().contains(&q) {
                self.search_matches.push(i);
            }
        }
        if let Some(&first_line) = self.search_matches.first() {
            self.scroll = first_line.saturating_sub(3);
        }
    }

    /// Jumps to the next in-page search match.
    pub fn next_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = (self.search_match_idx + 1) % self.search_matches.len();
        if let Some(&line) = self.search_matches.get(self.search_match_idx) {
            self.scroll = line.saturating_sub(3);
            self.status_message = Some(format!(
                "Match {}/{} (line {})",
                self.search_match_idx + 1,
                self.search_matches.len(),
                line + 1
            ));
        }
    }

    /// Jumps to the previous in-page search match.
    pub fn prev_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = if self.search_match_idx == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_match_idx - 1
        };
        if let Some(&line) = self.search_matches.get(self.search_match_idx) {
            self.scroll = line.saturating_sub(3);
            self.status_message = Some(format!(
                "Match {}/{} (line {})",
                self.search_match_idx + 1,
                self.search_matches.len(),
                line + 1
            ));
        }
    }

    /// Performs wiki-wide full-text search across words and sentence parts (`Ctrl+W`).
    pub fn update_wiki_search(&mut self) {
        self.wiki_search_hits.clear();
        self.wiki_search_selected = 0;
        let query = self.wiki_search_query.trim();
        if query.is_empty() {
            return;
        }
        let q_lower = query.to_lowercase();
        let tokens: Vec<&str> = q_lower.split_whitespace().collect();
        if tokens.is_empty() {
            return;
        }

        let mut hits = Vec::new();
        for item in self.fuzzy_dataset.items() {
            let Some(wiki) = self.wikis.get(&item.wiki) else {
                continue;
            };
            let abs_path = wiki.root.join(&item.relative);
            let Ok(content) = fs::read_to_string(&abs_path) else {
                continue;
            };
            let fm = terminalwiki_core::frontmatter::Frontmatter::parse(&content);
            let body = content.get(fm.body_offset..).unwrap_or(&content);
            let title_lower = item.title.to_lowercase();
            let title_matches_phrase = title_lower.contains(&q_lower);
            let title_matches_tokens = tokens.iter().all(|t| title_lower.contains(t));

            let mut matched_in_file = false;
            for (line_idx, raw_line) in body.lines().enumerate() {
                let line_lower = raw_line.to_lowercase();
                let exact_phrase = line_lower.contains(&q_lower);
                let all_tokens = tokens.iter().all(|t| line_lower.contains(t));
                if exact_phrase || all_tokens {
                    let mut score = if exact_phrase { 100 } else { 50 };
                    if title_matches_phrase {
                        score += 80;
                    } else if title_matches_tokens {
                        score += 40;
                    }
                    let snippet = raw_line.trim().to_string();
                    hits.push(WikiSearchHit {
                        wiki: item.wiki.clone(),
                        relative: item.relative.clone(),
                        title: if item.title.is_empty() {
                            item.relative.to_string_lossy().into_owned()
                        } else {
                            item.title.clone()
                        },
                        line_number: line_idx + 1,
                        snippet,
                        score,
                    });
                    matched_in_file = true;
                    if hits.len() >= 100 {
                        break;
                    }
                }
            }

            if !matched_in_file && (title_matches_phrase || title_matches_tokens) {
                let first_line = body
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                hits.push(WikiSearchHit {
                    wiki: item.wiki.clone(),
                    relative: item.relative.clone(),
                    title: item.title.clone(),
                    line_number: 1,
                    snippet: first_line,
                    score: if title_matches_phrase { 90 } else { 45 },
                });
            }
        }

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.line_number.cmp(&b.line_number))
        });
        hits.truncate(60);
        self.wiki_search_hits = hits;
    }

    /// Opens the selected wiki-wide search hit (`Ctrl+W`) and jumps to the matching line.
    pub fn open_wiki_search_selected(&mut self) {
        let Some(hit) = self
            .wiki_search_hits
            .get(self.wiki_search_selected)
            .cloned()
        else {
            return;
        };
        let query = self.wiki_search_query.trim().to_string();
        self.mode = Mode::Normal;
        let path_str = hit.relative.to_string_lossy().into_owned();
        if self.load_page(&hit.wiki, &path_str, true).is_ok() {
            self.in_page_query = query;
            self.update_in_page_search();
            if self.search_matches.is_empty() {
                self.scroll = hit.line_number.saturating_sub(1);
            }
            self.status_message = Some(format!(
                "Opened {} at match '{}'",
                hit.title, self.in_page_query
            ));
        }
    }

    /// Creates a Quick Note inside `QuickNotes/` and optionally suspends the TUI
    /// to open it directly in `$EDITOR` (`nvim`/`vim`).
    pub fn create_quick_note(&mut self, name: Option<&str>, open_editor: bool) -> Result<PathBuf> {
        let wiki = self
            .wikis
            .get(&self.current_wiki)
            .ok_or_else(|| Error::other("Current wiki is not available"))?;

        let raw_name = name.map(str::trim).filter(|s| !s.is_empty());
        let relative_name = if let Some(provided) = raw_name {
            if provided.contains('/') || provided.contains('\\') {
                provided.to_string()
            } else {
                format!("QuickNotes/{provided}")
            }
        } else {
            // Find the next available QuickNotes/quick-note-N.md
            let mut idx = 1usize;
            loop {
                let candidate = format!("QuickNotes/quick-note-{idx}");
                let abs = wiki.root.join(format!("{candidate}.md"));
                if !abs.exists() {
                    break candidate;
                }
                idx += 1;
            }
        };

        self.create_page(&relative_name, PageTemplate::QuickNote)?;
        let created_abs = wiki.root.join(&self.current_path);
        if open_editor {
            self.should_suspend_for_editor = Some(created_abs.clone());
        }
        Ok(created_abs)
    }

    /// Appends `#tag` to the currently open Markdown page, re-indexes, and reloads it.
    pub fn add_tag_to_current_page(&mut self, raw_tag: &str) -> Result<()> {
        let clean = raw_tag.trim().trim_start_matches('#');
        if clean.is_empty()
            || !clean
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/')
        {
            return Err(Error::other("Invalid tag name"));
        }
        let wiki = self
            .wikis
            .get(&self.current_wiki)
            .ok_or_else(|| Error::other("Current wiki is not available"))?;
        let abs = wiki.root.join(&self.current_path);
        if !abs.exists() {
            return Err(Error::other("Current page does not exist on disk"));
        }
        let mut content = fs::read_to_string(&abs).map_err(|e| Error::io(&abs, e))?;
        let is_tex = abs
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("tex") || e.eq_ignore_ascii_case("latex"));
        let tag_token = format!("#{clean}");
        if !content.ends_with('\n') {
            content.push('\n');
        }
        if is_tex {
            content.push_str(&format!("% {tag_token}\n"));
        } else {
            content.push_str(&format!("\n{tag_token}\n"));
        }
        fs::write(&abs, &content).map_err(|e| Error::io(&abs, e))?;

        let _ = terminalwiki_index::WikiIndex::open(wiki, self.config);
        self.rebuild_fuzzy_dataset();
        self.reload_current_page();
        self.status_message = Some(format!("Added tag {tag_token} to {}", self.current_title));
        Ok(())
    }

    /// Compiles the currently open `.tex` / `.latex` page using an installed
    /// external LaTeX engine (`tectonic`, `latexmk`, `xelatex`, `lualatex`, `pdflatex`, `pandoc`).
    pub fn compile_current_latex(&mut self, preferred_engine: Option<&str>) {
        let Some(wiki) = self.wikis.get(&self.current_wiki) else {
            self.status_message = Some("Current wiki is not available".to_string());
            return;
        };
        let abs = wiki.root.join(&self.current_path);
        let is_tex = abs
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("tex") || e.eq_ignore_ascii_case("latex"));
        if !is_tex {
            self.status_message = Some("Current page is not a .tex / .latex document".to_string());
            return;
        }
        match terminalwiki_render::latex::compile_latex_file(&abs, preferred_engine) {
            Ok(report) => {
                self.status_message = Some(report.summary);
            }
            Err(err) => {
                self.status_message = Some(err);
            }
        }
    }

    /// Toggles the page explorer. Opening it focuses the list on the page the
    /// reader is currently viewing; closing it always returns to the article.
    pub fn toggle_sidebar(&mut self) {
        if self.sidebar_visible {
            self.sidebar_visible = false;
            if self.mode == Mode::Sidebar {
                self.mode = Mode::Normal;
            }
        } else {
            self.sidebar_visible = true;
            self.mode = Mode::Sidebar;
            self.sidebar_selected = self.current_sidebar_index().unwrap_or(0);
        }
    }

    /// Index of the open page in the sidebar ordering.
    pub fn current_sidebar_index(&self) -> Option<usize> {
        self.fuzzy_dataset
            .items()
            .iter()
            .position(|item| item.wiki == self.current_wiki && item.relative == self.current_path)
    }

    /// Opens the sidebar entry selected with `j`/`k`.
    pub fn open_sidebar_selected(&mut self) -> Result<()> {
        let entries = self.sidebar_entries();
        let Some(entry) = entries.get(self.sidebar_selected) else {
            return Ok(());
        };
        match entry {
            SidebarEntry::Page { wiki, relative, .. } => {
                let rel_str = relative.to_string_lossy().into_owned();
                self.load_page(wiki, &rel_str, true)?;
                self.mode = Mode::Normal;
            }
            SidebarEntry::Heading { line, .. } => {
                self.scroll = line.saturating_sub(1);
                self.mode = Mode::Normal;
            }
            SidebarEntry::Tag { tag, .. } => {
                self.mode = Mode::Finder;
                self.finder_query = format!("#{tag}");
                self.update_finder_filter();
            }
        }
        Ok(())
    }

    /// Moves the sidebar selection by `delta`, clamped to the list.
    pub fn sidebar_select(&mut self, delta: isize) {
        let len = self.sidebar_entries().len();
        if len == 0 {
            return;
        }
        let next = (self.sidebar_selected as isize + delta).clamp(0, len as isize - 1);
        self.sidebar_selected = next as usize;
    }

    /// Opens an empty page-name prompt, optionally pre-selecting the todo
    /// template.
    pub fn begin_new_page(&mut self, template: PageTemplate) {
        self.mode = Mode::NewPage;
        self.new_page_name.clear();
        self.new_page_template = template;
    }

    /// Switches the new-page template between blank and todo list.
    pub fn toggle_new_page_template(&mut self) {
        self.new_page_template = self.new_page_template.toggle();
    }

    /// Creates the page named in the prompt and opens it.
    pub fn create_page_from_input(&mut self) -> Result<()> {
        let name = self.new_page_name.clone();
        let template = self.new_page_template;
        let result = if template == PageTemplate::QuickNote {
            self.create_quick_note(Some(&name), true).map(|_| ())
        } else {
            self.create_page(&name, template)
        };
        if result.is_ok() {
            self.mode = Mode::Normal;
        }
        result
    }

    /// Creates a Markdown page inside the current wiki without overwriting an
    /// existing file. Nested paths such as `Projects/Index` are allowed, but
    /// absolute paths and `..` components are rejected.
    pub fn create_page(&mut self, name: &str, template: PageTemplate) -> Result<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(Error::other("Page name is empty"));
        }

        let lower_trimmed = trimmed.to_ascii_lowercase();
        let is_tex = lower_trimmed.ends_with(".tex") || lower_trimmed.ends_with(".latex");
        let with_extension = if lower_trimmed.ends_with(".md") || is_tex {
            trimmed.to_string()
        } else {
            format!("{trimmed}.md")
        };
        let relative = PathBuf::from(&with_extension);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(Error::other(
                "Page name must be a relative path inside the wiki",
            ));
        }

        let wiki = self
            .wikis
            .get(&self.current_wiki)
            .ok_or_else(|| Error::other("Current wiki is not available"))?;
        let absolute = wiki.root.join(&relative);
        if absolute.exists() {
            return Err(Error::other(format!(
                "Page already exists: {}",
                relative.display()
            )));
        }

        let title = relative
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        let body = if is_tex {
            format!(
                "\\documentclass{{article}}\n\\title{{{title}}}\n\\begin{{document}}\n\\maketitle\n\n\\section{{Introduction}}\n\n\\end{{document}}\n"
            )
        } else {
            match template {
                PageTemplate::Blank => format!("# {title}\n\n"),
                PageTemplate::Todo => {
                    format!(
                        "# {title}\n\n- [ ] First task\n- [ ] Second task\n\n## Notes\n\n#todo\n"
                    )
                }
                PageTemplate::QuickNote => {
                    format!("# {title}\n\n#quicknote\n\n")
                }
            }
        };

        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
        }
        fs::write(&absolute, body).map_err(|error| Error::io(&absolute, error))?;

        let wiki_name = self.current_wiki.clone();
        let _ = terminalwiki_index::WikiIndex::open(wiki, self.config);
        self.rebuild_fuzzy_dataset();

        let relative_string = relative.to_string_lossy().into_owned();
        self.load_page(&wiki_name, &relative_string, true)?;
        self.sidebar_selected = self.current_sidebar_index().unwrap_or(0);
        self.status_message = Some(format!("Created {relative_string}"));
        Ok(())
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
            "wsearch" | "grep" => {
                self.mode = Mode::WikiSearch;
                self.wiki_search_query = if parts.len() > 1 {
                    parts[1..].join(" ")
                } else {
                    String::new()
                };
                self.update_wiki_search();
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
            "graph" => {
                let depth = parts
                    .get(1)
                    .and_then(|d| d.parse::<usize>().ok())
                    .unwrap_or(GRAPH_DEFAULT_DEPTH);
                self.open_graph_at_depth(depth.clamp(1, 5));
            }
            "edit" => self.open_current_in_editor(),
            "new" => {
                if parts.len() > 1 {
                    let page = parts[1..].join(" ");
                    if let Err(error) = self.create_page(&page, PageTemplate::Blank) {
                        self.status_message = Some(error.to_string());
                    }
                } else {
                    self.begin_new_page(PageTemplate::Blank);
                }
            }
            "quicknote" | "qn" => {
                let name = (parts.len() > 1).then(|| parts[1..].join(" "));
                if let Err(error) = self.create_quick_note(name.as_deref(), true) {
                    self.status_message = Some(error.to_string());
                }
            }
            "todo" => {
                if parts.len() > 1 {
                    let page = parts[1..].join(" ");
                    if let Err(error) = self.create_page(&page, PageTemplate::Todo) {
                        self.status_message = Some(error.to_string());
                    }
                } else {
                    self.begin_new_page(PageTemplate::Todo);
                }
            }
            "tag" => {
                if parts.len() > 1 {
                    let tag = parts[1];
                    if let Err(error) = self.add_tag_to_current_page(tag) {
                        self.status_message = Some(error.to_string());
                    }
                } else {
                    self.mode = Mode::TagPrompt;
                    self.tag_input.clear();
                }
            }
            "sidebar" => self.toggle_sidebar(),
            "tree" => self.toggle_sidebar_mode(),
            "daily" => {
                let _ = self.create_daily_note();
            }
            "allgraph" => self.open_global_graph(),
            "bookmark" => self.toggle_bookmark(),
            "bookmarks" => self.set_sidebar_mode(SidebarMode::Bookmarks),
            "linkfilter" => self.toggle_link_filter(),
            "todos" => {
                let _ = self.toggle_todo_on_current_line();
            }
            "zen" => self.toggle_zen_mode(),
            "wikipedia" | "layout" => self.toggle_wikipedia_layout(),
            "latex" | "compile" => {
                let engine = parts.get(1).copied();
                self.compile_current_latex(engine);
            }
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

    /// Builds the whole-wiki graph once and keeps it.
    ///
    /// Invalidated by [`App::invalidate_graph`] when the underlying files change.
    fn ensure_graph(&mut self) {
        if self.graph_cache.is_some() {
            return;
        }
        let mut entries = Vec::new();
        for wiki in self.wikis.iter() {
            let idx_opt = terminalwiki_index::WikiIndex::load(&wiki.name)
                .ok()
                .filter(|i| !i.entries.is_empty())
                .or_else(|| {
                    terminalwiki_index::WikiIndex::open(wiki, self.config)
                        .ok()
                        .map(|(idx, _)| idx)
                });
            if let Some(idx) = idx_opt {
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
        self.graph_cache = Some(WikiGraph::from_entries(&entries));
    }

    /// Drops the cached graph so the next consumer rebuilds it.
    pub fn invalidate_graph(&mut self) {
        self.graph_cache = None;
    }

    /// Applies a settled batch of filesystem changes (spec Gate 2.10-2.16).
    ///
    /// Everything derived from files is refreshed together so the finder,
    /// search, backlinks and graph never disagree about what exists:
    ///
    /// 1. reconcile the index of each affected wiki (incremental),
    /// 2. rebuild the fuzzy dataset from index *metadata* — never by re-reading
    ///    file bodies,
    /// 3. invalidate the graph so it is rebuilt lazily on next use,
    /// 4. reload the open page if it was touched, preserving scroll.
    ///
    /// Returns `true` if anything user-visible changed and a redraw is needed.
    pub fn apply_fs_changes(&mut self, changes: &[WikiChange]) -> bool {
        if changes.is_empty() {
            return false;
        }

        // The open page may have been renamed out from under us; follow it
        // rather than showing a stale or missing page (spec Gate 2.16).
        let mut followed_rename = None;
        for change in changes {
            if let ChangeKind::Rename { from } = &change.kind {
                if change.wiki == self.current_wiki {
                    if let Some(wiki) = self.wikis.get(&self.current_wiki) {
                        if from.strip_prefix(&wiki.root).ok() == Some(self.current_path.as_path()) {
                            followed_rename = change.relative_to(&wiki.root);
                        }
                    }
                }
            }
        }

        // One reconcile per affected wiki, not per changed file.
        let mut wikis_touched: Vec<String> = changes.iter().map(|c| c.wiki.clone()).collect();
        wikis_touched.sort();
        wikis_touched.dedup();

        for name in &wikis_touched {
            if let Some(wiki) = self.wikis.get(name) {
                if let Err(e) = terminalwiki_index::WikiIndex::open(wiki, self.config) {
                    self.status_message = Some(format!("Index update failed: {e}"));
                }
            }
        }

        self.rebuild_fuzzy_dataset();
        self.invalidate_graph();

        // A graph view showing a now-stale neighbourhood is reopened at the
        // same depth so it reflects the new link structure.
        if self.mode == Mode::Graph {
            if let Some(depth) = self.graph_view.as_ref().map(|v| v.depth) {
                self.open_graph_at_depth(depth);
            }
        }

        if let Some(new_rel) = followed_rename {
            let new_str = new_rel.to_string_lossy().into_owned();
            let wiki = self.current_wiki.clone();
            if self.load_page(&wiki, &new_str, false).is_ok() {
                self.page_missing = false;
                self.status_message = Some(format!("Page renamed to {new_str}"));
                return true;
            }
        }

        if self.current_page_affected(changes) {
            self.reload_current_page();
        }

        true
    }

    /// True if one of the changes touches the page currently displayed.
    fn current_page_affected(&self, changes: &[WikiChange]) -> bool {
        let Some(wiki) = self.wikis.get(&self.current_wiki) else {
            return false;
        };
        let absolute = wiki.root.join(&self.current_path);
        changes.iter().any(|c| {
            c.path == absolute
                || matches!(&c.kind, ChangeKind::Rename { from } if *from == absolute)
        })
    }

    /// Re-renders the open page, keeping the reader roughly where they were.
    ///
    /// Scroll is clamped rather than restored exactly: after an edit the old
    /// line number may not exist (spec Gate 2.14). Block-level anchoring comes
    /// with the rich-content layout model.
    pub fn reload_current_page(&mut self) {
        if self.current_path.as_os_str().is_empty() {
            return;
        }
        let Some(wiki) = self.wikis.get(&self.current_wiki) else {
            return;
        };
        let absolute = wiki.root.join(&self.current_path);

        if !absolute.exists() {
            // Deleted externally: say so instead of crashing or showing stale
            // content (spec Gate 2.15).
            self.page_missing = true;
            self.status_message =
                Some("Page was removed from disk — [b] back  [r] retry".to_string());
            return;
        }

        let previous_scroll = self.scroll;
        let page = self.current_path.to_string_lossy().into_owned();
        let wiki_name = self.current_wiki.clone();
        if self.load_page(&wiki_name, &page, false).is_ok() {
            self.page_missing = false;
            let max_scroll = self.lines.len().saturating_sub(1);
            self.scroll = previous_scroll.min(max_scroll);
        }
    }

    /// Rebuilds the fuzzy dataset from index metadata.
    ///
    /// Metadata only — titles, paths, aliases, tags. Re-reading file bodies to
    /// refresh a finder would make every save cost a full scan.
    pub fn rebuild_fuzzy_dataset(&mut self) {
        let mut items = Vec::new();
        for wiki in self.wikis.iter() {
            if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
                for e in &idx.entries {
                    if e.relative.file_name()
                        == Some(std::ffi::OsStr::new(
                            terminalwiki_core::config::WIKI_CONFIG_FILE,
                        ))
                    {
                        continue;
                    }
                    items.push(FuzzyItem {
                        wiki: wiki.name.clone(),
                        relative: e.relative.clone(),
                        title: e.title.clone(),
                        aliases: e.aliases.clone(),
                        tags: e.tags.clone(),
                    });
                }
            }
        }
        self.fuzzy_dataset = FuzzyDataset::new(items);
        self.sidebar_selected = self
            .sidebar_selected
            .min(self.fuzzy_dataset.items().len().saturating_sub(1));
        if self.mode == Mode::Finder {
            self.update_finder_filter();
        }
    }

    pub fn load_backlinks(&mut self) {
        self.ensure_graph();
        if let Some(graph) = self.graph_cache.as_ref() {
            self.backlinks = graph.backlinks(&self.current_wiki, &self.current_path);
        }
        self.backlinks_selected = 0;
    }

    /// Opens the local graph around the current page (spec items 41-42).
    pub fn open_graph(&mut self) {
        let depth = self
            .graph_view
            .as_ref()
            .map(|v| v.depth)
            .unwrap_or(GRAPH_DEFAULT_DEPTH);
        self.open_graph_at_depth(depth);
    }

    /// Recomputes the neighbourhood and layout at `depth` (spec item 7.10).
    pub fn open_graph_at_depth(&mut self, depth: usize) {
        if self.current_path.as_os_str().is_empty() {
            self.status_message = Some("No page open to graph".to_string());
            return;
        }
        self.ensure_graph();
        let Some(graph) = self.graph_cache.as_ref() else {
            self.status_message = Some("Graph unavailable".to_string());
            return;
        };

        let uncapped = graph.neighborhood(&self.current_wiki, &self.current_path, depth);
        if uncapped.nodes.is_empty() {
            self.status_message =
                Some("This page is not in the link graph (index may be stale)".to_string());
            return;
        }
        let capped = uncapped.nodes.len() > GRAPH_MAX_NODES;
        let sub = if capped {
            graph.neighborhood_limited(
                &self.current_wiki,
                &self.current_path,
                depth,
                GRAPH_MAX_NODES,
            )
        } else {
            uncapped
        };

        let pos = LayoutEngine::compute_layout(graph, &sub);

        // Order the selection domain spatially so j/k reads as up/down.
        let mut ordered_nodes = sub.nodes.clone();
        ordered_nodes.sort_by(|a, b| {
            let pa = pos.get(a).copied().unwrap_or((0.0, 0.0));
            let pb = pos.get(b).copied().unwrap_or((0.0, 0.0));
            pa.1.total_cmp(&pb.1)
                .then(pa.0.total_cmp(&pb.0))
                .then(a.cmp(b))
        });
        // Start on the page the user came from.
        let selected = ordered_nodes
            .iter()
            .position(|&n| n == sub.center)
            .unwrap_or(0);

        self.graph_view = Some(GraphViewState {
            root_wiki: self.current_wiki.clone(),
            root_path: self.current_path.clone(),
            depth,
            sub,
            pos,
            ordered_nodes,
            selected,
            capped,
            is_global: false,
        });
        self.mode = Mode::Graph;
        self.status_message = None;
    }

    /// Opens the complete vault graph showing all notes and interconnecting edges.
    pub fn open_global_graph(&mut self) {
        self.ensure_graph();
        let Some(graph) = self.graph_cache.as_ref() else {
            self.status_message = Some("Graph unavailable".to_string());
            return;
        };

        let center_opt = graph.find_node_index(&self.current_wiki, &self.current_path);
        let sub = graph.full_graph(GRAPH_MAX_NODES, center_opt);
        if sub.nodes.is_empty() {
            self.status_message = Some("No nodes in vault graph".to_string());
            return;
        }

        let pos = LayoutEngine::compute_layout(graph, &sub);
        let mut ordered_nodes = sub.nodes.clone();
        ordered_nodes.sort_by(|a, b| {
            let pa = pos.get(a).copied().unwrap_or((0.0, 0.0));
            let pb = pos.get(b).copied().unwrap_or((0.0, 0.0));
            pa.1.total_cmp(&pb.1)
                .then(pa.0.total_cmp(&pb.0))
                .then(a.cmp(b))
        });
        let selected = ordered_nodes
            .iter()
            .position(|&n| n == sub.center)
            .unwrap_or(0);

        let capped = graph.nodes().len() > GRAPH_MAX_NODES;

        self.graph_view = Some(GraphViewState {
            root_wiki: self.current_wiki.clone(),
            root_path: self.current_path.clone(),
            depth: 1,
            sub,
            pos,
            ordered_nodes,
            selected,
            capped,
            is_global: true,
        });
        self.mode = Mode::Graph;
        self.status_message = Some("Global Vault Graph".to_string());
    }

    /// Toggles between local neighborhood graph and global vault graph.
    pub fn toggle_graph_scope(&mut self) {
        let is_global = self
            .graph_view
            .as_ref()
            .map(|v| v.is_global)
            .unwrap_or(false);
        if is_global {
            let depth = self
                .graph_view
                .as_ref()
                .map(|v| v.depth)
                .unwrap_or(GRAPH_DEFAULT_DEPTH);
            self.open_graph_at_depth(depth);
        } else {
            self.open_global_graph();
        }
    }

    /// Moves the graph selection by `delta`, saturating at both ends.
    pub fn graph_select(&mut self, delta: isize) {
        if let Some(view) = self.graph_view.as_mut() {
            if view.ordered_nodes.is_empty() {
                return;
            }
            let last = view.ordered_nodes.len() - 1;
            let next = (view.selected as isize + delta).clamp(0, last as isize);
            view.selected = next as usize;
        }
    }

    /// Changes neighbourhood depth, reloading the surrounding nodes.
    pub fn graph_change_depth(&mut self, delta: isize) {
        let Some(view) = self.graph_view.as_ref() else {
            return;
        };
        let new_depth = (view.depth as isize + delta).clamp(1, 5) as usize;
        if new_depth == view.depth {
            return;
        }
        self.open_graph_at_depth(new_depth);
        if let Some(v) = self.graph_view.as_ref() {
            self.status_message = Some(format!("Graph depth {}", v.depth));
        }
    }

    /// Opens the selected graph node as a page or code file (spec items 44-45).
    pub fn graph_open_selected(&mut self) {
        let Some(view) = self.graph_view.as_ref() else {
            return;
        };
        let Some(node_idx) = view.selected_node() else {
            return;
        };
        let Some(graph) = self.graph_cache.as_ref() else {
            return;
        };
        let Some(node) = graph.node(node_idx) else {
            return;
        };

        let wiki = node.id.wiki.clone();
        let relative = node.id.relative.to_string_lossy().into_owned();

        // Leaving the graph before loading keeps history semantics identical to
        // following a link from the article view.
        self.mode = Mode::Normal;
        self.graph_view = None;
        if let Err(e) = self.load_page(&wiki, &relative, true) {
            self.status_message = Some(format!("Could not open node: {e}"));
        }
    }

    /// Closes the graph and returns to the article.
    pub fn close_graph(&mut self) {
        self.graph_view = None;
        self.mode = Mode::Normal;
    }

    pub fn open_current_in_editor(&mut self) {
        if let Some(wiki) = self.wikis.get(&self.current_wiki) {
            let full_path = wiki.root.join(&self.current_path);
            self.should_suspend_for_editor = Some(full_path);
        }
    }
}

/// Splits a finder query into free text and tag filters.
///
/// Both `#security` and `tag:security` are accepted; every tag must match.
/// The text remainder is still fuzzy-matched against titles and paths.
fn split_finder_query(query: &str) -> (String, Vec<String>) {
    let mut text = Vec::new();
    let mut tags = Vec::new();
    for token in query.split_whitespace() {
        let tag = token
            .strip_prefix('#')
            .or_else(|| token.strip_prefix("tag:"))
            .map(|tag| tag.trim_start_matches('#'));
        match tag {
            Some(tag) if !tag.is_empty() => tags.push(tag.to_string()),
            Some(_) => {}
            None => text.push(token),
        }
    }
    (text.join(" "), tags)
}
