# PRD 05: CLI Interface, Export & Integration

> Specifies the command-line interface (blocks B1, B2), headless export pipeline (block B18), metadata extraction (blocks B19, B20), and system configuration. Normative contracts live in [PRD 06](./06-component-interface-contracts.md).

## 1. CLI Command Grammar

Built using [`clap`](https://crates.io/crates/clap) with derive macros:

```bash
kitty-pptx [OPTIONS] <FILE>              # Interactive TUI presenter (default)
kitty-pptx view <FILE> [OPTIONS]         # One-shot inline terminal scrollback preview
kitty-pptx export <FILE> --out <DIR>     # Headless batch PNG / vector SVG export
kitty-pptx dump <FILE> [OPTIONS]         # Plaintext / markdown / AST extraction
kitty-pptx info <FILE>                   # Deck structure & metadata summary
```

---

## 2. Command Specifications

### 2.1 Default Command (Interactive TUI)
`kitty-pptx deck.pptx` launches the full-screen interactive presenter ([PRD 04](./04-interactive-tui-presenter.md)).
- `--slide <N>`: Start presentation directly on slide $N$ (1-based).
- `--presenter`: Launch directly in split-pane presenter console mode.
- `--protocol <DRIVER>`: Force driver override (`auto`, `kitty`, `sixel`, `iterm2`, `halfblock`).

### 2.2 Subcommand: `view` (One-Shot Inline Pager)
Prints slides directly into the terminal scrollback buffer and terminates immediately:
```bash
kitty-pptx view presentation.pptx             # Renders slide 1
kitty-pptx view presentation.pptx -n 4        # Renders slide 4
kitty-pptx view presentation.pptx --all       # Renders all slides sequentially
```

| Flag | Type | Description |
| :--- | :--- | :--- |
| `-n, --slide <INDEX>` | `usize` | 1-based slide index (default: 1) |
| `-a, --all` | `bool` | Sequentially emits all slides separated by header lines |
| `--fit <contain\|width>` | `enum` | Scaling fit mode (default: `contain`) |
| `--payload <png\|raw>` | `enum` | Kitty payload override (default: `png`) |

**Contract Rules**:
1. **TTY Requirement**: `view` requires a graphics-capable TTY on stdout. If `isatty(stdout) == false`, it exits with code `3` immediately before emitting any bytes.
2. **Scrollback Preservation**: Emits images using Unicode Placeholders (`U=1`, [PRD 03a](./03a-kitty-graphics-protocol-driver.md) §4) on Kitty protocol terminals, allowing images to scroll naturally with shell output.

### 2.3 Subcommand: `export` (Headless Batch Exporter)
Renders slides directly into PNG raster images or clean vector SVG files without any display dependencies (CI/CD, web documentation, thumbnails, LLM indexing).

```bash
kitty-pptx export deck.pptx --out ./dist/ --format png --dpi 200
kitty-pptx export deck.pptx --out ./vector/ --format svg --svg-text paths
kitty-pptx export deck.pptx --out ./all/ --format all --slides 1-5,8
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `-o, --out <DIR>` | `PathBuf` | *(Required)* | Destination directory (created recursively if absent) |
| `-f, --format <FORMAT>` | `enum` | `png` | Export format: `png`, `svg`, or `all` (both) |
| `--dpi <DPI>` | `u32` | `150` | Render DPI for PNG (range: 50–600; sets `RenderTargetSpec`) |
| `--svg-text <MODE>` | `enum` | `paths` | SVG text mode: `paths` (glyph curves) or `text` (DOM `<text>`) |
| `-s, --slides <RANGE>` | `String` | all | Slide range expression (§2.4) |
| `-j, --concurrency <N>`| `usize` | CPU count | Parallel export worker threads |

**Dual-Backend Execution Model**:
- **PNG Export**: The worker compiles the slide into [`VectorSlide`](./06-component-interface-contracts.md) and passes it to `SlideRasterizer` (`tiny-skia`). The resulting `RgbaSlide` encodes to PNG and writes to disk, bounding peak memory below the 150 MB RSS limit.
- **SVG Export**: The worker compiles the slide into `VectorSlide` and passes it to `SlideSvgSerializer`. The vector display list serializes directly to clean XML (`<path>`, `<linearGradient>`, `<text>`) with zero rasterization overhead.
- **Display-Free Build**: `export` links and executes without any display driver code compiled or loaded ([PRD 06](./06-component-interface-contracts.md) §5.2).

### 2.4 Slide Range Expression Syntax (`RangeSet`)
The `--slides` flag accepts a flexible grammar:
- Single slide: `4`
- Range: `1-5`
- Open start: `-3` (slides 1 through 3)
- Open end: `10-` (slides 10 through total count)
- Comma-separated: `1,3,5-7,10-`
- **Validation**: Indices are 1-based. Out-of-bounds numbers produce `ConfigError::InvalidSlideRange` (exit 1). Duplicate indices are deduplicated and sorted ascending.

### 2.5 Subcommand: `dump` (Text & AST Extraction)
Extracts presentation data for downstream Unix pipelines:
- `--notes`: Emits slide numbers and speaker notes text.
- `--comments`: Emits reviewer comments (author, timestamp, text).
- `--markdown`: Emits GitHub-Flavored Markdown via native parser extraction.
- `--json`: Emits complete presentation AST wrapped in a versioned envelope:
  ```json
  {
    "schemaVersion": 1,
    "warnings": [ ... ],
    "presentation": { ... }
  }
  ```

### 2.6 Subcommand: `info` (Metadata Summary)
Prints deck diagnostic information to stdout:
```
File:           quarterly_review.pptx
Slides:         24 slides (2 hidden)
Aspect Ratio:   16:9 (960 x 540 px at 96 DPI base)
Themes:         Office Theme (Calibri, Calibri Light)
Media Assets:   14 images (4.8 MB)
Warnings:       2 (1 font substitution, 1 placeholder chart)
```

---

## 3. Configuration File (`config.toml`)

Loaded from `$XDG_CONFIG_HOME/kitty-pptx/config.toml` (default `~/.config/kitty-pptx/config.toml`):

```toml
[display]
default_protocol = "auto"     # auto | kitty | sixel | iterm2 | halfblock
fit = "contain"               # contain | width
payload = "png"               # png | raw
render_scale = 2.0            # default 2.0x supersampling (1920x1080)

[export]
default_format = "png"        # png | svg | all
default_dpi = 150
svg_text_mode = "paths"       # paths | text

[render]
workers = 0                   # 0 = auto (min(cores-1, 4))

[assets]
cache_mb = 64                 # decoded image LRU budget in MB

[presenter]
start_with_notes = false
target_duration_mins = 20
cache_mb = 64                 # composited frame LRU budget in MB

[typography]
custom_font_dirs = [
  "~/.local/share/fonts",
  "/usr/share/fonts",
]

[media]
player = "mpv"                 # external media player executable

[keybindings]
next_slide = ["Space", "Enter", "Right", "j"]
prev_slide = ["Backspace", "Left", "k"]
toggle_notes = ["n", "p"]
toggle_comments = ["c"]
toggle_sorter = ["Tab", "t"]
play_media = ["M"]
quit = ["q", "Esc"]
```

---

## 4. Exit Code Projections

| Exit Code | Meaning | Mapped Error Variant ([PRD 06](./06-component-interface-contracts.md) §3) |
| :--- | :--- | :--- |
| `0` | Clean execution / success | — |
| `1` | Configuration or file I/O error | `KittyPptxError::Io`, `KittyPptxError::Config` |
| `2` | Corrupted PPTX archive or XML parse failure | `KittyPptxError::Parse` |
| `3` | Display failure or graphics TTY unavailable | `KittyPptxError::Display` |
| `130`| Terminated by user signal (`SIGINT`) | Process signal handler |
