# PRD 04: Interactive TUI & Presenter Mode

> Specifies the interactive terminal user interface (block B17) built on **`ratatui`** and **`crossterm`**. Interfaces consumed by the TUI are normative in [PRD 06](./06-component-interface-contracts.md).

## 1. Architectural Role

The TUI Presenter is a pure *consumer* of the Render Pipeline ([PRD 02a](./02a-composition-core-and-render-pipeline.md) §5) and the Kitty Display Adapter ([PRD 03a](./03a-kitty-graphics-protocol-driver.md)).
- It owns UI layouts, user input routing, presentation state machines, and timer states.
- It **never** schedules worker rendering, manages Image IDs directly, or generates escape sequence bytes.

---

## 2. Modal Interaction State Machine

```
               ┌──────────────────────────────────────────┐
               │         Mode 1: Fullscreen Slide         │◄───────┐
               └──────┬────────────────────────────┬──────┘        │
       toggle 'p'     │                            │ toggle 'Tab'  │ Esc /
                      ▼                            ▼               │ Enter
┌───────────────────────────────┐        ┌────────────────────────┐│
│ Mode 2: Presenter Console     │        │ Mode 3: Slide Sorter   ││
│ (Notes, Timer, Next Thumbnail)│        │ (Visual Thumbnail Grid)││
└──────────────┬────────────────┘        └────────────────────────┘│
               │                                                   │
               │ Overlay triggers ('?', 'W', 'c', '/', 'b')        │
               ▼                                                   │
┌─────────────────────────────────────────────────────────────────┐│
│ Overlays: Help ('?') · Warnings ('W') · Comments ('c')          │┘
│           Fuzzy TOC ('/') · Blackout ('b') / Whiteout ('w')     │
└─────────────────────────────────────────────────────────────────┘
```

### 2.1 State & Transition Matrix

| Current State | Trigger Key | Next State | Action / Visual Behavior |
| :--- | :--- | :--- | :--- |
| **`SlideView`** | `p` | `PresenterConsole` | Switches to split view with notes and next-slide thumbnail. |
| **`SlideView`** | `Tab` / `s` | `SlideSorter` | Prefetches card-sized thumbnail targets for the visible page and displays the grid. |
| **`SlideView`** | `b` / `w` | `Blackout` / `Whiteout` | Clears slide cells to solid black/white; resident image retained. |
| **Any Base** | `?` / `F1` | `HelpModal` | Displays centered modal with all keybindings; Esc dismisses. |
| **Any Base** | `W` (Shift-W) | `WarningDrawer` | Slides out bottom drawer listing all slide `WarningEvent`s. |
| **Any Base** | `c` | `CommentsDrawer` | Opens right drawer displaying reviewer comments (`Slide.comments`). |
| **Any Base** | `/` / `Ctrl+F` | `TocSearchModal` | Opens fuzzy slide search popup with interactive input. |
| **Overlay** | `Esc` / `q` | Previous Base State | Closes active overlay and restores previous focus. |

---

## 3. Ratatui Custom Slide Widget Integration

```rust
pub struct SlideWidget<'a> {
    pub slide_index: usize,
    pub pipeline_entry: PipelineEntry,
    pub placement_mode: PlacementMode,
}
```

### 3.1 Direct Placement Rendering
- `SlideWidget::render` computes the aspect-preserving fitted rectangle inside the available area and clears those cells in `ratatui::buffer::Buffer`.
- Immediately following `terminal.draw()`, the TUI triggers `driver.present(id, Placement::Cells(rect))` to position the Kitty image over the blank terminal region.
- Any active overlay (e.g. `HelpModal` or `TocSearchModal`) is drawn after the slide and composites above the image because the placement z-index is below `INT32_MIN/2`, which Kitty and Ghostty draw beneath non-default cell backgrounds.
- The layout is recomputed on every resize from the deck's original aspect ratio, so slides always best-fit without stretching.

---

## 4. Presenter Console Mode (Split View)

The split layout reserves $\approx 70\%$ of screen width for the main slide canvas and $30\%$ for presenter assistance widgets:

```
┌───────────────────────────────────────────────┬────────────────────────┐
│                                               │ Next Slide (Thumbnail) │
│                                               │ ┌────────────────────┐ │
│                                               │ │ [ 320x180 Image ]  │ │
│                                               │ └────────────────────┘ │
│             Current Slide Canvas              ├────────────────────────┤
│           (1920x1080 Native Render)           │ Speaker Notes          │
│                                               │                        │
│                                               │ 1. Highlight Q3 margin │
│                                               │ 2. Address supply chain│
│                                               │ 3. Open for questions  │
├───────────────────────────────────────────────┴────────────────────────┤
│ Slide 5 / 24  │ 00:14:22 Elapsed (Target 20m) │ 15:30:12 │ [?] Help    │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Speaker Notes Component
- Notes text extracted from `Slide.notes` is wrapped using Ratatui's `Paragraph` widget with word wrapping.
- Navigation: `Up` / `Down` arrows or mouse scroll wheel adjusts `notes_scroll_offset` when the notes pane is focused.

### 4.2 Presentation Stopwatch & Overtime Warning
- Measures elapsed wall-clock presentation time.
- Compares elapsed time against `[presenter] target_duration_mins`:
  - **Normal** ($< 90\%$): Stopwatch text is rendered in green.
  - **Warning** ($90\dots100\%$): Stopwatch text renders in bold yellow.
  - **Overtime** ($> 100\%$): Stopwatch text renders in flashing bold red with an `[OVERTIME]` badge.
- Controls: `r` resets timer to `00:00:00`; `s` pauses / resumes the stopwatch.

---

## 5. Visual Slide Sorter & Fuzzy TOC Search

### 5.1 Slide Sorter Grid
- Activated via `Tab` or `s`.
- Dynamically computes thumbnail cell grid based on terminal dimensions (e.g. $4\times 3$ or $5\times 4$), capped at `MAX_THUMBNAIL_IMAGES` (16) visible cards.
- Places one directly-positioned image per visible card, rasterized at the dedicated aspect-derived **sorter target** (1280 px wide, see [PRD 03a §5.1](./03a-kitty-graphics-protocol-driver.md)) and downscaled to ~480 px before transmission; the terminal scales it into the card. 1280 px keeps text runs above the rasterizer's 6 px font-size floor, and the aspect-derived target survives resizes and zoom changes.
- Prefetches the visible window at that same sorter target; at most 4 cards are transmitted per frame so scrolling never blocks the event loop. The sync reports how many ready cards that throttle deferred, and the run loop keeps drawing frames until the count reaches zero — with a warm cache nothing schedules workers, so no completion event could otherwise wake the loop (the sorter shows no clock), and the grid used to stay four-cards-full until the next key press.
- Releases cards that scroll out of view immediately. Leaving the grid releases every card, but only after the slide view has placed its own bitmap — tearing the grid down first blanked the screen for the length of the canvas transmit.
- Entering or leaving the grid never re-transmits a displayed image id: the canvas upgrades from the card raster to the canvas raster through the spare-id swap described in [PRD 03a §5.1](./03a-kitty-graphics-protocol-driver.md), so the slide never blinks while it sharpens.
- Displays slide numbers, titles, and dimmed borders for `hidden` slides.
- Arrow keys navigate the grid cursor; `Enter` jumps to selected slide; `Esc` cancels.

### 5.2 Fuzzy TOC Search (`/`)
- Centered popup window with search text input.
- Real-time substring and fuzzy matching across slide titles, numbers, notes, and outline section numbers.
- Matched slide list updates on each keystroke. Pressing `Enter` navigates directly to the matched slide.
- Documents with a bookmark outline (PDF) render it as a collapsible tree; each row is `<tree number> <title> [<page>]` (e.g. `2.1 Sub Chapter [64]`) with the page column right-aligned. Flat decks (PPTX) keep the leading `[page]` prefix and gain no numbers.

---

## 6. External Media Playback (`M`)

While audio/video playback inside terminal cells is a non-goal ([PRD 00](./00-system-overview.md) §7), presentations frequently include crucial demo clips:
1. When the current slide contains a `SlideElement::Media` item, a `[M: Play Media]` badge appears in the status bar.
2. Pressing `M` triggers the loader to extract the embedded media archive part (`ppt/media/mediaN.mp4`) into a secure temporary file.
3. Spawns the configured external player process (`[media] player = "mpv"`, [PRD 05](./05-cli-and-export.md) §3) in a detached subprocess.
4. The temporary file is tracked and cleaned up on application exit.

---

## 7. Event Loop & Terminal Lifecycle

```rust
loop {
    terminal.draw(|frame| render_ui(frame, &mut app_state))?;
    
    // 50 ms timeout drives clock and stopwatch updates
    if crossterm::event::poll(Duration::from_millis(50))? {
        match crossterm::event::read()? {
            Event::Key(key)     => handle_key(key, &mut app_state)?,
            Event::Resize(..)   => handle_resize(&mut app_state)?,
            Event::Mouse(mouse) => handle_mouse(mouse, &mut app_state)?,
        }
    }
}
```

### 7.1 Resize Handling (Zero Re-Rasterization)
Because terminal resident graphics scale client-side ([PRD 03a](./03a-kitty-graphics-protocol-driver.md) §4):
1. Terminal resize event triggers layout recomputation.
2. Recomputed `CellRect` is updated in the Ratatui buffer.
3. In direct placement mode, calls `driver.reposition(id, Cells(new_rect))`.
4. **No raster caches are invalidated**; no slide re-rendering occurs.
