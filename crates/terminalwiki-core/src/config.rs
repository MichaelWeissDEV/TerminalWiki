//! Configuration and its layering rules (spec §81, §82, §83).
//!
//! Precedence, lowest to highest (spec §83):
//!
//! ```text
//! defaults < global config < wiki config < environment < CLI arguments
//! ```
//!
//! Each source is parsed into a [`ConfigLayer`] whose fields are all
//! `Option`. Layers are folded in precedence order, so "the more specific
//! setting wins" is a property of the data structure rather than something
//! reimplemented at each call site.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::filetype::parse_size;
use crate::paths;

/// Colour theme selection (spec §64).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Auto,
    Dark,
    Light,
    Mono,
}

impl Theme {
    pub fn parse(s: &str) -> Option<Theme> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Theme::Auto),
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            "mono" | "none" => Some(Theme::Mono),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Auto => "auto",
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Mono => "mono",
        }
    }
}

/// Graphics backend selection (spec §33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsMode {
    #[default]
    Auto,
    Kitty,
    Iterm2,
    Sixel,
    Unicode,
    Off,
}

impl GraphicsMode {
    pub fn parse(s: &str) -> Option<GraphicsMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(GraphicsMode::Auto),
            "kitty" => Some(GraphicsMode::Kitty),
            "iterm2" | "iterm" => Some(GraphicsMode::Iterm2),
            "sixel" => Some(GraphicsMode::Sixel),
            "unicode" => Some(GraphicsMode::Unicode),
            "off" | "none" | "false" => Some(GraphicsMode::Off),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GraphicsMode::Auto => "auto",
            GraphicsMode::Kitty => "kitty",
            GraphicsMode::Iterm2 => "iterm2",
            GraphicsMode::Sixel => "sixel",
            GraphicsMode::Unicode => "unicode",
            GraphicsMode::Off => "off",
        }
    }
}

/// Tri-state used for capability overrides (spec §34, §39).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tri {
    #[default]
    Auto,
    On,
    Off,
}

impl Tri {
    pub fn parse(s: &str) -> Option<Tri> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Tri::Auto),
            "on" | "true" | "yes" => Some(Tri::On),
            "off" | "false" | "no" => Some(Tri::Off),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Tri::Auto => "auto",
            Tri::On => "on",
            Tri::Off => "off",
        }
    }
}

/// Naming policy for `tw new` (spec §46).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NamingPolicy {
    Preserve,
    #[default]
    KebabCase,
    SnakeCase,
    Slug,
}

impl NamingPolicy {
    pub fn parse(s: &str) -> Option<NamingPolicy> {
        match s.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "preserve" => Some(NamingPolicy::Preserve),
            "kebab_case" | "kebab" => Some(NamingPolicy::KebabCase),
            "snake_case" | "snake" => Some(NamingPolicy::SnakeCase),
            "slug" => Some(NamingPolicy::Slug),
            _ => None,
        }
    }

    /// Applies the policy to a human-written page title (spec §46).
    pub fn apply(self, title: &str) -> String {
        let title = title.trim();
        match self {
            NamingPolicy::Preserve => title.to_string(),
            NamingPolicy::KebabCase => join_words(title, '-', false),
            NamingPolicy::SnakeCase => join_words(title, '_', false),
            NamingPolicy::Slug => join_words(title, '-', true),
        }
    }
}

fn join_words(title: &str, sep: char, lowercase: bool) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_sep = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push(sep);
            }
            pending_sep = false;
            if lowercase {
                out.extend(c.to_lowercase());
            } else {
                out.push(c);
            }
        } else {
            // Any run of non-alphanumerics collapses into one separator.
            pending_sep = true;
        }
    }
    out
}

/// A registered wiki (spec §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiEntry {
    pub name: String,
    pub path: PathBuf,
    /// Names of wikis logically mounted into this one. This is a *logical*
    /// mount: no file is ever copied (spec §8).
    pub mounts: Vec<String>,
}

/// Render settings (spec §79, §81).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderConfig {
    /// Maximum width for prose. Code may exceed it (spec §79).
    pub max_content_width: usize,
    pub line_numbers: bool,
    pub graphics: GraphicsMode,
    pub math: bool,
    pub tab_width: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            max_content_width: 110,
            line_numbers: true,
            graphics: GraphicsMode::Auto,
            math: true,
            tab_width: 4,
        }
    }
}

/// Index settings (spec §54, §81).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexConfig {
    /// Whether code files participate in the index (spec §81).
    pub code: bool,
    /// Whether hidden files are scanned (spec §81).
    pub hidden: bool,
    /// Files above this size are recorded but not fully indexed (spec §54).
    pub max_file_size: u64,
    /// Extra ignore globs, merged with `.gitignore` and `.twignore` (spec §53).
    pub ignore: Vec<String>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        IndexConfig {
            code: true,
            hidden: false,
            max_file_size: 16 * 1024 * 1024,
            ignore: Vec::new(),
        }
    }
}

/// TUI settings (spec §81).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiConfig {
    pub mouse: bool,
    pub history_size: usize,
}

impl Default for TuiConfig {
    fn default() -> Self {
        TuiConfig {
            mouse: true,
            history_size: 500,
        }
    }
}

/// Terminal settings (spec §39, §81).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalConfig {
    pub tmux_passthrough: Tri,
}

/// Fully resolved configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
    pub default_wiki: Option<String>,
    pub editor: Option<String>,
    pub theme: Theme,
    pub naming: NamingPolicy,
    pub render: RenderConfig,
    pub index: IndexConfig,
    pub tui: TuiConfig,
    pub terminal: TerminalConfig,
    pub wikis: Vec<WikiEntry>,
    /// Where the global config was read from, for `tw config` and `tw doctor`.
    pub source: Option<PathBuf>,
}

impl Config {
    pub fn wiki(&self, name: &str) -> Option<&WikiEntry> {
        self.wikis.iter().find(|w| w.name == name)
    }

    pub fn wiki_names(&self) -> Vec<String> {
        self.wikis.iter().map(|w| w.name.clone()).collect()
    }

    /// The wiki used when the user names none (spec §8, §10).
    pub fn default_wiki(&self) -> Option<&WikiEntry> {
        if let Some(name) = &self.default_wiki {
            if let Some(w) = self.wiki(name) {
                return Some(w);
            }
        }
        // A single registered wiki is unambiguously the default.
        self.wikis.first()
    }
}

/// One layer of configuration; every field is optional so layers can merge.
#[derive(Debug, Clone, Default)]
pub struct ConfigLayer {
    pub default_wiki: Option<String>,
    pub editor: Option<String>,
    pub theme: Option<Theme>,
    pub naming: Option<NamingPolicy>,
    pub max_content_width: Option<usize>,
    pub line_numbers: Option<bool>,
    pub graphics: Option<GraphicsMode>,
    pub math: Option<bool>,
    pub tab_width: Option<usize>,
    pub index_code: Option<bool>,
    pub index_hidden: Option<bool>,
    pub max_file_size: Option<u64>,
    pub ignore: Option<Vec<String>>,
    pub mouse: Option<bool>,
    pub history_size: Option<usize>,
    pub tmux_passthrough: Option<Tri>,
    pub wikis: Option<Vec<WikiEntry>>,
    pub source: Option<PathBuf>,
}

impl ConfigLayer {
    /// Applies this layer on top of `base`. Set fields win.
    pub fn apply_to(&self, base: &mut Config) {
        macro_rules! set {
            ($field:expr, $value:expr) => {
                if let Some(v) = $value.clone() {
                    $field = v;
                }
            };
        }
        if self.default_wiki.is_some() {
            base.default_wiki = self.default_wiki.clone();
        }
        if self.editor.is_some() {
            base.editor = self.editor.clone();
        }
        set!(base.theme, self.theme);
        set!(base.naming, self.naming);
        set!(base.render.max_content_width, self.max_content_width);
        set!(base.render.line_numbers, self.line_numbers);
        set!(base.render.graphics, self.graphics);
        set!(base.render.math, self.math);
        set!(base.render.tab_width, self.tab_width);
        set!(base.index.code, self.index_code);
        set!(base.index.hidden, self.index_hidden);
        set!(base.index.max_file_size, self.max_file_size);
        set!(base.tui.mouse, self.mouse);
        set!(base.tui.history_size, self.history_size);
        set!(base.terminal.tmux_passthrough, self.tmux_passthrough);
        set!(base.wikis, self.wikis);
        // Ignore globs accumulate rather than replace: a wiki adding its own
        // build directory should not discard the user's global rules.
        if let Some(extra) = &self.ignore {
            base.index.ignore.extend(extra.iter().cloned());
        }
        if self.source.is_some() {
            base.source = self.source.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Serde representations of the on-disk files
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct GlobalFile {
    default_wiki: Option<String>,
    editor: Option<String>,
    theme: Option<String>,
    naming: Option<String>,
    #[serde(default)]
    render: Option<RenderFile>,
    #[serde(default)]
    index: Option<IndexFile>,
    #[serde(default)]
    tui: Option<TuiFile>,
    #[serde(default)]
    terminal: Option<TerminalFile>,
    #[serde(default, rename = "wiki", alias = "wikis")]
    wikis: Vec<WikiFile>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RenderFile {
    max_content_width: Option<usize>,
    line_numbers: Option<bool>,
    graphics: Option<String>,
    math: Option<bool>,
    tab_width: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct IndexFile {
    code: Option<bool>,
    hidden: Option<bool>,
    max_file_size: Option<toml::Value>,
    ignore: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TuiFile {
    mouse: Option<bool>,
    history_size: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TerminalFile {
    tmux_passthrough: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WikiFile {
    name: String,
    path: String,
    #[serde(default, alias = "mounts")]
    mount: Vec<String>,
}

/// Wiki-local `.terminalwiki.toml` (spec §82).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiLocalConfig {
    /// Display title of this wiki.
    pub title: Option<String>,
    /// Explicit home page, consulted after the standard names (spec §10).
    pub home: Option<String>,
    /// Extra ignore globs for this wiki (spec §53, §82).
    #[serde(default)]
    pub ignore: Vec<String>,
    /// Wiki-specific render overrides.
    #[serde(default)]
    render: Option<RenderFile>,
}

/// The file name of a wiki-local configuration.
pub const WIKI_CONFIG_FILE: &str = ".terminalwiki.toml";

impl WikiLocalConfig {
    /// Loads `.terminalwiki.toml` from a wiki root. A missing file is not an error.
    pub fn load(wiki_root: &Path) -> Result<WikiLocalConfig> {
        let path = wiki_root.join(WIKI_CONFIG_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(WikiLocalConfig::default())
            }
            Err(e) => return Err(Error::io(&path, e)),
        };
        toml::from_str(&text).map_err(|e| Error::Config {
            message: e.message().to_string(),
            path: Some(path.clone()),
        })
    }

    /// Turns the wiki-local file into a configuration layer.
    pub fn layer(&self) -> ConfigLayer {
        let mut layer = ConfigLayer {
            ignore: Some(self.ignore.clone()),
            ..Default::default()
        };
        if let Some(r) = &self.render {
            layer.max_content_width = r.max_content_width;
            layer.line_numbers = r.line_numbers;
            layer.math = r.math;
            layer.tab_width = r.tab_width;
            layer.graphics = r.graphics.as_deref().and_then(GraphicsMode::parse);
        }
        layer
    }
}

/// Parses the global configuration text into a layer (spec §81).
pub fn parse_global(text: &str, source: Option<PathBuf>) -> Result<ConfigLayer> {
    let file: GlobalFile = toml::from_str(text).map_err(|e| Error::Config {
        message: e.message().to_string(),
        path: source.clone(),
    })?;

    let mut layer = ConfigLayer {
        source,
        ..Default::default()
    };
    layer.default_wiki = file.default_wiki;
    layer.editor = file.editor;

    if let Some(t) = &file.theme {
        layer.theme = Some(Theme::parse(t).ok_or_else(|| {
            Error::config(format!(
                "unknown theme `{t}` (expected auto, dark, light or mono)"
            ))
        })?);
    }
    if let Some(n) = &file.naming {
        layer.naming = Some(NamingPolicy::parse(n).ok_or_else(|| {
            Error::config(format!(
                "unknown naming policy `{n}` (expected preserve, kebab-case, snake_case or slug)"
            ))
        })?);
    }
    if let Some(r) = file.render {
        layer.max_content_width = r.max_content_width;
        layer.line_numbers = r.line_numbers;
        layer.math = r.math;
        layer.tab_width = r.tab_width;
        if let Some(g) = &r.graphics {
            layer.graphics = Some(GraphicsMode::parse(g).ok_or_else(|| {
                Error::config(format!(
                    "unknown graphics mode `{g}` (expected auto, kitty, iterm2, sixel, unicode or off)"
                ))
            })?);
        }
    }
    if let Some(i) = file.index {
        layer.index_code = i.code;
        layer.index_hidden = i.hidden;
        layer.ignore = i.ignore;
        if let Some(v) = &i.max_file_size {
            layer.max_file_size = Some(parse_toml_size(v)?);
        }
    }
    if let Some(t) = file.tui {
        layer.mouse = t.mouse;
        layer.history_size = t.history_size;
    }
    if let Some(t) = file.terminal {
        if let Some(p) = &t.tmux_passthrough {
            layer.tmux_passthrough = Some(Tri::parse(p).ok_or_else(|| {
                Error::config(format!(
                    "unknown tmux_passthrough `{p}` (expected auto, on or off)"
                ))
            })?);
        }
    }

    // Wiki entries are validated eagerly: a duplicate name would make page
    // resolution ambiguous, which spec §8 forbids.
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    let mut wikis = Vec::with_capacity(file.wikis.len());
    for w in file.wikis {
        if w.name.trim().is_empty() {
            return Err(Error::config("wiki name must not be empty"));
        }
        if seen.insert(w.name.clone(), ()).is_some() {
            return Err(Error::config(format!("duplicate wiki name `{}`", w.name)));
        }
        wikis.push(WikiEntry {
            name: w.name,
            path: paths::expand_tilde(Path::new(&w.path)),
            mounts: w.mount,
        });
    }
    if !wikis.is_empty() {
        layer.wikis = Some(wikis);
    }
    Ok(layer)
}

/// `max_file_size` may be written as `"16 MiB"` or as a plain integer.
fn parse_toml_size(v: &toml::Value) -> Result<u64> {
    match v {
        toml::Value::Integer(i) if *i >= 0 => Ok(*i as u64),
        toml::Value::String(s) => parse_size(s)
            .ok_or_else(|| Error::config(format!("cannot parse size `{s}` (try \"16 MiB\")"))),
        other => Err(Error::config(format!(
            "max_file_size must be a size string, got {other}"
        ))),
    }
}

/// Reads the environment layer (spec §83).
///
/// Only settings a user plausibly wants to vary per-invocation are exposed
/// here; everything else belongs in the config file.
pub fn env_layer() -> ConfigLayer {
    let mut layer = ConfigLayer::default();
    if let Ok(v) = std::env::var("TW_WIKI") {
        if !v.is_empty() {
            layer.default_wiki = Some(v);
        }
    }
    // $VISUAL / $EDITOR are handled at edit time (spec §45); TW_EDITOR is the
    // TerminalWiki-specific override that outranks both.
    if let Ok(v) = std::env::var("TW_EDITOR") {
        if !v.is_empty() {
            layer.editor = Some(v);
        }
    }
    if let Ok(v) = std::env::var("TW_THEME") {
        layer.theme = Theme::parse(&v);
    }
    if let Ok(v) = std::env::var("TW_GRAPHICS") {
        layer.graphics = GraphicsMode::parse(&v);
    }
    if let Ok(v) = std::env::var("TW_MAX_CONTENT_WIDTH") {
        layer.max_content_width = v.parse().ok();
    }
    // NO_COLOR implies the mono theme unless something more specific is set
    // later in the chain (spec §64).
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) && layer.theme.is_none() {
        layer.theme = Some(Theme::Mono);
    }
    layer
}

/// Loads the global configuration file, if one exists.
///
/// A missing config file is normal: TerminalWiki works with zero configuration.
pub fn load_global() -> Result<ConfigLayer> {
    let Some(path) = config_path() else {
        return Ok(ConfigLayer::default());
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_global(&text, Some(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigLayer::default()),
        Err(e) => Err(Error::io(&path, e)),
    }
}

/// The path of the global config file, honouring `TW_CONFIG`.
pub fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("TW_CONFIG") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    paths::config_file()
}

/// Builds the configuration from defaults, the global file and the environment.
///
/// Wiki-local configuration and CLI arguments are applied later by the caller,
/// because both depend on which wiki was selected.
pub fn load() -> Result<Config> {
    let mut config = Config::default();
    load_global()?.apply_to(&mut config);
    env_layer().apply_to(&mut config);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_spec_configuration_example() {
        let text = r#"
default_wiki = "main"
editor = "nvim"
theme = "auto"

[render]
max_content_width = 110
line_numbers = true
graphics = "auto"
math = true

[index]
code = true
hidden = false
max_file_size = "16 MiB"

[tui]
mouse = true
history_size = 500

[terminal]
tmux_passthrough = "auto"

[[wiki]]
name = "main"
path = "/home/user/Knowledge"

[[wiki]]
name = "rust"
path = "/home/user/Knowledge-Rust"
"#;
        let layer = parse_global(text, None).expect("valid config");
        let mut cfg = Config::default();
        layer.apply_to(&mut cfg);

        assert_eq!(cfg.default_wiki.as_deref(), Some("main"));
        assert_eq!(cfg.editor.as_deref(), Some("nvim"));
        assert_eq!(cfg.theme, Theme::Auto);
        assert_eq!(cfg.render.max_content_width, 110);
        assert_eq!(cfg.index.max_file_size, 16 * 1024 * 1024);
        assert_eq!(cfg.tui.history_size, 500);
        assert_eq!(cfg.terminal.tmux_passthrough, Tri::Auto);
        assert_eq!(cfg.wikis.len(), 2);
        assert_eq!(cfg.wikis[0].name, "main");
        assert_eq!(cfg.default_wiki().unwrap().name, "main");
    }

    #[test]
    fn mounts_are_logical_and_recorded_per_wiki() {
        let text = r#"
[[wiki]]
name = "main"
path = "/k"
mount = ["rust", "security"]
"#;
        let layer = parse_global(text, None).unwrap();
        let wikis = layer.wikis.unwrap();
        assert_eq!(wikis[0].mounts, vec!["rust", "security"]);
    }

    #[test]
    fn the_more_specific_layer_wins() {
        let mut cfg = Config::default();
        assert_eq!(cfg.render.max_content_width, 110);

        let global = ConfigLayer {
            max_content_width: Some(90),
            ..Default::default()
        };
        global.apply_to(&mut cfg);
        assert_eq!(cfg.render.max_content_width, 90);

        let wiki = ConfigLayer {
            max_content_width: Some(70),
            ..Default::default()
        };
        wiki.apply_to(&mut cfg);
        assert_eq!(cfg.render.max_content_width, 70);

        // An unset field must not clobber what a lower layer established.
        let empty = ConfigLayer::default();
        empty.apply_to(&mut cfg);
        assert_eq!(cfg.render.max_content_width, 70);

        let cli = ConfigLayer {
            max_content_width: Some(60),
            ..Default::default()
        };
        cli.apply_to(&mut cfg);
        assert_eq!(cfg.render.max_content_width, 60);
    }

    #[test]
    fn ignore_globs_accumulate_across_layers() {
        let mut cfg = Config::default();
        ConfigLayer {
            ignore: Some(vec!["target/".into()]),
            ..Default::default()
        }
        .apply_to(&mut cfg);
        ConfigLayer {
            ignore: Some(vec!["build/".into()]),
            ..Default::default()
        }
        .apply_to(&mut cfg);
        assert_eq!(cfg.index.ignore, vec!["target/", "build/"]);
    }

    #[test]
    fn duplicate_wiki_names_are_rejected() {
        let text =
            "[[wiki]]\nname = \"a\"\npath = \"/x\"\n\n[[wiki]]\nname = \"a\"\npath = \"/y\"\n";
        let err = parse_global(text, None).unwrap_err();
        assert_eq!(err.exit_code(), crate::ExitCode::ConfigError);
    }

    #[test]
    fn unknown_keys_are_rejected_rather_than_silently_ignored() {
        // A typo in a config file should be reported, not quietly dropped.
        let err = parse_global("dafault_wiki = \"main\"\n", None).unwrap_err();
        assert_eq!(err.exit_code(), crate::ExitCode::ConfigError);
    }

    #[test]
    fn invalid_enum_values_report_the_valid_options() {
        let err = parse_global("theme = \"neon\"\n", None).unwrap_err();
        assert!(err.to_string().contains("auto"), "{err}");
    }

    #[test]
    fn max_file_size_accepts_a_string_or_an_integer() {
        let a = parse_global("[index]\nmax_file_size = \"2 MiB\"\n", None).unwrap();
        assert_eq!(a.max_file_size, Some(2 * 1024 * 1024));
        let b = parse_global("[index]\nmax_file_size = 4096\n", None).unwrap();
        assert_eq!(b.max_file_size, Some(4096));
    }

    #[test]
    fn wiki_local_config_parses_the_spec_example() {
        let text = "title = \"Security Knowledge Base\"\nhome = \"index.md\"\n\nignore = [\n  \"build/\",\n  \"captures/\"\n]\n";
        let local: WikiLocalConfig = toml::from_str(text).unwrap();
        assert_eq!(local.title.as_deref(), Some("Security Knowledge Base"));
        assert_eq!(local.home.as_deref(), Some("index.md"));
        assert_eq!(local.ignore, vec!["build/", "captures/"]);
    }

    #[test]
    fn naming_policies_produce_the_documented_file_names() {
        let t = "Memory Management";
        assert_eq!(NamingPolicy::Preserve.apply(t), "Memory Management");
        assert_eq!(NamingPolicy::KebabCase.apply(t), "Memory-Management");
        assert_eq!(NamingPolicy::SnakeCase.apply(t), "Memory_Management");
        assert_eq!(NamingPolicy::Slug.apply(t), "memory-management");
    }

    #[test]
    fn naming_policies_collapse_punctuation_runs() {
        assert_eq!(
            NamingPolicy::KebabCase.apply("Use   After -- Free"),
            "Use-After-Free"
        );
        assert_eq!(NamingPolicy::Slug.apply("C++ / Rust!"), "c-rust");
        // Non-ASCII must survive rather than be mangled away.
        assert_eq!(
            NamingPolicy::KebabCase.apply("Größe messen"),
            "Größe-messen"
        );
    }

    #[test]
    fn a_single_registered_wiki_is_the_default_without_configuration() {
        let mut cfg = Config::default();
        cfg.wikis.push(WikiEntry {
            name: "only".into(),
            path: PathBuf::from("/k"),
            mounts: vec![],
        });
        assert_eq!(cfg.default_wiki().unwrap().name, "only");
    }

    #[test]
    fn empty_config_is_valid_and_yields_defaults() {
        let layer = parse_global("", None).unwrap();
        let mut cfg = Config::default();
        layer.apply_to(&mut cfg);
        assert_eq!(cfg, Config::default());
    }
}
