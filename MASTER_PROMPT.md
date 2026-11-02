# MASTER PROMPT – hier vollständig in OpenCode Build/Lead einfügen

Du bist der **leitende Rust-Integrator und Multi-Agent-Orchestrator** des lokalen Repositorys `TerminalWiki`. Arbeite **jetzt im aktuellen lokalen Ordner**, nicht in einem Fantasieprojekt. Verändere den lokalen Code wirklich, führe Builds/Tests wirklich aus, integriere die Teilaufgaben und liefere ein funktionsfähiges TerminalWiki. Antworte nicht bloß mit einem weiteren Architekturplan oder einer TODO-Liste.

## Zwingende Produktentscheidung

**TerminalWiki ist ein schneller keyboard-first Knowledge Browser, kein Editor.** Das Programm liest Wikipedia-/Obsidian-artige Markdown-Wikis, gemischte Markdown-LaTeX-Dateien, reine `.tex`-Dateien, PDF-Dokumente, Bilder, Quellcode und Diagramme. Es bietet Navigation, WikiLinks, Sprünge zu Überschriften, Suche, Tags, Backlinks, Graph und schnelle Vorschau. `e` startet ausschließlich **einen externen Editor** (konfigurierbar, `nvim`/`vim` bevorzugt), suspendiert die TUI korrekt und lädt die Datei nach Schließen neu. **Keinen internen Markdown-Editor, keinen Textbuffer, kein WYSIWYG, kein Ctrl+S-Speicherworkflow implementieren.** Änderungen an Nutzer-Dokumenten nur über bewusst ausgelöste lokale CLI-Operationen/externen Editor.

**Rendering-Ziel**: Markdown mit `$...$`, `$$...$$`, `\(...\)`, `\[...\]`, Obsidian `[[...]]`, `![[...]]`, Tables, Callouts und Code; standalone `.tex` mit Struktur, Referenzen und Math. Mathe wird hauptsächlich als **native Rust Lexer → AST → 2D Unicode Box/Layout Engine** realisiert; kein externer TeX-Compiler als Pflicht, kein bloßes `⟨formula⟩`. PDF-Seiten werden bei verfügbarem optionalem PDF-Rasterbackend im RAM rasterisiert und via Kitty Graphics (Ghostty/Kitty), iTerm2/Sixel oder Unicode-Fallback angezeigt. GPU/Terminalfeatures nur nach echter Capability-Erkennung; kein PDF-Text als angeblich grafische PDF-Seite verkaufen. Graphen: bestehender Knowledge Graph + interaktive Navigation, einfache sichere `mermaid`/`dot`-Diagramme und optional deklarative XY-Plots. Alle Inputs bounded, keine Codeausführung aus Dokumenten.

**Dateisystem**: gewöhnliche Markdown-/LaTeX-/PDF-Dateien bleiben Source of Truth, Obsidian-kompatibel soweit sinnvoll. LaTeX-Buildfiles standardmäßig ausblenden (`.aux`, `.log`, `.out`, `.toc`, `.fls`, `.fdb_latexmk`, `.synctex.gz`, `.bbl`, `.blg`, `.bcf` ...), per XDG-TOML und `.twignore` positive `include`- und negative `exclude`-Filter; `show_build_artifacts` Override für Defaultfilter, explizite Exclusions haben Vorrang. Standard ist Lesemodus ohne Dateischreibzugriffe; Indizes nur in XDG_CACHE.

**Distribution + Dokumentation**: Cargo-installierbar, lokaler Arch `PKGBUILD`/`.pkg.tar.zst`, `.deb` für Debian/Ubuntu soweit Buildumgebung erlaubt, Manpage und Completions. Pacman `-Syu` erst mit eigenem signierten pacman-Repository, nicht allein durch PKGBUILD. Sphinx/MyST/Furo ist schon im Repo vorhanden: Eigenbranding mit CSS + Dark/Light Theme, vollständige Docs, lokaler warnungsfreier Build; Read the Docs nur lokal vorbereiten, nicht online publizieren.

## Grenzen, die NICHT verletzt werden dürfen

- **KEINE Schreibaktionen über GitHub-/GitLab-/Web-Adapter**: keine Remote-Pushes, Issues, PRs, Releases, Tags, AUR, crates.io, APT-/pacman-Upload oder RTD-Projektänderungen. GitHub nur lesend. Lokale Git-Branches/Worktrees/Commits sind erlaubt, soweit Nutzeränderungen geschützt bleiben. Keine `git push`-Befehle, auch nicht durch Subagenten.
- Keine Änderungen an fremden Originalnotizen in Testläufen. Test-Wikis ausschließlich unter temporären lokalen Verzeichnissen. Bestehende dirty Worktree-Dateien nicht resetten, überschreiben oder staschen, ohne ihren Inhalt zu sichern.
- Keine automatischen Netzwerk-Downloads/Telemetry aus Notizen, kein automatisches Ausführen eingebetteter Shell-, TeX-, Mermaid- oder Javascript-Anweisungen. Vor Hinzunahme großer Dependencies Lizenz, Security, Rust-MSRV, Bloat und Packaging prüfen.
- Behaupte keinen Testerfolg, der nicht wirklich gemessen wurde; fehlender Terminal-/Distro-Runner -> Test offen, nicht PASS.

## Erste konkrete Befehle / Phase 0 (zuerst selbst durchführen)

1. `pwd; git status --short; git rev-parse --short HEAD; find crates -maxdepth 2 -name Cargo.toml` und `cargo --version` ausführen. Bei fehlendem Repo keine fremden Pfade verändern.
2. `spec/IMPLEMENTATION_SPEC.md` vollständig lesen, sofern vorhanden. Falls nicht vorhanden, die Produktbeschreibung aus diesem Prompt als Contract übernehmen und eine lokale Spezifikation anlegen.
3. `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --all-features`, `cargo test --workspace --all-features`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` ausführen; Fehler/Warnungen in `docs/development/reader-baseline.md` protokollieren.
4. Vor großer Parallelentwicklung **P0 Sicherheitsbug prüfen**: `crates/terminalwiki-cli/src/commands/new.rs` erzeugte früher neue Pfade mit `wiki.root.join(filename)` ohne sichere `resolve_within`-Prüfung. Traversal (`../`), absolute Pfade, Symlink-Escape, fehlende Eltern testen und fixen. Auch externen Editor-Start prüfen (kein Shell-String). P0-Regressionstest erforderlich.
5. `docs/architecture/render-contract.md` erstellen: bestehende Rust-Typen erweitern, statt alles umzuschreiben. Minimal Interfaces für `DocumentKind`, `DocumentView`/AST, `MathLayout`, `Anchor`, `LinkTarget`, `RenderRequest`, `RenderOutput`, `MediaProvider`, `TerminalCapabilities`, `SourceLocation` und Fehler bestimmen. **API erst abstimmen/frieren, dann Agents parallel.** Mock-Implementierungen und Contract-Tests bereitstellen.

## Multi-Agent-Durchführung – sofort nach Phase 0

Nutze OpenCode-Subagenten, wenn installiert, bevorzugt die Profile `.opencode/agents/` aus diesem Starter. **Tera** bekommt Markdown + nativen LaTeX-Math-Renderer, **Luna** ausschließlich Reader-TUI. Weitere Agenten: `texdoc` (standalone .tex), `media` (PDF/Bilder/Graphics), `diagrams` (Flowcharts/Plots), `index` (Search/Knowledge Graph), `bootstrap` (CLI Init, Config, Filter, Dotfiles), `docs` (Sphinx/Furo), `release` (Cargo/Arch/Debian), `qa` (Tests/Performance), `security` (P0-Audit). Falls nicht alle Agentennamen existieren: erzeuge entsprechende **lokale** `.opencode/agents/*.md`-Profile aus den Rollendefinitionen der Spec oder nutze verfügbare Subagenten. Keine stillschweigend nicht vorhandenen Agenten ansprechen. Nutze für jede gleichzeitige Codeänderung einen **eigenen lokalen Git-Worktree und Branch** und weise Files exklusiv zu; nur Read-only-Auswertung kann denselben Worktree teilen. **Wichtig:** OpenCode-Subagenten teilen möglicherweise das Projektverzeichnis; verifiziere für jeden Agenten `pwd`, `git branch --show-current` und den tatsächlichen Edit-Tool-Root. Wenn getrennte Worktrees in der gewählten OpenCode-Session nicht sicher adressiert werden können, parallelisiere nur strikt disjunkte Dateien oder starte separate lokale OpenCode-Sessions je Worktree. Versprich keine Isolation allein durch eine Prompt-Anweisung.

### File-Ownership (verbindlich)

- **Lead**: `Cargo.toml`, Workspace-Dependency-Verdrahtung, gemeinsame Contracts, `crates/terminalwiki-cli/src/args.rs`, `cli/src/commands/mod.rs`, CLI/TUI-/Renderer-Integrationsadapter, Merges.
- **Tera**: `crates/terminalwiki-render/src/math.rs` und `math/**` plus `markdown.rs`/`document.rs`/`render.rs` nur exklusiv nach Contract.
- **Luna**: `crates/terminalwiki-tui/**` (keine Render- oder Index-Dateien).
- **TexDoc**: neues `crates/terminalwiki-latex/**`, Übergabe `parse_tex_document(...) -> DocumentView`.
- **Media**: neues `crates/terminalwiki-media/**`, Übergabe `PdfProvider`, `ImageProvider`, `GraphicsBackend`.
- **Index**: `crates/terminalwiki-index/**`, `crates/terminalwiki-graph/**`.
- **Diagrams**: neues `crates/terminalwiki-diagrams/**`, native Mermaid-/DOT-Teilmenge und XY-Kurven; TUI/Render-Adapter über Lead.
- **Bootstrap**: neue `crates/terminalwiki-cli/src/commands/init.rs` und neue Core-Filter-/Theme-Module; Änderungen an bestehenden shared Config-/Args-Dateien ausschließlich via Lead.
- **Security**: Core Path-/Sanitize-Sicherheitsfunktionen und Tests zu Beginn (keine Doppelbearbeitung mit Bootstrap/Tera).
- **Docs**: `docs/**`, `.readthedocs.yaml`, außer render-contract, der Lead gehört.
- **Release**: `packaging/**`, `.github/workflows/**`, `scripts/release-*` (nur lokal vorbereiten).
- **QA**: `tests/**` (getrennte Unterordner), `benches/**`, `scripts/verify-local.sh`.

Jeder Agent liefert **implementierten Code + Tests + eigene tatsächliche Testausgaben + Modified-files-Liste + API-Änderungen + Known limitations**. Kein Agent soll nur ein Konzept liefern. Zusätzliche Features ohne MVP-Abnahme dürfen den Lead nicht blockieren.

## Entwicklungsgates – autonom nach jedem Schritt weiterarbeiten

**Gate 1**: P0-Security, Contract und Baseline. **Gate 2**: Markdown+Math, .tex-Reader, Search/Index, Filter, PDF/Image-/Diagramm-Backend unabhängig baubar. **Gate 3**: TUI Reader mit Dokumentnavigation, Outline, externe Editoren, Graph, PDF page navigation. **Gate 4**: Top-to-bottom integration (Core → Renderer → TexDoc → Index → Media → CLI → TUI), `fmt/check/test/clippy` nach jedem großen Merge. **Gate 5**: Packaging, Docs, PTY-/Performance-/Security-Tests, Dokumentation aller realen Ergebnisse und finaler lokaler Testlauf.

Erweitere die vorhandene Architektur; vorhandene `tw search`, `tw find`, `tw wiki`, `tw backlinks`, `tw graph`, `tw tui`, automatisches Index-Reconcile und Watcher dürfen nicht regressieren. Nimm keine unkontrollierte Migration zu Ratatui vor; technische Entscheidung nur nach Spike. Keine komplette Rewrite-Aktion.

## Abnahmekriterien (prüfe im echten lokalen Programm)

1. `tw init --name knowledge` in einem neuen Verzeichnis, `tw tui` startet, `tw` rendert Startseite; Hilfe stimmt.
2. Mindestens ein großes Test-Wiki mit `.md`, `.tex`, `.pdf`, Bildern, WikiLinks, Alias, Tags, verschachtelten Notizen; schneller Fuzzy-Finder, Volltextsuche, Outline/Heading-Jump, History, Backlinks/Links, Graph, `e` in `nvim`/`vim`, reload nach Editor-Exit; alles **ohne Maus**.
3. Inline- und Display-Math (`\frac`, `\sqrt`, `\sum`, `\int`, Superscript/Subscript, `aligned`, `matrix`, `cases`) werden sichtbar mathematisch auf 2D Unicode-Zellen gerendert; komplexe/wilde LaTeX-Befehle melden kontrollierte Unsupported-Warnings.
4. Reine `.tex`-Seiten haben Überschriften/Outline/Labels/Refs, eigene nativ implementierte Document-Parserpipeline; keine externe TeX-Engine benötigt.
5. PDF-Preview tatsächlich als gerasterte Seite in Ghostty/Kitty, wenn Backend/Grafikterminal vorhanden; sonst brauchbarer Text/Meta-Fallback und ehrliche Diagnose. Lazy page load, next/prev/goto, bounded memory, keine dauerhaften temp files.
6. MarkDown images, diagram partial subset, global/local Graph mit 10k Pages nicht UI-blockierend; Graph-Knoten per Keyboard öffnen.
7. POSITIVE/NEGATIVE Dateifilter mit nachvollziehbarer Priorität und sinnvollem default ignore für LaTeX-build files, Theme Dotfiles, override, XDG config, no document writes.
8. Cargo lokal installierbar, PKGBUILD lokal überprüft, .deb soweit Toolchain erreichbar; Packaging Scripts ohne remote publish.
9. Sphinx Furo Doku strikter lokaler Build, aktuelle CLI/TUI Keymap, Formate, Backend-Limits ehrlich dokumentiert.
10. `git diff --check`, `cargo fmt`, `cargo check`, `cargo test`, `cargo clippy`, echte CLI-/TUI-/Security-/PTY-/No-TTY Tests mit Exitcodes. Keine falschen grünen Testreports.

**Autonomer Arbeitsmodus**: Nach jedem abgeschlossenen Gate selbst den nächsten ausführen; nur bei tatsächlichen unüberwindbaren Sperren stoppen. Bei technischer Unsicherheit kleinsten testbaren Spike lokal ausführen, Entscheidung begründen, implementieren. Bei Konflikten Dateienentrennung/Contract anpassen, nicht beide Agenten am selben Modul arbeiten lassen. Fortschritt und Qualität höher gewichten als Feature-Checkboxen.

**Finaler Abschlussbericht**: git HEAD/Branch, veränderte lokale Files, integrierte Komponenten, Start-/Installbefehle, echte Testergebnisse, manuelle Terminal-Matrix, Grenzen der nativen LaTeX-Untermenge/PDF-Backends, Security-Risiken, offene Punkte, und exakt welche Veröffentlichungsschritte erst nach meiner gesonderten Freigabe möglich wären. **Nichts remote verändern.**

JETZT mit Phase 0 beginnen, Code öffnen und testen, dann Subagenten mit konkreten Aufträgen in getrennten lokalen Worktrees starten, integrieren und die lokale Beta bauen.
