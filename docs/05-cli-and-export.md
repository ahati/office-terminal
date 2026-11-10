# PRD 05: CLI Interface, Export & Integration

## 1. Overview
While the interactive TUI is ideal for live presentations, `kitty-pptx` also functions as a flexible Unix CLI tool for one-shot inline slide previews, headless image batch exports, speaker notes extraction, and terminal pipeline automation.

---

## 2. CLI Command Structure

Built using [`clap`](https://crates.io/crates/clap) with derive macros:

```bash
kitty-pptx [OPTIONS] <FILE>
kitty-pptx view <FILE> [OPTIONS]
kitty-pptx export <FILE> --out <DIR> [OPTIONS]
kitty-pptx dump <FILE> [OPTIONS]
kitty-pptx info <FILE>
```

### 2.1 Default Invocation (Interactive TUI)
Running `kitty-pptx deck.pptx` launches the full-screen interactive presenter TUI (specified in [PRD 04](./04-interactive-tui-presenter.md)).
- Accepts `--slide <NUM>` to start presentation directly on a specific slide.
- Accepts `--presenter` to start with split-view speaker notes enabled.

---

### 2.2 Subcommand: `view` (One-Shot Inline Pager)
Prints one or more slides directly into the terminal scrollback buffer and terminates immediately (similar to `cat`, `viu`, or `chafa`).

```bash
# Preview slide 1 inline in Kitty terminal
kitty-pptx view presentation.pptx

# Preview slide 5, scaled to 100% of terminal width
kitty-pptx view presentation.pptx -n 5 --fit width

# Pipe all slides sequentially into the terminal buffer
kitty-pptx view presentation.pptx --all
```

#### Flags:
- `-n, --slide <INDEX>`: Slide number to display (1-based index, default: 1).
- `-a, --all`: Render all slides sequentially separated by slide title headers.
- `--fit <contain|width>`: Scaling mode (default: `contain`).
- `--protocol <kitty|sixel|iterm2|halfblock|auto>`: Explicitly select or force a graphics protocol.

---

### 2.3 Subcommand: `export` (Headless Batch Exporter)
Renders slides headlessly into raster images or vector files without requiring a terminal environment (ideal for CI/CD, documentation generation, or thumbnailing).

```bash
# Export all slides as high-res PNGs to ./slides/
kitty-pptx export deck.pptx --out ./slides/ --dpi 200

# Export specific slide range (slides 1 to 4 and slide 8)
kitty-pptx export deck.pptx --out ./thumbs/ --slides 1-4,8 --format png
```

#### Flags:
- `-o, --out <DIR>`: Destination directory for exported files (created if missing).
- `-f, --format <png|svg>`: Output image format (default: `png`).
- `--dpi <DPI>`: Target render DPI (default: 150 DPI; e.g. 2000x1125 for 16:9).
- `-s, --slides <RANGE>`: Comma-separated slide indices or ranges (e.g. `1-3,5,10-12`).
- `--concurrency <N>`: Number of parallel worker threads (default: CPU core count via `rayon`).

---

### 2.4 Subcommand: `dump` (Text & Metadata Extraction)
Extracts non-visual presentation content for terminal piping, indexing, or feeding into LLMs.

```bash
# Dump all speaker notes to stdout
kitty-pptx dump deck.pptx --notes

# Extract presentation as clean GitHub-Flavored Markdown
kitty-pptx dump deck.pptx --markdown

# Dump internal AST as JSON for tool integration
kitty-pptx dump deck.pptx --json
```

#### Flags:
- `--notes`: Prints slide numbers and corresponding speaker notes.
- `--markdown`: Generates structured GitHub-Flavored Markdown (headers, bullet points, and tables) leveraging upstream's `to_markdown_native` function.
- `--json`: Emits the complete `pptx-model::Presentation` serialized as JSON.

---

### 2.5 Subcommand: `info` (Metadata Summary)
Outputs a concise summary of the presentation deck:
```
File:           quarterly_review.pptx
Slides:         24 slides
Aspect Ratio:   16:9 (1920 x 1080)
Themes:         Office Theme (Calibri, Calibri Light)
Media Assets:   14 images (Total: 4.8 MB)
Slide Outline:
  1. Q3 Financial Performance
  2. Revenue Growth by Region
  3. Product Milestone Updates
  ...
```

---

## 3. Configuration File (`config.toml`)

Users can customize default behavior via a TOML configuration file located at:
- Linux / macOS: `~/.config/kitty-pptx/config.toml` (respects `$XDG_CONFIG_HOME`)
- Windows: `%APPDATA%\kitty-pptx\config.toml`

### Example Configuration:
```toml
[display]
# Preferred graphics protocol ("auto", "kitty", "sixel", "iterm2", "halfblock")
default_protocol = "auto"

# Default fitting behavior ("contain", "width")
fit = "contain"

# Render resolution multiplier for high-DPI screens
render_scale = 1.5

[presenter]
# Always open in split presenter view (with notes and next slide)
start_with_notes = false

# Speaker timer warning threshold in minutes (shows yellow/red badge)
target_duration_mins = 20

# Slide cache window size (number of neighboring slides kept in RAM)
cache_window = 5

[typography]
# Directories to scan for extra custom presentation fonts (.ttf, .otf)
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

---

## 4. Unix Pipeline & Stdin Support

`kitty-pptx` supports standard Unix streaming:
```bash
# Read PPTX stream from stdin
curl -sL "https://example.com/slides.pptx" | kitty-pptx view -

# Pipe speaker notes into an LLM summarizer
kitty-pptx dump presentation.pptx --markdown | llm "Summarize the key takeaways"
```

### Exit Codes:
| Code | Meaning |
| :--- | :--- |
| `0` | Success / Clean exit |
| `1` | General file I/O error (file not found, permission denied) |
| `2` | Corrupted PPTX or XML parsing failure |
| `3` | Terminal graphics protocol unsupported / incompatible TTY |
| `130` | Terminated by user (`SIGINT` / `Ctrl+C`) |
