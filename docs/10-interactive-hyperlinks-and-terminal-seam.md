# PRD 10: Interactive Hyperlinks & Terminal Interactivity Seam

> **Normative Architectural Specification & Product Requirements Document**  
> **Status**: Approved · **Applies to**: `office-terminal-core`, `office-terminal-display`, `office-terminal-tui`, `office-terminal-cli`  
> **Upstream Contracts**: [PRD 01](./01-parser-and-data-model.md) §3, [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2, [PRD 03a](./03a-kitty-graphics-protocol-driver.md) §4, [PRD 04](./04-interactive-tui-presenter.md) §2, [PRD 06](./06-component-interface-contracts.md) §2

---

## 1. Architectural Role & Three Interactivity Seams

Previously, hyperlinks were treated as degraded visual formatting (`DEGRADED` in [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2: underline + theme hyperlink color), lacking runtime interactivity or protocol integration. This document elevates hyperlinks to **`FULL` fidelity** across three distinct, decoupled architecture seams.

```
                     ┌───────────────────────────────────────────┐
                     │          PresentationML Slide             │
                     │  (a:hlinkClick, a:hlinkMouseOver, .rels)  │
                     └─────────────────────┬─────────────────────┘
                                           │ pptx-parser / pptx-model
                                           ▼
                     ┌───────────────────────────────────────────┐
                     │          SlideCompiler Engine             │
                     │  - Relationship Resolution to URIs        │
                     │  - ppaction:// Action Verb Parsing        │
                     │  - URI Sanitization & Security Gate       │
                     └─────────────────────┬─────────────────────┘
                                           │
                                           ▼
                     ┌───────────────────────────────────────────┐
                     │        VectorSlide Display List           │
                     │      Enriched with HyperlinkTarget        │
                     │  (DrawText, DrawPath, DrawImage + Bounds) │
                     └──────┬──────────────┬──────────────┬──────┘
                            │              │              │
      ┌─────────────────────┘              │              └─────────────────────┐
      ▼                                    ▼                                    ▼
┌───────────────────────────┐┌───────────────────────────┐┌───────────────────────────┐
│          SEAM 1           ││          SEAM 2           ││          SEAM 3           │
│   Terminal OSC 8 Driver   ││   TUI Interactive Present ││   Native Vector SVG Export│
│ \x1b]8;id=K;<URI>\x1b\... ││ - Hotkey 'o' Modal Picker ││ <a href="<URI>"          │
│ Integrated with Kitty     ││ - Cell Spatial Hit-Test   ││    target="_blank"        │
│ Direct Kitty placement   ││ - Non-blocking Browser OS ││    rel="noopener">...</a> │
└───────────────────────────┘└───────────────────────────┘└───────────────────────────┘
```

### 1.1 The Three Interactivity Seams

1. **Seam 1 — TUI Link Picker and Mouse Dispatch**:
   - The Kitty image path uses direct placement, not Unicode placeholder cells, so terminal-native OSC 8 wrapping of slide pixels is not emitted.
   - External links are opened through the [PRD 04](./04-interactive-tui-presenter.md) link picker and mouse hit-testing; internal jumps and presentation actions are handled in-process.
2. **Seam 2 — TUI Interactive Presenter Navigation**:
   - Provides universal, terminal-independent interactivity within the Ratatui presenter loop ([PRD 04](./04-interactive-tui-presenter.md)).
   - Pressing `o` or `Ctrl+O` opens the **Slide Link Picker** overlay, enumerating all hyperlinks on the active slide with quick-access hotkeys `1..=N`.
   - Supports direct mouse click routing via terminal cell coordinate hit-testing against `SlideLinkIndex`.
   - Dispatches external URLs to the user's default browser via detached, non-blocking OS subprocesses (`xdg-open`, `open`, `powershell Start-Process`).
   - Executes internal slide jumps ($S \to S_{\text{target}}$) and presentation actions without external tools.
3. **Seam 3 — Native Vector SVG Export**:
   - Serializes clickable elements into standard SVG anchor wrappers:
     `<a xlink:href="<URL>" href="<URL>" target="_blank" rel="noopener noreferrer">...</a>`.
   - Maps internal slide jumps to relative anchor fragments (e.g. `#slide-5`), preserving multi-slide web navigation in vector decks exported via `ot export --format svg`.

---

## 2. Data Model & Extraction (`pptx-model` + `pptx-parser`)

### 2.1 OOXML Source Elements

In ECMA-376 Part 1 §21.1.2.3.5, hyperlinks are declared via `CT_Hyperlink` elements:
- **Shape-Level Hyperlinks**: `<p:cNvPr>` $\to$ `<a:hlinkClick r:id="..." action="...">` or `<a:hlinkMouseOver>`. Activates when clicking anywhere within the shape's vector geometry or picture bounds.
- **Text Run-Level Hyperlinks**: `<a:rPr>` $\to$ `<a:hlinkClick r:id="..." action="...">`. Scoped strictly to the glyph bounding box of that text run.
- **Table Cell Hyperlinks**: Attached to individual text runs within table cell paragraphs (`<a:tc>` $\to$ `<a:txBody>` $\to$ `<a:p>` $\to$ `<a:r>`).

### 2.2 Relationship Resolution & Normalization

Hyperlinks reference relationship IDs (`r:id`). The parser resolves these IDs against the part's relationship manifest (`.rels`):
1. **External Web Hyperlinks**:
   - The relationship entry has `TargetMode="External"` and `Target="https://example.com"`.
   - The target is an absolute URI. Normalization trims whitespace and verifies URI syntax.
2. **Internal Slide Jump Links**:
   - The relationship entry has `TargetMode="Internal"` (or omitted) and points to another slide part (e.g., `Target="../slides/slide3.xml"`).
   - Normalization resolves the relative path against the owning part's directory (`ppt/slides/` $\to$ `ppt/slides/slide3.xml`).
   - The compiler queries the presentation `SlideCatalog` to resolve the internal part name to a 0-indexed presentation slide sequence number $S \in [0, N-1]$.

### 2.3 PowerPoint Action Verbs (`ppaction://`)

When `a:hlinkClick` contains an `@action` attribute, it indicates an internal PowerPoint command verb:

| Action Verb (`@action`) | Target Relationship | Parsed Semantic Action |
| :--- | :--- | :--- |
| `ppaction://hlinksldjump` | Points to `slideK.xml` via `r:id` | Jump to specific slide index $K$ |
| `ppaction://hlinkshowjump?jump=nextslide` | None (`r:id` absent) | Jump to next slide ($S + 1$) |
| `ppaction://hlinkshowjump?jump=prevslide` | None (`r:id` absent) | Jump to previous slide ($S - 1$) |
| `ppaction://hlinkshowjump?jump=firstslide` | None (`r:id` absent) | Jump to first slide ($0$) |
| `ppaction://hlinkshowjump?jump=lastslide` | None (`r:id` absent) | Jump to last slide ($N - 1$) |
| `ppaction://hlinkshowjump?jump=endshow` | None (`r:id` absent) | Terminate presentation / Exit TUI |
| `ppaction://customshow?id=...` | None | Logged as degraded warning (treated as no-op) |

### 2.4 Tooltip Metadata & Styling Attributes

- **Tooltip Text**: Parsed from optional `a:hlinkClick/@tooltip`. Displayed in terminal TUI hover toasts and SVG `<title>` child tags.
- **Color Inversion Override**: Parsed from Office extension `a:hlinkClick/@hlinkClr="tx"` (`hyperlink_uses_text_fill`). When false (default), text runs take the theme's `hyperlink` color (`#0066CC`). When true, authored run fill takes precedence.

---

## 3. Vector IR Representation (`VectorSlide`)

The intermediate display list in `office-terminal-core` ([PRD 06](./06-component-interface-contracts.md) §2.1) is enriched to retain semantic hyperlink metadata and bounding geometries.

### 3.1 Type Definitions

```rust
/// Semantic destination of an interactive hyperlink.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HyperlinkTarget {
    /// Validated external web or communication URI (https://, http://, mailto:).
    ExternalUri(String),
    /// 0-indexed chronological slide number in the presentation.
    SlideIndex(usize),
    /// Native PowerPoint presentation control action.
    Action(SlideAction),
}

/// Standardized slide navigation actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlideAction {
    NextSlide,
    PreviousSlide,
    FirstSlide,
    LastSlide,
    EndShow,
}

/// Bounding descriptor and metadata for hit-testing and export.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HyperlinkDescriptor {
    pub target: HyperlinkTarget,
    pub tooltip: Option<String>,
    pub label: Option<String>,
    /// Bounding rectangle in slide EMU coordinates.
    pub bounds_emu: RectF32,
}
```

### 3.2 Display List Command Enrichment

`VectorCommand` and `PositionedGlyphRun` carry optional hyperlink descriptors:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum VectorCommand {
    DrawPath {
        path: VectorPath,
        fill: Option<VectorPaint>,
        stroke: Option<VectorStroke>,
        transform: AffineTransform,
        hyperlink: Option<HyperlinkDescriptor>,
    },
    DrawText {
        runs: Vec<PositionedGlyphRun>,
        transform: AffineTransform,
        hyperlink: Option<HyperlinkDescriptor>, // Shape-level fallback
    },
    DrawImage {
        rel_id: String,
        src_rect: NormalizedRect,
        dest_rect: RectF32,
        opacity: f32,
        transform: AffineTransform,
        effects: Vec<BlipEffect>,
        hyperlink: Option<HyperlinkDescriptor>,
    },
    PushClip(VectorPath),
    PopClip,
}
```

Each `PositionedGlyphRun` also carries its run-specific hyperlink:
```rust
pub struct PositionedGlyphRun {
    pub text: String,
    pub font_family: String,
    pub font_size_pt: f32,
    pub color: [u8; 4],
    pub glyphs: Vec<PositionedGlyph>,
    pub underline: bool,
    pub strikethrough: bool,
    pub hyperlink: Option<HyperlinkDescriptor>,
}
```

### 3.3 Spatial Hit-Testing Math & Coordinate Mapping

To map continuous slide EMU coordinates $(x, y) \in [0, W_{\text{emu}}] \times [0, H_{\text{emu}}]$ to discrete terminal character cells $(c, r)$, the layout engine applies the negotiated viewport affine transform:

Let the terminal slide canvas occupy $C_{\text{cols}}$ columns and $R_{\text{rows}}$ rows, anchored at terminal cell offset $(C_0, R_0)$. For any shape or run with EMU bounding box $[x_{\min}, y_{\min}, x_{\max}, y_{\max}]$:

$$C_{\text{start}} = C_0 + \left\lfloor \frac{x_{\min}}{W_{\text{emu}}} \times C_{\text{cols}} \right\rfloor, \quad C_{\text{end}} = C_0 + \min\left(C_{\text{cols}} - 1, \; \left\lfloor \frac{x_{\max}}{W_{\text{emu}}} \times C_{\text{cols}} \right\rfloor\right)$$

$$R_{\text{start}} = R_0 + \left\lfloor \frac{y_{\min}}{H_{\text{emu}}} \times R_{\text{rows}} \right\rfloor, \quad R_{\text{end}} = R_0 + \min\left(R_{\text{rows}} - 1, \; \left\lfloor \frac{y_{\max}}{H_{\text{emu}}} \times R_{\text{rows}} \right\rfloor\right)$$

Given a terminal mouse click at cell $(c_{\text{mouse}}, r_{\text{mouse}})$, a hit occurs when:
$$C_{\text{start}} \le c_{\text{mouse}} \le C_{\text{end}} \quad \text{and} \quad R_{\text{start}} \le r_{\text{mouse}} \le R_{\text{end}}$$

When bounding boxes overlap, the highest z-index element (the command appearing latest in `VectorSlide::commands`) takes precedence.

### 3.4 `SlideLinkIndex` Query Acceleration

During slide compilation, an immutable `SlideLinkIndex` is generated for each slide:
```rust
#[derive(Clone, Debug, Default)]
pub struct SlideLinkIndex {
    links: Vec<IndexedHyperlink>,
}

#[derive(Clone, Debug)]
pub struct IndexedHyperlink {
    pub id: usize, // 1-based index (1..=N) for hotkey selection
    pub descriptor: HyperlinkDescriptor,
    pub cell_bounds: CellRect,
}

impl SlideLinkIndex {
    pub fn hit_test(&self, col: u16, row: u16) -> Option<&IndexedHyperlink>;
    pub fn iter(&self) -> impl Iterator<Item = &IndexedHyperlink>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

---

## 4. Terminal Driver & OSC 8 Protocol Mechanics

### 4.1 OSC 8 Escape Sequence Syntax

The terminal hyperlink escape sequence follows the standard OSC 8 format:
```
\x1b]8;<params>;<URI>\x1b\<content>\x1b]8;;\x1b\
```
- **Initiator**: `\x1b]8;` (`ESC ] 8 ;`).
- **Parameters**: Semicolon-delimited key-value pairs. `id=<link_id>` is emitted to logically group disjoint text segments or multi-line wraps under a single clickable hyperlink entity in terminal emulators.
- **URI**: RFC 3986 percent-encoded URI string.
- **Terminator**: String Terminator `\x1b\` (`ESC \`).
- **Closing Sequence**: `\x1b]8;;\x1b\` (empty URI and empty params) closes the active link scope.

### 4.2 URL Security Sanitization Gate

Hyperlink targets from untrusted PPTX files must pass strict security sanitization before being emitted in OSC 8 sequences, rendered in SVG, or dispatched to OS subprocesses:

```rust
pub fn sanitize_hyperlink_url(raw_url: &str) -> Result<String, SecurityError> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(SecurityError::EmptyUrl);
    }
    // Length restriction prevents buffer overflow / terminal parser denial of service
    if trimmed.len() > 2048 {
        return Err(SecurityError::UrlTooLong);
    }
    // Reject ASCII control characters and terminal injection sequences
    if trimmed.bytes().any(|b| b < 0x20 || b == 0x7F) {
        return Err(SecurityError::IllegalControlCharacters);
    }
    // Explicit scheme allow-list
    let parsed = url::Url::parse(trimmed).map_err(|e| SecurityError::InvalidUri(e.to_string()))?;
    match parsed.scheme() {
        "http" | "https" | "mailto" => Ok(parsed.to_string()),
        "file" => Err(SecurityError::ForbiddenScheme("file:// access blocked")),
        "javascript" => Err(SecurityError::ForbiddenScheme("javascript:// execution blocked")),
        "data" => Err(SecurityError::ForbiddenScheme("data:// URIs blocked")),
        "shell" | "cmd" | "powershell" => Err(SecurityError::ForbiddenScheme("shell execution blocked")),
        other => Err(SecurityError::UnsupportedScheme(other.to_string())),
    }
}
```

---

## 5. TUI Keyboard & Mouse Interaction

### 5.1 Status Bar Integration

When the current slide contains active hyperlinks (`!slide_link_index.is_empty()`):
- A status badge `[o: N Links]` (e.g. `[o: 3 Links]`) renders in bold cyan in the presenter status line alongside `[?] Help` and `[M: Play Media]`.
- If no hyperlinks are present on the slide, the badge is hidden.

### 5.2 Slide Link Picker Modal (`o` / `Ctrl+O`)

Pressing `o` or `Ctrl+O` pauses standard navigation and renders a centered Ratatui popup overlay:

```
┌───────────────────────── Active Slide Hyperlinks ─────────────────────────┐
│ Slide 5 of 24 · Press [1..=N] or [↑/↓ + Enter] to open · [Esc] to dismiss │
├───────────────────────────────────────────────────────────────────────────┤
│ [1] [WEB]    Rust Language Homepage                                       │
│              https://www.rust-lang.org                                    │
│                                                                           │
│ [2] [SLIDE]  Jump to Slide 12: Microarchitecture Benchmarks               │
│              Slide #12                                                    │
│                                                                           │
│ [3] [ACTION] Jump to Next Slide                                           │
│              Action: NextSlide                                            │
│                                                                           │
│ [4] [MAIL]   Contact Speaker                                              │
│              mailto:presenter@example.com                                 │
└───────────────────────────────────────────────────────────────────────────┘
```

#### Modal Interaction Rules:
- **Direct Hotkeys `1..=9`**: Instantly activates the corresponding link item without requiring an additional `Enter` confirmation.
- **List Navigation**: `Up` / `Down` arrows or `j` / `k` navigate selection cursor; `Enter` activates the selected link.
- **Cancellation**: `Esc`, `q`, or `o` closes the modal and returns focus to the active presentation view.

### 5.3 Cross-Platform Detached Browser Spawning

When an `ExternalUri` is triggered via the TUI, it spawns the OS default web browser off the main event loop to avoid UI latency:

```rust
pub fn launch_external_browser(url: &str) -> Result<(), PresentationError> {
    let sanitized = sanitize_hyperlink_url(url)?;
    
    #[cfg(target_os = "linux")]
    let mut cmd = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-Command", "Start-Process"]);
        c
    };

    cmd.arg(&sanitized)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let _child = cmd.spawn().map_err(|e| {
        PresentationError::BrowserLaunchFailed(format!("Failed to spawn browser: {}", e))
    })?;

    Ok(())
}
```

- **Non-blocking Execution**: The child process is detached. The TUI event loop continues without interruption.
- **Transient Error Toast**: If browser launching fails (e.g. `xdg-open` missing on minimal headless Linux), a 3-second red notification toast is queued in `app_state.notifications`: `[Error: Unable to launch web browser]`.

### 5.4 Mouse Click Event Routing

When mouse capture is enabled (`crossterm::event::EnableMouseCapture`):
1. `MouseEventKind::Down(MouseButton::Left)` receives terminal column and row coordinates $(c, r)$.
2. App state tests the coordinate against `SlideLinkIndex::hit_test(c, r)`:
   - **Hit `ExternalUri(url)`**: Dispatches `launch_external_browser(url)`.
   - **Hit `SlideIndex(idx)`**: Sets `app_state.current_slide = idx`, triggering immediate slide prefetch and flip.
   - **Hit `Action(action)`**: Dispatches corresponding slide navigation method (`next_slide()`, `prev_slide()`, etc.).
   - **Miss**: Triggers default presentation mouse behavior (left click advances to next slide).

---

## 6. Native Vector SVG Export

When exporting vector presentations via `ot export --format svg`:
- Hyperlink descriptors wrap their underlying graphical elements using standard SVG `<a ...>` tags.
- For `HyperlinkTarget::ExternalUri(url)`:
  ```xml
  <a xlink:href="https://www.rust-lang.org" href="https://www.rust-lang.org" target="_blank" rel="noopener noreferrer">
    <text x="120" y="240" font-family="Carlito" font-size="24" fill="#0066CC" text-decoration="underline">
      Rust Language Homepage
    </text>
  </a>
  ```
- For `HyperlinkTarget::SlideIndex(idx)`:
  ```xml
  <a xlink:href="#slide-12" href="#slide-12" target="_self">
    <path d="M 10 10 L 100 10 L 100 50 Z" fill="#2B579A"/>
  </a>
  ```
- Tooltip metadata is emitted using an inline SVG `<title>` child element inside the `<a>` wrapper:
  ```xml
  <a xlink:href="https://example.com" href="https://example.com" target="_blank">
    <title>Click to visit documentation</title>
    <rect x="50" y="50" width="200" height="100" fill="#EFEFEF"/>
  </a>
  ```

---

## 7. Quality & Verification Regime

The testing strategy follows the multi-tiered regime defined in [PRD 07](./07-quality-and-testing.md):

### 7.1 L1 Unit Tests: Security & Normalization
- **Scheme Allow-list Verification**: Asserts that `http://`, `https://`, and `mailto:` parse successfully.
- **Forbidden Scheme Rejection**: Verifies that `file:///etc/passwd`, `javascript:alert(1)`, and `data:text/html,...` return `SecurityError::ForbiddenScheme`.
- **Terminal Injection Protection**: Tests URLs containing embedded escape sequences (e.g. `https://example.com/\x1b]8;;evil\x1b\`) and asserts immediate rejection.
- **Relative Path Resolution**: Tests that `.rels` paths like `../slides/slide3.xml` map accurately to 0-based slide indices.

### 7.2 L2a Vector Scene Goldens
- Asserts that compiled `VectorSlide` JSON snapshots correctly preserve `HyperlinkDescriptor` fields across shape, picture, and text run commands.
- Validates that text runs retain `underline: true` and appropriate theme hyperlink colors.

### 7.3 L3 Protocol & Export Goldens
- **OSC 8 Driver Capture**: A mock `TtyWriter` records emitted escape sequence bytes, verifying exact OSC 8 string structures:
  `\x1b]8;id=link_1;https://example.com\x1b\...content...\x1b]8;;\x1b\`
- **SVG Anchor Goldens**: Validates that exported `.svg` files contain well-formed `<a>` tags with `rel="noopener noreferrer"` and `target="_blank"`.

### 7.4 L4 TUI State Machine Tests
- Tests pressing `o` opens `PresenterMode::LinkPickerModal`.
- Tests pressing `1` fires link action and closes modal.
- Simulates mouse click event at known cell coordinate and verifies correct action dispatch.

---

## 8. Fidelity Classification Update

With the completion of this specification, the normative Fidelity Policy table in [PRD 02d](./02d-element-renderers-and-fidelity-policy.md) §2 is updated:

| Element / Feature | Previous Fidelity | Updated Fidelity | Behavioral Contract & Output |
| :--- | :--- | :--- | :--- |
| **Hyperlinks** | `DEGRADED` | **`FULL`** | Clickable via terminal OSC 8, interactive TUI modal (`o`), mouse hit-testing, and native SVG anchor tags. |
