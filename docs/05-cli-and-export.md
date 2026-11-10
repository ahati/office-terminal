# PRD 05: CLI Interface, Export & Integration

## 1. Overview

While the interactive TUI is ideal for live presentations, `kitty-pptx` also functions as a flexible Unix CLI tool for one-shot inline slide previews, headless image batch exports, speaker-notes extraction, and terminal pipeline automation.

The binary name is **`kitty-pptx`** everywhere (usage examples across all PRDs use it consistently).

---

## 2. CLI Command Structure

Built with [`clap`](https://crates.io/crates/clap) derive macros:

```bash
kitty-pptx [OPTIONS] <FILE>              # interactive TUI (default)
kitty-pptx view <FILE> [OPTIONS]         # one-shot inline pager
kitty-pptx export <FILE> --out <DIR>     # headless batch PNG export
kitty-pptx dump <FILE> [OPTIONS]         # text / metadata extraction
kitty-pptx info <FILE>                   # metadata summary
```

### 2.1 Default Invocation (Interactive TUI)
`kitty-pptx deck.pptx` launches the full-screen presenter TUI ([PRD 04](./04-interactive-tui-presenter.md)).
- `--slide <NUM>`: start on a specific slide.
- `--presenter`: start with split-view speaker notes enabled.

### 2.2 Subcommand: `view` (One-Shot Inline Pager)
Prints one or more slides into the terminal **scrollback buffer** and terminates immediately (similar in spirit to `cat`, `viu`, or `chafa`).

```bash
kitty-pptx view presentation.pptx             # slide 1, contained
kitty-pptx view presentation.pptx -n 5        # slide 5
kitty-pptx view presentation.pptx --all       # all slides, sequentially
```

| Flag | Behavior |
| :--- | :--- |
| `-n, --slide <INDEX>` | 1-based slide index (default 1) |
| `-a, --all` | Render all slides sequentially, each preceded by a text header line (`— 3 / 20 · Title —`) |
| `--fit <contain\|width>` | Scaling mode (default `contain`) |
| `--protocol <kitty\|sixel\|iterm2\|halfblock\|auto>` | Force a graphics driver (default `auto`, [PRD 03](./03-terminal-graphics-protocol.md) §8) |
| `--payload <png\|raw>` | Kitty payload format override (default `png`, PRD 03 §3) |

Behavioral contract:
- **Requires a graphics-capable TTY on stdout.** If stdout is not a TTY or no capable driver exists, `view` exits `3` before emitting anything (a piped `view` must not spray escape bytes into files). Use `dump`/`export` in pipelines.
- Slides print via **scrollback placement** (`U=1` Unicode placeholders, PRD 03 §7.2) so they survive scrolling on Kitty-protocol terminals; drivers without placeholder support page one slide per keypress instead.
- Exits after the last slide (or `q`); image cleanup follows the standard contract (PRD 03 §9).

### 2.3 Subcommand: `export` (Headless Batch Exporter)
Renders slides into PNG files without any terminal environment (CI/CD, documentation generation, thumbnails, LLM pipelines).

```bash
kitty-pptx export deck.pptx --out ./slides/ --dpi 200
kitty-pptx export deck.pptx --out ./thumbs/ --slides 1-4,8
```

| Flag | Behavior |
| :--- | :--- |
| `-o, --out <DIR>` | Destination directory (created if missing) |
| `--dpi <DPI>` | Render DPI (default 150). Target spec: slide inches × DPI, e.g. 16:9 @ 200 DPI = 2000×1125 (feeds `RenderTargetSpec`, PRD 06 §2.2) |
| `-s, --slides <RANGE>` | Comma-separated indices/ranges (`1-3,5,10-12`; default all) |
| `-j, --concurrency <N>` | Parallel workers (default: pipeline default, PRD 02a §5.4) |

Contract:
- **Headless by construction**: export builds the pipeline with no `StageSink`/driver — the composition half only (PRD 06 §6.2). This is the enforced architectural invariant; display code is not merely unused but absent from the build.
- Output is deterministic for a given (deck, dpi, versions, font set) tuple — required by the golden regime ([PRD 07](./07-quality-and-testing.md) §4.2): fixed PNG encoder settings, no timestamps.
- **PNG is the only v1 format.** *(SVG export from the previous revision is removed: the engine rasterizes; a vector backend would be a second renderer. Vector export may be revisited post-v1 as a separate renderer backend with its own PRD.)*
- Exit codes follow §5; a slide that fails composition exports as its placeholder rendering with a warning (never silently skipped, listed in stderr summary).

### 2.4 Subcommand: `dump` (Text & Metadata Extraction)
```bash
kitty-pptx dump deck.pptx --notes      # speaker notes
kitty-pptx dump deck.pptx --markdown   # GitHub-Flavored Markdown
kitty-pptx dump deck.pptx --json       # full AST
```
- `--notes`: slide numbers + notes text (the slide-thumbnail image embedded in notes slides is filtered out — notes text only).
- `--markdown`: GFM via the upstream `to_markdown_native` (headers, bullets, tables).
- `--json`: the full `pptx-model::Presentation`, wrapped as `{"schemaVersion": 1, "warnings": [...], "presentation": {...}}`. The schema version bumps on breaking AST changes ([PRD 06](./06-component-interface-contracts.md) §11); session `WarningEvent`s are included so degradation is visible to tooling.

### 2.5 Subcommand: `info` (Metadata Summary)
```
File:           quarterly_review.pptx
Slides:         24 slides
Aspect Ratio:   16:9 (960 x 540 px at the 96 DPI base; see PRD 02a §2.2)
Themes:         Office Theme (Calibri, Calibri Light)
Media Assets:   14 images (4.8 MB)
Warnings:       2 (1 font substitution, 1 placeholder chart)
Slide Outline:
  1. Q3 Financial Performance
  ...
```

---

## 3. Configuration File (`config.toml`)

Located at `~/.config/kitty-pptx/config.toml` (respects `$XDG_CONFIG_HOME`). *(The platform scope is Linux/macOS/WSL2 per [PRD 00](./00-system-overview.md) §5; the Windows path from the previous revision is removed for consistency.)*

```toml
[display]
default_protocol = "auto"     # auto | kitty | sixel | iterm2 | halfblock
fit = "contain"               # contain | width
payload = "png"               # png | raw   (PRD 03 §3)
render_scale = 2.0            # render-target multiplier over the 960x540 base

[render]
workers = 0                   # 0 = auto (min(cores-1, 4))

[assets]
cache_mb = 64                 # decoded-image LRU budget

[presenter]
start_with_notes = false
target_duration_mins = 20
cache_window = 5              # pipeline window size (PRD 02a §5.1)

[typography]
custom_font_dirs = [
  "~/.local/share/fonts",
  "/usr/share/fonts",
]

[keybindings]
next_slide = ["Space", "Enter", "Right", "j"]
prev_slide = ["Backspace", "Left", "k"]
toggle_notes = ["n", "p"]
toggle_sorter = ["Tab", "t"]
quit = ["q", "Esc"]
```

Precedence: CLI flags > config file > built-in defaults. Unknown keys warn and continue.

---

## 4. Unix Pipeline & Stdin Support

```bash
curl -sL "https://example.com/slides.pptx" | kitty-pptx dump - --markdown
kitty-pptx dump presentation.pptx --markdown | llm "Summarize the key takeaways"
```

`-` as FILE reads from stdin ([PRD 06](./06-component-interface-contracts.md) §3 `Source::Stdin`). Graphics subcommands (`view`, TUI) still require a TTY on stdout regardless of input source; `dump`/`export`/`info` are fully pipe-safe.

---

## 5. Exit Codes

| Code | Meaning | Projected from ([PRD 06](./06-component-interface-contracts.md) §8) |
| :--- | :--- | :--- |
| `0` | Success / clean exit | — |
| `1` | General I/O or configuration error (file not found, permission, bad flags) | `Io`, `Config` |
| `2` | Corrupted/unsupported PPTX, XML parse failure, resource-limit breach | `Parse` |
| `3` | Terminal graphics unavailable (no capable TTY, probe failure, `DisplayError::Io` without fallback) | `Display` |
| `130` | Terminated by user (`SIGINT`) | — |
