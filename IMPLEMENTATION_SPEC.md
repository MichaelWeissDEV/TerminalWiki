# TerminalWiki – verbindliche Produktspezifikation und Implementierungsaufträge

**Stand:** 2026-11-02  | **Ziel:** brauchbarer TerminalWiki-Reader als v0.2.0-beta-Kandidat, kein vollwertiger Editor

## 0. Interpretation / oberste Regeln

TerminalWiki ist ein **local-first, keyboard-first, read-mostly Knowledge Browser**, der normale Dateien in einem Wiki-Verzeichnis als Source of Truth behandelt. Die TUI ist *kein* WYSIWYG- und *kein* Texteditor. Erstellen, Ändern und Löschen von Inhalten erfolgt über lokale CLI-Aktionen oder einen externen Editor (`nvim`, `vim`, `$VISUAL`, `$EDITOR`), nie über einen eingebauten Editiermodus. Der Nutzer kann Dokumente ansehen, suchen, zwischen ihnen und ihren Überschriften springen, Links folgen, Graphen ansehen und aus der TUI kurz in den Editor wechseln. Danach wird die Datei neu geladen und der Index aktualisiert. Keine versteckte Cloud, keine Accountpflicht, kein Service, kein obligatorischer PDF-/TeX-Daemon.

**Nicht automatisch in GitHub schreiben**: keine Pushes, Remote-Branches, GitHub Issues, PRs, Tags, Releases oder sonstige Remote-Mutationen. Kein Publizieren zu crates.io, AUR, Read the Docs, APT-/Pacman-Repositories. Lokale Source-Änderungen und lokale Tests sind erwünscht. Die bestehenden Dateien des Nutzers, insbesondere nicht committete Änderungen, dürfen nicht überschrieben werden.

## 1. Verifizierter Ausgangspunkt – vor Arbeitsbeginn durch lokalen Clone prüfen

Repository: `MichaelWeissDEV/TerminalWiki`, Branch beim Review `master`, SHA `19ed03cf5fa1...` (2026-08-15). Der Klon kann inzwischen neuer sein: **aktuellen lokalen HEAD bestimmen, Annahmen anpassen, keine Hardcoded-Versionen**.

Bestehende Crates:
- `terminalwiki-core`: Wiki-Roots, Config-Layers, Dateisystem/Links, Watcher, Sanitization.
- `terminalwiki-cli`: Binär-CLI `tw`, Registrierung, `new`/`edit`, Search, etc.
- `terminalwiki-render`: Markdown AST, `pulldown-cmark` mit `ENABLE_MATH`, aber `math.rs` ist nur Stub. `render.rs` gibt Formeln als `⟨...⟩` aus; Bilder als Platzhalter.
- `terminalwiki-index`: Tantivy-Suche/Index und Fuzzy; Semantik/Snippets noch teils rudimentär.
- `terminalwiki-graph`: Graphmodell und Unicode-Layout.
- `terminalwiki-tui`: crossterm-basierte Zeichnung und Eventloop, Finder, Backlinks, Outline, Graph und Watcher.
- Sphinx/Read the Docs existiert und verwendet bereits Furo, aber Build war im dokumentierten Testlauf nicht ausgeführt.

**P0-Sicherheitsprüfung**: `crates/terminalwiki-cli/src/commands/new.rs` verwendet im früheren Stand direkte `wiki.root.join(filename)`-Bildung und schreibt dann; prüfen, ob Pfad-/Symlink-Ausbrüche möglich sind. Bestehende zentrale `Wiki::resolve_within`/Containment nutzen und fehlende Pfade über existierende Eltern sicher prüfen. Racebedingungen bei Erstellen/Umbenennen und externe Editoren ebenfalls beachten. Niemals als vermeintlich fertigen Fix deklarieren ohne Regressionstest.

**P0-Qualitätsprüfung**: Baseline `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `sphinx-build -n -W -b html docs docs/_build/html`. Ergebnisse notieren, nicht aus älteren STATUS-Dateien übernehmen.

## 2. Produktumfang und Akzeptanzszenarien

### 2.1 Erststart

Nach Installation:

```
mkdir -p ~/Knowledge
cd ~/Knowledge
tw init --name knowledge
tw tui
```

`tw init`: erzeugt auf Wunsch `index.md`, `.terminalwiki.toml` und optional `.twignore`, registriert ein bestehendes Wiki als Default, überschreibt niemals Dateien ohne expliziten Force-Schalter, funktioniert in neuen und existierenden Verzeichnissen. `tw init --help` ist selbsterklärend. Optional global `tw wiki add ...` bleibt abwärtskompatibel. Start ohne Wiki: Fehlermeldung mit konkretem `tw init`-Beispiel; wenn `cwd` ein Wiki ist, automatisch erkennen (wählbarer Modus).

`tw` darf als schneller Pager/Default-Wiki-Einstieg funktionieren; `tw tui` bleibt expliziter, stabiler Start. Benutzerseitige Dokumentdateien dürfen **niemals** beim Lesen verändert werden. Indizes dürfen nur in explizit konfiguriertem Cache/XDG-Laufzeitverzeichnis liegen.

### 2.2 Navigation – Wikipedia/Obsidian im Terminal

Verpflichtend: Verzeichnisbaum, aktuelle Seite, Outline/H2-H6, Backlinks/Outgoing Links, fuzzy Finder, Volltextsuche, Seite-vor/zurück, Linkliste, anklickbare Links **per Tastatur**, Semantik von Ankern, Tabs/Pane-Fokus, direkter Sprung zur Überschrift, Suchtreffer mit Snippets und Markierung, deterministisches Page-Resolution bei Namenskonflikten, `[[Wiki]]`-Links, `[[Page#Heading|Alias]]`, `[[#Heading]]`, gewöhnliche Markdown-Links, `#tags`, YAML aliases und Frontmatter.

Das aktive Dokument soll bei Selektion im Explorer sofort in der Ansicht erscheinen; Suche/Treffer dürfen den Lesestand nicht verlieren, wenn die Ansicht nur als Vorschau wechselt. Öffnen eines Treffers speichert die Navigationshistory. Modale Overlays sind bedienbar, abbrechbar und besitzen konsistenten Fokus. Graph-View zeigt lokale Nachbarschaft und globales Wiki mit Auswahl, Zoom/Stufen, fokussiertem Zentrum, performanter Top-N-Auswahl und Sprung zum Knoten. Graph ist kein blockierender Startbildschirm.

Externe Links nur nach expliziter Bestätigung über sicheren konfigurierbaren Opener öffnen; keine Shell-String-Interpolation. Alle Interaktionen ohne Maus; Mouse-Events optional und standardmäßig deaktivierbar.

### 2.3 Inhaltstypen – gemeinsame Semantik

- Markdown `.md`, `.markdown`: GFM-Tabellen, Checkboxen, Code-Fences, Callouts `> [!NOTE]`, Blockquotes, Footnotes, YAML Frontmatter, normales `![alt](path)`, `![[asset]]`, `![[note#heading]]` (begrenzte Embeds), Unicode.
- Gemischte mathematische Syntax in Markdown: `$inline$`, `$$display$$`, `\(inline\)`, `\[display\]`, optional `\begin{equation}` etc. Syntax in Code-Fences, Inline-Code und escaped Dollarzeichen darf **nicht** interpretiert werden.
- Reines LaTeX `.tex`, `.latex`: `\section`/`\subsection`/`\paragraph`/`\label`/`\ref`/`\eqref`/`\begin{document}`/`\title`/`\author`/`\emph`/`\textbf`/`\texttt`/`\href`/`\url`/`\includegraphics`/Listen/Zitate/Code, Gleichungsumgebungen, `\input`/`\include` nur innerhalb Wiki-Root und mit Rekursionslimit. Ziel ist *Dokumentenlektüre*, nicht vollständiges TeX-Paket- oder Makroexecution-Emulieren. Unbekannte Befehle bleiben lesbar und erzeugen strukturierte Warnungen statt Crash oder stiller Datenvernichtung.
- PDF `.pdf`: Seitenweise Vorschau, Navigation (`next/prev page`, `go to page`, Seitenübersicht), Metadaten, PDF-Outline falls verfügbar, Textsuche falls verfügbar, für Bilder Ghostty/Kitty. Verschlüsselte PDFs: höfliche Fehlermeldung oder ausdrücklich angefragte Passwort-Session, keine stille Passwortspeicherung.
- Bilder `.png`, `.jpg`, `.webp`, `.gif` (erste Frame), `.svg` (kontrolliert gerastert), eingebettete Bilder aus Markdown/LaTeX; Textterminal-Alternativen.
- Quellcode/Text: bisherige Highlight-/Line-Numbers-Funktionen erhalten.
- Diagramme/Graphen: existierender Knowledge Graph, optional fenced `mermaid` **klar definierte Teilmenge** (vorzugsweise flowchart), fenced `dot` nur sichere Teilmenge ohne Ausführung externer Befehle; optional `xyplot`-DSL für Funktions- und Messkurven. Unbekannte Diagrammsyntax bleibt als lesbarer Quelltext erhalten.

## 3. Architektur – strikt eine Renderpipeline, keine doppelte Logik

Ziel-Fluss:

```
file/wiki path -> secure classifier -> content reader
     |-- Markdown AST (vorhanden, erweitern)
     |-- LaTeX document AST (neues optionales Modul/Crate)
     |-- PDF document/page provider (neues Modul/Crate)
     |-- image/media provider
           -> gemeinsamer DocumentView-IR + Anchors + Links + SourceLoc
           -> Layout(width, theme, capabilities)
           -> text/ANSI/crossterm cells + optional graphics placements
           -> CLI pager UND TUI (identische Semantik)
```

**Bestehende AST-/RenderedDocument-Typen inkrementell erweitern, nicht blind ersetzen.** Ein ADR muss explizit dokumentieren, welche Typen im `terminalwiki-render::document` bleiben und welche neuen optionalen Interfaces bereitgestellt werden. Unerlässliche Verträge:

```
DocumentId { wiki, relative_path }
SourceLocation { file, byte_start, byte_end, line, column } // optional, nicht alle Backends haben Quelloffsets
AnchorId / LinkTarget / NavigationTarget
DocumentKind { Markdown, Latex, Pdf, Image, Code, Text, Unknown }
DocumentView { title, blocks, anchors, links, outline, warnings, assets }
RenderBlock { Text, Heading, Code, Math, Table, Image, Diagram, PdfPage, Unsupported }
RenderRequest { width_cells, height_cells, theme, terminal_caps, content_revision }
RenderOutput { text_cells, graphics_placements, links_hit_regions, visible_anchors }
```

Die obigen Namen sind API-Vorschläge, keine Forderung nach einem Big-Bang-Rewrite. Der Lead definiert eine minimal invasive Version, friert sie vor paralleler Implementierung in `docs/architecture/render-contract.md` ein und kommuniziert Contract-Änderungen. **In einer PR darf es nicht zwei unterschiedliche Systeme zur Linkauflösung, Schriftbreite, Zeilenumbrüchen oder Pfadsicherheit geben.**

Rendering von mathematischen und bildbasierten Blöcken benötigt Größenangaben in Terminalzellen, nicht allein Strings. Cursor-/Hitbox-Mapping bei Umbruch, TUI-Resize, Source-Positionen und Link-Ankern muss deshalb bereits im Contract vorgesehen werden. CLI ohne TTY / `--plain` muss ausschließlich sichere Texte und keine Grafikeskapes ausgeben.

## 4. Rust-nativer LaTeX-Math-Renderer (Agent Tera)

**Kein `latex`, `pdflatex`, `xelatex`, `lualatex`, `tectonic`, KaTeX oder Browser-Rendering als Pflichtbackend.** Native Rust-Tokenizer/AST/Box-Layout sind primär. Nutze Bibliotheken nur, wenn sie die Semantik sicher und messbar besser umsetzen, ohne eine TeX-Distribution/JavaScript-Runtime; bevor Abhängigkeiten eingeführt werden, Lizenzen/Größe/Vollständigkeit prüfen. `latex2mathml` ist ein mögliches Referenz-/Parser-Werkzeug, aber MathML allein ist keine Terminal-Darstellung; ein tatsächliches Unicode-/Zellen-Layout muss entstehen.

Implementiere:
1. Lexer für Befehle, Groups, Math-Modi, Escapes, Superscripts/Subscripts, Optional-Argumente. Begrenze Eingabegröße, Tokenanzahl, AST-Tiefe, Rekursion/Makroexpansion.
2. Math-AST (symbol/number/group/fraction/root/fence/supsub/operator/accent/matrix/alignment/text/unknown) mit Quelloffsets soweit möglich.
3. Unicode-Konvertierung von Greek, Relationen, Mengen, Pfeilen, Operatoren (inkl. `\mathbb`, `\mathbf` graceful fallback) und textmode.
4. Layout als **2D Cell/Box**: `width`, `height`, `baseline`, Kinderpositionen; Bruchstriche, Wurzeln, Grenzen bei `\sum`/`\int`, tiefgestellte/hochgestellte Elemente, Klammern, Matrizen `bmatrix`, `pmatrix`, `cases`, `aligned`/`align`, Zeilenumbruch in enger Ansicht.
5. `render_math_inline` und `render_math_display`, Rückgabewert als Zeilen+Größen/Hittest oder renderfähiger Span/Block, niemals bloß `format!("⟨{formula}⟩")` als finaler Renderer.
6. Cache: in-memory bounded LRU nach Formelhash, Breite, Style und Backend; keine obligatorischen temporären Dateien.
7. Text-Terminal: Unicode-Layout, nicht verlustfreie typographische TeX-Ausgabe versprechen. Optional höherwertiger grafischer Math-Backend über Rust-font/text-rasterisation, sofern Font-Lizenzen, Assetbudget und Bildprotokoll geklärt; keine Pflicht für Beta.
8. Fehler: unbekannte Makros sichtbar mit Warning + Source-Snippet, keine Panics, keine ANSI-Injektion.

Golden Cases mit exakter sichtbarer Ausgabe/Snapshot + Dimensionsinvarianten: `E=mc^2`, `\frac{a+b}{c}`, `\sqrt{x^2+1}`, `\sum_{i=1}^{n} i^2`, `\int_0^1 x^2\,dx`, `\alpha \le \beta`, `\begin{bmatrix}1&2\\3&4\end{bmatrix}`, `\begin{cases}x&x>0\\-x&x\le0\end{cases}`, Alignment, defekte `{`/`}`-Klammern, unbekannte Befehle, 5000-Tokens-Bomben. Tests auch für enge Widths, non-BMP, breite CJK-Grapheme.

## 5. LaTeX-Dokumentlektüre (Agent TexDoc)

**Trennung Math vs. Dokumentstruktur**: `terminalwiki-latex` (oder `terminalwiki-render::latex_document`) parst `.tex` zu denselben Navigationsknoten wie Markdown. Unterstützt Abschnittshierarchie, Textabsätze, Textformatierung, Zitate, Listen, Verweise, Bilder, tabular (begrenzte Spalten-/Layout-Untermenge) und Matheblöcke. Aus einem `\label` wird ein Anchor; `\ref` verweist auf das Ziel und die TUI kann folgen. `\input` und `\include` dürfen nur lokale Dateien aus der Wiki-Root öffnen (konfigurierbar: default eingeschränkt, max 8 Tiefen), mit Zykluserkennung; keine Netzwerkzugriffe, kein Shell-Escape, niemals `\write18` o. ä. ausführen. Unbekannte Pakete/Befehle als Warning und Quelltext sichtbar lassen. `\begin{tikzpicture}` niemals automatisch ausführen; sichtbar lassen bzw. optional sicheren Stub anzeigen.

Beim Verlassen des `.tex`-Dokuments sind Outline, Cross-Refs, Suchtreffer und History genauso funktionsfähig wie für Markdown. Für reines LaTeX muss klar zwischen **strukturiertem nativen Reader** und **authentisch kompiliertem PDF** unterschieden werden. PDF-Komplexität bleibt optionale separate Pipeline; eine spätere `tw build`-Funktion ist ausdrücklich *nicht* Teil dieses Pflichtumfangs.

## 6. PDF-/Bildansicht und Terminalprotokolle (Agent Media)

Ghostty und Kitty beherrschen Kitty Graphics Protocol. Das ist **Rasterbildübertragung**, keine unmittelbare PDF-Unterstützung. PDF: Seiten als Bitmaps auf Benutzeranforderung rasterisieren, im Viewport anzeigen, Text/Outline getrennt halten, keine Endloskonvertierung des ganzen PDFs.

Implementierungsarchitektur:
- `PdfProvider` trait: `open`, `page_count`, `metadata`, `page_outline`, `extract_text(page)` soweit verfügbar, `raster_page(page, max_pixel_budget)` -> `RGBA/PNG bytes` im Speicher. Fehler bei fehlendem Renderer klar melden.
- Erstes Backend: **optionales** PDFium (`pdfium-render` + dynamisch auflösbare PDFium-Library) ODER optional Poppler-Subprocess (`pdftoppm` und `pdftotext` mit stdout/in-memory Pipe). Kein stilles Herunterladen von PDFium, keine Einbettung fremder Binärdateien ohne Lizenzprüfung. Bei Wahl des Backends Abhängigkeitsmatrix/Installation dokumentieren.
- Fallback 1: PDF-Text/Metadaten über vorhandene verfügbare Backend-Funktionen; Fallback 2: Metadaten + Hinweis `tw doctor`/Paketinstallation; `pdftotext` allein erzeugt keine grafische Seite.
- `GraphicsBackend` im Terminal: Kitty Graphics (Ghostty/Kitty), optional iTerm2, Sixel, Unicode half-block (Bilder als Textzellen), Off. Keine unbewiesenen Terminal-Fähigkeiten behaupten; bounded Probe mit Timeout/TTY-Anforderung und explicit forced override. In SSH/tmux nur nach sicherem Cap-Check bzw. Opt-in; bei schlechtem Probe-Ergebnis Fallback. Korrekte Delete-/Placement-/Resize-Semantik ohne Artefakte, keine unendlichen Escape-Streams.
- Bilddecoder mit Größenlimits, Seitenlimit, Downsampling, Worker-Thread/Cancel-Tokens und Generation-ID: Öffnen anderer Seiten führt nicht zum Anzeigen stale Ergebnisse; das Rendern darf die Navigation nicht blockieren.
- PDFs mit 100+ Seiten: Lazy-Loading, prefetch höchstens Nachbarseiten, LRU mit messbarem RAM-Limit, Thumbnail nur bei Bedarf. Feste Obergrenze für Dekodierpixels und Speicher.
- PDF-Link `[[papers/example.pdf#page=12]]` öffnet Seite 12; `![[papers/example.pdf#page=12]]` optional Inline-Vorschau; bei normalem Terminal fallback `[PDF: …, Seite 12]`.
- Erkennung und Vermeidung von Grafikartefakten nach TUI-Dokumentwechsel und Modals. CLI-/Pipe-Modus darf niemals Binär-/Grafikcontrolsequences ausgeben, außer explizit angefordert und TTY vorhanden.

**Quellen/Referenzen (nur zum Evaluieren):** https://sw.kovidgoyal.net/kitty/graphics-protocol/ ; https://ghostty.org/docs/features ; https://docs.rs/pdfium-render/latest/pdfium_render/ ; https://docs.rs/ratatui-image/latest/ratatui_image/ . `ratatui-image` setzt Ratatui voraus: *nicht* die ganze existierende crossterm-TUI allein wegen dieses Crates migrieren, bevor ein Spike Nutzen und Kosten belegt.

## 7. Knowledge Graph, Diagramme, Plots (Agent IndexGraph + Media/Tera Contract)

**Knowledge Graph**: vorhandenes `terminalwiki-graph` wiederverwenden. Skalierbarkeit: incremental graph cache/invalidation, BFS-level neighborhood, node/edge budget, Layout nicht bei jedem Frame, graph modes `local`, `global`, `related`, Suche innerhalb des Graphen, aktive Knoten per Tasten auswählen/öffnen, `+/-` Tiefe ändern, `r` Recompute, Escape zurück, `?` Hilfe. `g` öffnet Graph bereits im vorhandenen Stand; Konsistenz der Keymaps herstellen. Für 10k Seiten muss `global` nie alle Knoten in O(N²) layouten: sampling, top-degree filter, clusters/pagination und sichtbare Trunkierung. Keine irreführende Vollständigkeit.

**Diagramme**: in Markdown fenced `mermaid` (klar dokumentierter `flowchart`/`graph`-Subset) und `dot` (sicherer Parser-Subset) als Knoten/Kanten-IR. Optional `xyplot` als rein deklarative Kurvenspezifikation (kein Ausführen von JS, Python, Lua oder beliebigen Mathe-Ausdrücken), gezeichnet mit Unicode Braille/Boxdrawing oder rasterisiert in Rust für grafikfähige Terminals. Diagramm- und Wissensgraph-Renderer teilen Layout-Budget/Canvas-/Viewport-Adapter; nicht den gleichen Parser erzwingen. Unbekannte/komplexe Syntax bleibt als Source-Fence sichtbar, nie still verschwinden. Tests gegen Parser-Bomben/Giganten-Graphen, zyklische Layouts, Unicode, 40x15 Viewport.

## 8. Bedienkonzept – vollständig keyboard-first (Agent Luna)

**Keine Maus erforderlich.** Tastenkonflikte mit bestehender App vor Änderung dokumentieren; pro Mode gilt eine eindeutige Bindung. Vorschlag:

| Tastenkürzel | Normal-Modus | Abweichungen in Dialogen |
|---|---|---|
| `j/k`, Pfeile | Scrollen | Auswahlliste auf/ab |
| `Ctrl+d/u`, `PgDn/PgUp` | Seitenweise scrollen | – |
| `g g`, `G` | Anfang / Ende | `g` allein kann Graph öffnen, wenn erhalten; Alternative `Home` für Anfang |
| `f` / `Ctrl+p` | Fuzzy-Finder | Fuzzy-Dropdown |
| `/` | Suche in aktueller Seite | `n/N` nächster/vorheriger Treffer |
| `s` | globale Suche | Suchmodal |
| `o` | Outline | Enter springt zur Heading |
| `b` | Backlinks | Enter öffnet Backlink |
| `Tab`/`Shift+Tab` | nächsten/vorherigen sichtbaren Link | Fokuswechsel explizit per `Ctrl+Tab` falls erforderlich |
| `Enter` | gewählten Link öffnen | Ergebnis/Node öffnen |
| `Ctrl+o`/`Ctrl+i` | Navigation zurück/vor | – |
| `e` | Datei in `$VISUAL`/`$EDITOR`/`nvim`/`vim` bearbeiten | TUI suspend/resume |
| `g` | Graph öffnen (bestehender Stand) | `+/-` Tiefe, Enter Knoten |
| `h` oder `Ctrl+h` | Explorer ein-/ausblenden | bei Kollision explizit anders definieren |
| `p` | PDF-Seitenmodus | `[`, `]`, `gg`, `G` für Seite |
| `t` | Tags | – |
| `:` | Command Palette | `:open`, `:wiki`, `:config`, `:graph`, `:help` |
| `?` | Hilfe | – |
| `q`, `Esc` | Dialog zurück / verlassen | wiederherstellen statt neu starten |

**Entscheidungen vor Implementierung**: `(g)` als existierendes Graph-Shortcut behalten, `Home` für Dokumentanfang; `h/l` bisher Horizontal-Scroll und dürfen nicht still für Explorer geopfert werden; definierte sichere Alternative. `Ctrl+N` nicht für internen Editor vorsehen. `Ctrl+S` ist kein Speichern in der TUI, da Reader! Alles dokumentieren. TUI nicht zwingend zu Ratatui migrieren – bestehendes crossterm-Modell kann erhalten werden, aber Einbau sauberer `Viewport`, `Pane`, `Focus`, `Overlay`, `LinkHitbox`, `GraphicsPlacement`-Abstraktionen prüfen. 40x12, 80x24, 120x35, 240x60 testen. Zählung / Breite Grapheme-aware via bestehende Unicode utilities; kein horizontaler Overflow in textmode. Externen Editor terminal state garantiert per RAII zurücksetzen auch bei Exit/Fehler; Editor-Kommando als argv tokenisieren, nie Shell-Strings ausführen; `nvim -u config` muss als sichere argv-Struktur funktionieren, Pfad mit Leerzeichen ebenso.

## 9. Dotfiles, Theme, Filter (Agent Bootstrap / Lead)

Konfigurationsschichtung aus vorhandener Rust-Config übernehmen: `defaults < global < wiki-local < env < CLI`. Vorgeschlagener Standard: `$XDG_CONFIG_HOME/terminalwiki/config.toml`, sonst `~/.config/terminalwiki/config.toml`. Themes: `$XDG_CONFIG_HOME/terminalwiki/themes/<name>.toml`; konfigurierbare lokale Themes nur als Daten lesen, keine Plugins oder Skripte. **Keine automatische globale Konfigurationsüberschreibung** bei `tw init`; nur registrierende Config-Mutation mit Validierung.

Beispiel (`examples/config.toml` separat):

```
[ui]
layout = "reader"  # reader, split, minimal
sidebar = true
outline = true
backlinks = true
mouse = false

[theme]
name = "terminalwiki-dark"

[render]
math = "native"               # native, source, off
images = "auto"               # auto, kitty, iterm2, sixel, unicode, off
pdf = "auto"                  # auto, pdfium, poppler, text, off
code_highlight = true
max_content_width = 110

[files]
show_hidden = false
show_build_artifacts = false
include = ["**/*.md", "**/*.markdown", "**/*.tex", "**/*.pdf", "**/*.png", "**/*.jpg", "**/*.svg"]
exclude = ["**/.git/**", "**/node_modules/**", "**/target/**"]

[editor]
command = ["nvim"]
```

Dieses Schema ist **Zielentwurf**: vorhandene `Config`-Keys beim Implementieren nicht brechen; Migration/Deprecation testen. `include` und `exclude` sollen explizit positive/negative Filter erlauben. Defaults: Latex-Build-Artefakte `.aux`, `.log`, `.out`, `.toc`, `.lof`, `.lot`, `.fls`, `.fdb_latexmk`, `.synctex.gz`, `.bcf`, `.run.xml`, `.bbl`, `.blg`, generierte Output-Verzeichnisse und `target/` ausblenden, **auch aus Suche/Indexer/Explorer**. Quelle `.twignore`, `.gitignore`, globale und lokale Config vereinheitlichen (ein kanonischer Filterdienst statt drei gegensätzlichen). Dokumentierte Priorität: Sicherheits-Containment > explizite `exclude`/ignore > `include` > optionale show-hidden/show-build-artifacts-Flags > Defaults; je nach Implementationsentscheidung muss `show_build_artifacts=true` Default-Exclusion übersteuern können, nicht explizite Nutzer-Exclude. `tw files --all`, `--include`, `--exclude`, `tw index rebuild`, `tw config explain <path>` als UX-Ziele; CLI kann mit stabiler kleinerer Oberfläche beginnen.

Die verzeichnisbasierten Wikis müssen immer ohne TerminalWiki nutzbar sein, inkl. Obsidian. Nicht ohne Zustimmung massenhaft umbenennen, Markdown umschreiben oder Frontmatter automatisch formatieren.

## 10. Performance / Qualität / Sicherheit (Agent QA + Security)

- **Sicherheit:** Root confinement für jede Dateioperation inkl. .tex `\input`, Markdown embeds, Symlink-Ziele, config-include, PDF assets; kanonischer Root, keine Schreibpfade außerhalb. Keine URLs/remote assets automatisch laden. ANSI/OSC/CSI/APC-Escapes aus Dateiinhalten immer sanitizen; durch Grafikprotokoll generierte Sequenzen ausschließlich über vertrauenswürdigen Backend-Writer. Keine Ausführung von LaTeX-Befehlen oder Mermaid-JS. PDF-Dekoder als potenziell angreifbare Schnittstelle behandeln; begrenzen und in `doctor` backend version zeigen.
- **Robustheit:** malformed UTF-8, fehlende Dateien, Broken Symlinks, atomare Editor-Saves, Rename-Bursts, verschwundene PDFs, invalid Header, 100MB Parser-Bomben, zip-bomb-ähnliche Bilder, Config-Syntaxfehler, widersprüchliche Filter.
- **Nebensächlicher Disk-I/O:** Quelle nur lesen, Index in XDG_CACHE, keine tempfile-per-formula. PDF-Backend in-memory, wenn möglich; falls technisch zwingend Tempdateien, restriktiv, sichere Tmpdir, Reinigung und Opt-in dokumentieren. Keine periodischen FSyncs beim Leser.
- **Korrektheit:** gleiche Such-/Linkauflösung zwischen CLI/TUI und MD/TEX; Markdown Code-Fence darf kein Math/Wiki-Link auslösen; PDF page id sauber validieren; Rendering in Pipes gibt keine Kitty-Escape-Folgen aus.
- **Tests:** Unit, Integration, Golden Render/Snapshot, PTY/Terminal Escape Tests, Fuzz-Targets für Lexer/Links/Math/PDF metadata/Graph parsing, Property-Tests für no-panic und keine Root-Escapes, 10k-Seiten Fixture, Benchmark-basierte Aussagen mit Umgebung. Testcalls dokumentieren, **keine behaupteten grünen Tests ohne echten Lauf**.
- **Messbare Bench-Ziele, nicht fälschlich absolute Versprechen:** nach Warmstart Finder und UI-Keypress p95 < 50ms auf Referenzgerät, 1000-Seiten-Fulltext p95 < 100ms, Graph lokal interaktiv < 200ms, PDF-Render asynchron, Peak-RAM pro PDF-Preview konfigurierbar. Jeder Benchmark zeigt Gerät, Dokumentanzahl, Indexwarmheit und Testmethodik. Keine Eingabe durch Vollindexierung/Rendering blockieren.
- **Accessibility:** Kontrastprofile, monochrome Unterstützung, `NO_COLOR`, Tastatur-Hilfe, Text-Fallback für Bilder/Diagramme, Unicode-Breiten, deutsche Sonderzeichen/CJK, schmale Terminals.

## 11. Agentenorganisation und Dateieigentum

**Lead / Integrator:** Eigentümer aller zentralen Cargo-/CLI-/Render-Adapter-Verdrahtungen, `docs/architecture/render-contract.md`, Config-Layer-Migrationsentscheidung, Integrations- und Review-Gates. Arbeitet zuerst allein am Contract; delegiert anschließend. Niemals zwei Agenten auf derselben Datei im gleichen Worktree. Jeder Agent implementiert in eigenem **lokalen** Worktree/Branch, oder in strikt zugewiesenem Modul, wenn Worktree technisch nicht verfügbar. Jede Lieferung enthält Commit/Branch lokal, geänderte Dateien, API-Diff, Testausgabe, offene Punkte; Lead cherry-pickt/merged lokal kontrolliert. Achtung: Ein OpenCode-Subagent übernimmt nicht zwangsläufig einen anderen Worktree als sein Parent; Arbeitsverzeichnis und Edit-Root pro Agent aktiv prüfen. Wenn nicht möglich, getrennte lokale OpenCode-Sessions/Branches verwenden oder für disjunkte Dateien in einem Worktree arbeiten. Keine Remote Writes.

### A – `security` (Phase 0, P0)
Ownership: `crates/terminalwiki-core/src/paths.rs`, ggf. `wiki.rs`/`resolve.rs` **exklusiv vom Lead freigegeben**; Tests für Root-Pfade; Audit-Spec und Regressiontests. Scope: new/create, symlink, rename, `\input`, embeds, CLI edit; Kontamination der terminalen Ausgabe; command invocation. Erst Sicherheitsgates, dann parallele Featurearbeit. Gibt Sicherheitsvorgaben vor, die jede Agentin einhalten muss.

### B – `bootstrap` (Phase 1, P1)
Ownership: neue CLI Module `commands/init.rs`, `commands/open.rs`, `commands/config_explain.rs`; neue Konfig-/Theme-Module `core/src/filter.rs`, `core/src/themes.rs`; kein Bearbeiten `cli/src/args.rs` oder `core/src/config.rs` bis Lead exklusive Integrationsphase. Scope: `tw init`, list/exclude/include, Theme von Dotfiles, helper for external editor. Existing Config API beibehalten; Contract/Handoff an Lead. Tests: clean/existing/invalid roots; `.twignore`, Glob-Match, precedence, hostile paths.

### C – `tera` (Phase 2, P1)
Ownership: `crates/terminalwiki-render/src/math.rs`, neue `math/` Untermodule und Renderer-/Markdown-Adapter **nur nach vereinbartem Contract**; exklusives Ownership bei `document.rs`, `markdown.rs`, `render.rs` falls notwendig. Scope: nativer Math-AST/Box-Layout, MD+LaTeX math delimiters, unit/snapshot tests und einfache Benchmarks. Keine externe TeX-Engine und keine TUI-Änderungen.

### D – `texdoc` (Phase 2, P1)
Ownership: neues `crates/terminalwiki-latex/` oder neues `latex_document/` Untermodul; keine bestehenden `terminalwiki-render` Dateien ohne Contract-Handoff. Scope: LaTeX Strukturparser + Dokument-/Outline-/Ref-IR. Kommunikation mit Tera zu `MathBlock`, mit Index zu Headings. Pure Rust, no arbitrary command execution. Fixtures.tex, tests.

### E – `media` (Phase 2, P1)
Ownership: neues `crates/terminalwiki-media/`; Terminalgraphics-Backend und PdfProvider-Backend, Tests. Kein autonomer Wechsel der TUI-Bibliothek. PDFium/Poppler optional, backend `none` muss weiterhin bauen. Bild/ PDF Vorschau werden vom Lead/Luna über schlanke API verdrahtet. Tests für TTY/noTTY, Ghostty/Kitty, fallback, cancellation, memory.

### F – `index` (Phase 2, P1)
Ownership: `crates/terminalwiki-index/**`, `crates/terminalwiki-graph/**`; ggf. neue `graph-views` Module. Keine Änderungen an Tera/TexDoc-Parsern; normalisierte Metadaten-Schnittstelle vom Lead nutzen. Scope: `title_exact`, phrase, path, type, negation, all/any, page heading anchors, backlink semantics, improved snippets, global vs local graph caps, link resolver policies. CLI-Erweiterungen werden dem Lead als API-Handoff geliefert.

### G – `luna` (Phase 3, P1)
Ownership: `crates/terminalwiki-tui/**`; keine Änderungen `terminalwiki-render/**` und `terminalwiki-media/**`. Scope: keyboard-first reader panes, focus/modes, outline, search, anchors, external edit suspension, PDF pager/image/graph widget, handling of async media events. Keine internen Texteditierfunktionen und kein Editorbuffer. Handoff with Tera/Media via frozen API. Test user journeys mit simulated key events.

### G2 – `diagrams` (Phase 2, P1)
Ownership: neues `crates/terminalwiki-diagrams/**`; keine Render-/TUI-/Index-Dateien ohne Contract-Handoff. Scope: Mermaid `flowchart`-/DOT-Subsets als sichere Graph-IR, native Unicode/Braille-/Boxdraw-Layoutengine, begrenzte `xyplot`-DSL für mathematische Kurven und Messdaten; Pixel-Rendering optional über Media-Grafikbackend; keine `eval`-/JS-/Graphviz-Prozesse. Tests: DAG/Zyklen, komplexe Knoten, Punktelimits, schmale Terminals, Unicode; Fallback zeigt Quelltext bei unbekannten Konstrukten.

### H – `docs` (Phase 2/3, P1)
Ownership: `docs/**`, `.readthedocs.yaml` und lokale Doku-Assets; keine anderen Dateien. Scope: MyST Markdown + Sphinx + Furo custom CSS/colors/fonts fallback, light/dark mobile doc navigation, diagrams/screenshots nur wo reproduzierbar, Getting Started, config + formats, PDF optional deps, keyboard shortcut map, graph, troubleshooting, security, API/architecture. `sphinx-build -n -W --keep-going -b html ...` muss laufen; nur lokale Artefakte, kein RTD-Projekt mutieren.

### I – `release` (Phase 2/3, P1)
Ownership: `packaging/**`, `.github/workflows/**` (lokale Files, nicht aktivieren), `scripts/release-*`, README-Install-Abschnitt **nur koordiniert**. Scope: lokale `cargo install --path . --locked --bin tw`/`--git` Prüfung, `PKGBUILD`, `.SRCINFO`, `.pkg.tar.zst`, Debian `.deb` AMD64/ARM64 soweit verfügbar, Manpages, Completions, checksum, dependencies. `pacman -Syu` verlangt eigenes signiertes Repo, kein falsches Versprechen allein durch `.pkg.tar.zst`; AUR ist optional und **nicht** ein eigenes pacman Repo. Kein publishing, keine GitHub Actions remote ausführen.

### J – `qa` (Phase 2-4, P0/P1)
Ownership: `tests/reader/**`, `tests/fixtures/**`, `tests/pty/**`, `benches/**`, `scripts/verify-local.sh`; keine Produktivcode-Datei ohne Lead. Scope: Contract-Tests, hostile fixtures, CLI/TUI-user journeys, no-side-effects, cfg/theme matrix, Rust/OS matrix, benchmarks; echte Testergebnisse und FAIL-Liste. GitHub Actions config via release/lead, nicht selbst zuweisen.

## 12. Integrationsphasen und Regeln

- **Phase 0 (seriell)**: Istzustand, `git status --porcelain`, Baseline bauen/testen, P0-Sicherheitsproblem identifizieren und fixen, Architektur-ADR, APIs einfrieren. Bei bestehenden Nutzeränderungen erst erhalten und auf separatem Branch arbeiten. KEINE parallele Bearbeitung zentraler Dateien, solange Contract offen ist.
- **Phase 1 (parallel isoliert)**: Security/Bootstrap und Tera/ TexDoc/Media/Index starten, docs und release können sofort mit Ist-Befunden beginnen. Jede Änderung in lokalem Branch/Worktree. Lead dokumentiert wer welche Datei besitzt.
- **Phase 2 (parallel)**: Implementierungen pro Crate mit Mock-Contract testen, QA Fixtures vorbereiten. Keine gegenseitigen Direkt-Commits in fremde Branches.
- **Phase 3 (seriell integrieren, danach Luna)**: Lead integriert `core → document IR → math/latex → search/graph → media adapters → CLI → TUI`; testet nach jedem Merge `fmt/check/test/clippy`. Luna darf TUI intern parallel auf Contract-Mocks bauen, finaler Integrationstest erst mit realen Adaptern.
- **Phase 4 (gates)**: realistic wiki smoke, keyboard PTY, Kitty/Ghostty (manuelle Terminalprüfungen falls keine echte Terminalinstanz), PDF native+fallback, search, graph, build packages, RTD/Sphinx lokal. Alle im Abnahmeprotokoll dokumentiert.
- **Phase 5 (lokale Übergabe)**: funktionierendes lokales Release-Paket, Changelog, Packaging scripts, docs build, bekannte Einschränkungen, `git diff --stat`, Testmatrix, Entscheidung ob Beta-Kriterien erreicht. Nicht ohne ausdrückliche Freigabe publizieren.

**Abnahmekriterien (kein bloßer TODO-Abschluss):**
1. 100+ gemischte `.md`/`.tex` Dateien in einem Wiki werden im TUI mit Outline, Search, Links, Backlinks navigiert; no mouse.
2. Mathematik in 6+ komplexen Testformeln strukturiert/mehrzeilig und korrekt auf 80x24, schmaler fallback; Math-Markdown and standalone TEX.
3. PDF mit mehreren Seiten: page go/next/prev, wenigstens eine tatsächlich gerenderte Seite in Kitty/Ghostty mit vorhandenem Backend, `--plain` fallback, fehlende PDF-Library ohne Absturz. Falls Hardware nicht verfügbar, manuelles Test-Gate **offen** dokumentieren statt Erfüllung zu behaupten.
4. Bilder/Diagramme in geeigneten Terminals sichtbar, bei text-only als beschriftetes Ersatzobjekt.
5. `tw init`/`tw tui` vom leeren Verzeichnis bis erster Seitennavigation; `e` suspendiert TUI, öffnet externe Editor-argv, resümiert sicher, neu gespeicherte Datei wird aktualisiert.
6. LaTeX Buildfiles standardmäßig verborgen, `include`/`exclude`/show-artifacts Tests, keine Dokumentdatei beim Browsen modifiziert.
7. `cargo install --path . --locked --bin tw` funktioniert; Arch PKGBUILD kann lokal gebaut und Paketinhalte geprüft werden; Debian Paket soweit Toolchain vorhanden. Keine Veröffentlichungsansprüche.
8. README + Read the Docs (lokaler strict build) stimmen mit Implementierung überein; fehlende Features werden als Limitations angegeben.
9. Sicherheitsregression: Traversal, Symlink escape, URL injection, ANSI/OSC, LaTeX macro recursion, huge PDF/image, editor args, P0 new-page issue.
10. Alle tatsächlichen `fmt/check/test/clippy`-Ergebnisse schriftlich, keine erfundenen grünen Gates.

## 13. Bewusste Nicht-Ziele der Beta

Kein in-app Editor, kein Markdown WYSIWYG, keine plugin-execution/JS, kein vollständiger LaTeX-Compiler, kein TikZ-/PSTricks-Interpreter, kein beliebiges Mermaid-Rendering, keine Sync-/Account-Cloud, keine automatische Transkodierung oder Änderung vorhandener Notizen, keine Release-Veröffentlichung ohne Freigabe. Optional zukünftige `tw build --engine tectonic/latexmk` nur mit explizitem Nutzeraufruf, Sandbox/Command-Allowlist und konfigurierten Output-Pfaden.

## 14. Benötigtes Abschlussprotokoll

Lead gibt am Ende aus: tatsächlicher git HEAD/Branch, lokale Worktrees und Changesets, alle integrierten Features mit file/code refs, reale Test-Commands+Exitcodes, noch offene Bugs, manuelle Terminaltest-Matrix, Bootstrapping-, PDF- und Math-Demo, Installationsbefehle für tatsächlich gebaute Artefakte, Dokumentations-Build-Pfad, sämtliche Sicherheitsrestrisiken, nächster Schritt. **Nicht behaupten, etwas sei bereits in GitHub veröffentlicht.**
