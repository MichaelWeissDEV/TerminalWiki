#[derive(Debug, Default)]
pub struct Args {
    pub command: Command,
    pub wiki: Option<String>,
    pub all: bool,
    pub plain: bool,
    pub json: bool,
    pub jsonl: bool,
    pub path_only: bool,
    pub no_color: bool,
    pub color: Option<String>,
    pub pager: bool,
    pub version: bool,
    pub help: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Command {
    #[default]
    Home,
    Page {
        wiki: Option<String>,
        page: String,
    },
    Tui {
        wiki: Option<String>,
        page: Option<String>,
    },
    Search {
        query: String,
    },
    Find {
        query: String,
    },
    Raw {
        page: String,
    },
    Render {
        file: String,
    },
    New {
        page: String,
    },
    Edit {
        page: String,
    },
    Delete {
        page: String,
        force: bool,
    },
    Backlinks {
        page: String,
    },
    Links {
        page: String,
    },
    Related {
        page: String,
    },
    Graph {
        page: Option<String>,
        depth: Option<usize>,
        format: Option<String>,
    },
    Tags {
        wiki: Option<String>,
    },
    Tag {
        tag: String,
    },
    Files {
        type_filter: Option<String>,
    },
    Query {
        query: String,
    },
    Wiki(WikiCommand),
    Index(IndexCommand),
    Lint,
    Doctor,
    Stats,
    Config,
    Completions {
        shell: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum WikiCommand {
    List,
    Add {
        name: String,
        path: String,
        default: bool,
    },
    Remove {
        name: String,
    },
    Rename {
        old: String,
        new: String,
    },
    Mount {
        parent: String,
        child: String,
    },
    Unmount {
        parent: String,
        child: String,
    },
    Default {
        name: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum IndexCommand {
    Status,
    Update,
    Rebuild,
}

impl Args {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut parsed = Args::default();
        let mut positional = Vec::new();
        let mut i = 1;

        while i < args.len() {
            let arg = &args[i];
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--all" => parsed.all = true,
                    "--plain" => parsed.plain = true,
                    "--json" => parsed.json = true,
                    "--jsonl" => parsed.jsonl = true,
                    "--path-only" => parsed.path_only = true,
                    "--no-color" => parsed.no_color = true,
                    "--pager" => parsed.pager = true,
                    "--version" => parsed.version = true,
                    "--help" => parsed.help = true,
                    "--force" => positional.push(arg.clone()), // handled per command later
                    "--default" => positional.push(arg.clone()),
                    _ if arg.starts_with("--wiki=") => {
                        parsed.wiki = Some(arg["--wiki=".len()..].to_string());
                    }
                    "--wiki" => {
                        i += 1;
                        if i < args.len() {
                            parsed.wiki = Some(args[i].clone());
                        } else {
                            return Err("Missing argument for --wiki".to_string());
                        }
                    }
                    _ if arg.starts_with("--color=") => {
                        parsed.color = Some(arg["--color=".len()..].to_string());
                    }
                    _ if arg.starts_with("--depth=") => {
                        positional.push(arg.clone());
                    }
                    "--depth" => {
                        positional.push(arg.clone());
                        i += 1;
                        if i < args.len() {
                            positional.push(args[i].clone());
                        }
                    }
                    _ if arg.starts_with("--format=") => {
                        positional.push(arg.clone());
                    }
                    "--format" => {
                        positional.push(arg.clone());
                        i += 1;
                        if i < args.len() {
                            positional.push(args[i].clone());
                        }
                    }
                    _ if arg.starts_with("--type=") => {
                        positional.push(arg.clone());
                    }
                    "--type" => {
                        positional.push(arg.clone());
                        i += 1;
                        if i < args.len() {
                            positional.push(args[i].clone());
                        }
                    }
                    _ => {
                        // ignore unknown or pass to command
                        positional.push(arg.clone());
                    }
                }
            } else if arg.starts_with("-") && arg != "-" {
                if arg == "-h" {
                    parsed.help = true;
                } else {
                    positional.push(arg.clone());
                }
            } else {
                positional.push(arg.clone());
            }
            i += 1;
        }

        if parsed.help || parsed.version {
            return Ok(parsed);
        }

        if positional.is_empty() {
            parsed.command = Command::Home;
            return Ok(parsed);
        }

        let cmd = positional[0].as_str();
        match cmd {
            "page" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'page' command".to_string());
                }
                parsed.command = Command::Page {
                    wiki: parsed.wiki.clone(),
                    page: positional[1].clone(),
                };
            }
            "tui" => {
                let mut wiki = None;
                let mut page = None;
                if positional.len() == 2 {
                    page = Some(positional[1].clone());
                } else if positional.len() == 3 {
                    wiki = Some(positional[1].clone());
                    page = Some(positional[2].clone());
                }
                parsed.command = Command::Tui { wiki, page };
            }
            "search" => {
                if positional.len() < 2 {
                    return Err("Missing QUERY argument for 'search' command".to_string());
                }
                parsed.command = Command::Search {
                    query: positional[1].clone(),
                };
            }
            "find" => {
                if positional.len() < 2 {
                    return Err("Missing QUERY argument for 'find' command".to_string());
                }
                parsed.command = Command::Find {
                    query: positional[1].clone(),
                };
            }
            "raw" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'raw' command".to_string());
                }
                parsed.command = Command::Raw {
                    page: positional[1].clone(),
                };
            }
            "render" => {
                if positional.len() < 2 {
                    return Err("Missing FILE argument for 'render' command".to_string());
                }
                parsed.command = Command::Render {
                    file: positional[1].clone(),
                };
            }
            "new" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'new' command".to_string());
                }
                parsed.command = Command::New {
                    page: positional[1].clone(),
                };
            }
            "edit" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'edit' command".to_string());
                }
                parsed.command = Command::Edit {
                    page: positional[1].clone(),
                };
            }
            "delete" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'delete' command".to_string());
                }
                let force = positional.contains(&"--force".to_string());
                parsed.command = Command::Delete {
                    page: positional[1].clone(),
                    force,
                };
            }
            "backlinks" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'backlinks' command".to_string());
                }
                parsed.command = Command::Backlinks {
                    page: positional[1].clone(),
                };
            }
            "links" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'links' command".to_string());
                }
                parsed.command = Command::Links {
                    page: positional[1].clone(),
                };
            }
            "related" => {
                if positional.len() < 2 {
                    return Err("Missing PAGE argument for 'related' command".to_string());
                }
                parsed.command = Command::Related {
                    page: positional[1].clone(),
                };
            }
            "graph" => {
                let mut page = None;
                let mut depth = None;
                let mut format = None;
                let mut j = 1;
                while j < positional.len() {
                    let arg = &positional[j];
                    if arg == "--depth" && j + 1 < positional.len() {
                        depth = positional[j + 1].parse().ok();
                        j += 2;
                    } else if let Some(val) = arg.strip_prefix("--depth=") {
                        depth = val.parse().ok();
                        j += 1;
                    } else if arg == "--format" && j + 1 < positional.len() {
                        format = Some(positional[j + 1].clone());
                        j += 2;
                    } else if let Some(val) = arg.strip_prefix("--format=") {
                        format = Some(val.to_string());
                        j += 1;
                    } else {
                        if page.is_none() && !arg.starts_with('-') {
                            page = Some(arg.clone());
                        }
                        j += 1;
                    }
                }
                parsed.command = Command::Graph {
                    page,
                    depth,
                    format,
                };
            }
            "tags" => {
                let wiki = if positional.len() > 1 {
                    Some(positional[1].clone())
                } else {
                    None
                };
                parsed.command = Command::Tags { wiki };
            }
            "tag" => {
                if positional.len() < 2 {
                    return Err("Missing TAG argument for 'tag' command".to_string());
                }
                parsed.command = Command::Tag {
                    tag: positional[1].clone(),
                };
            }
            "files" => {
                let mut type_filter = None;
                let mut j = 1;
                while j < positional.len() {
                    let arg = &positional[j];
                    if arg == "--type" && j + 1 < positional.len() {
                        type_filter = Some(positional[j + 1].clone());
                        j += 2;
                    } else if let Some(val) = arg.strip_prefix("--type=") {
                        type_filter = Some(val.to_string());
                        j += 1;
                    } else {
                        j += 1;
                    }
                }
                parsed.command = Command::Files { type_filter };
            }
            "query" => {
                if positional.len() < 2 {
                    return Err("Missing QUERY argument for 'query' command".to_string());
                }
                parsed.command = Command::Query {
                    query: positional[1].clone(),
                };
            }
            "wiki" => {
                if positional.len() < 2 {
                    return Err("Missing subcommand for 'wiki'".to_string());
                }
                match positional[1].as_str() {
                    "list" => parsed.command = Command::Wiki(WikiCommand::List),
                    "add" => {
                        if positional.len() < 4 {
                            return Err("Usage: tw wiki add NAME PATH [--default]".to_string());
                        }
                        let default = positional.contains(&"--default".to_string());
                        parsed.command = Command::Wiki(WikiCommand::Add {
                            name: positional[2].clone(),
                            path: positional[3].clone(),
                            default,
                        });
                    }
                    "remove" => {
                        if positional.len() < 3 {
                            return Err("Usage: tw wiki remove NAME".to_string());
                        }
                        parsed.command = Command::Wiki(WikiCommand::Remove {
                            name: positional[2].clone(),
                        });
                    }
                    "rename" => {
                        if positional.len() < 4 {
                            return Err("Usage: tw wiki rename OLD NEW".to_string());
                        }
                        parsed.command = Command::Wiki(WikiCommand::Rename {
                            old: positional[2].clone(),
                            new: positional[3].clone(),
                        });
                    }
                    "mount" => {
                        if positional.len() < 4 {
                            return Err("Usage: tw wiki mount PARENT CHILD".to_string());
                        }
                        parsed.command = Command::Wiki(WikiCommand::Mount {
                            parent: positional[2].clone(),
                            child: positional[3].clone(),
                        });
                    }
                    "unmount" => {
                        if positional.len() < 4 {
                            return Err("Usage: tw wiki unmount PARENT CHILD".to_string());
                        }
                        parsed.command = Command::Wiki(WikiCommand::Unmount {
                            parent: positional[2].clone(),
                            child: positional[3].clone(),
                        });
                    }
                    "default" => {
                        if positional.len() < 3 {
                            return Err("Usage: tw wiki default NAME".to_string());
                        }
                        parsed.command = Command::Wiki(WikiCommand::Default {
                            name: positional[2].clone(),
                        });
                    }
                    _ => return Err(format!("Unknown wiki subcommand: {}", positional[1])),
                }
            }
            "index" => {
                if positional.len() < 2 {
                    return Err("Missing subcommand for 'index'".to_string());
                }
                match positional[1].as_str() {
                    "status" => parsed.command = Command::Index(IndexCommand::Status),
                    "update" => parsed.command = Command::Index(IndexCommand::Update),
                    "rebuild" => parsed.command = Command::Index(IndexCommand::Rebuild),
                    _ => return Err(format!("Unknown index subcommand: {}", positional[1])),
                }
            }
            "lint" => parsed.command = Command::Lint,
            "doctor" => parsed.command = Command::Doctor,
            "stats" => parsed.command = Command::Stats,
            "config" => parsed.command = Command::Config,
            "completions" => {
                if positional.len() < 2 {
                    return Err("Missing SHELL argument for 'completions'".to_string());
                }
                parsed.command = Command::Completions {
                    shell: positional[1].clone(),
                };
            }
            _ => {
                if let Some(wiki_name) = cmd.strip_prefix('@') {
                    // @WIKI [PAGE]
                    let page = if positional.len() > 1 {
                        positional[1].clone()
                    } else {
                        String::new()
                    };
                    parsed.command = Command::Page {
                        wiki: Some(wiki_name.to_string()),
                        page,
                    };
                } else if positional.len() == 1 {
                    // PAGE or WIKI (handled by resolution)
                    parsed.command = Command::Page {
                        wiki: parsed.wiki.clone(),
                        page: cmd.to_string(),
                    };
                } else if positional.len() == 2 {
                    // WIKI PAGE
                    parsed.command = Command::Page {
                        wiki: Some(cmd.to_string()),
                        page: positional[1].clone(),
                    };
                } else {
                    return Err(format!("Unknown command: {}", cmd));
                }
            }
        }

        Ok(parsed)
    }
}
