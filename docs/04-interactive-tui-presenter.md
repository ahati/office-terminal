# PRD 04: Interactive TUI & Presenter Mode

## 1. Overview
The Interactive TUI transforms `kitty-pptx` from a static image viewer into a live presentation console. Built on **`ratatui`** and **`crossterm`**, it combines graphics rendering with terminal widgets, slide navigation, speaker notes, an elapsed presentation timer, and a visual slide sorter.

---

## 2. Display Modes & Layouts

The presenter interface supports three distinct layout modes toggled via hotkeys:

### 2.1 Mode 1: Fullscreen Slide View (Default)
Optimized for sharing screen or presenting to an audience.
```
┌────────────────────────────────────────────────────────────────────────┐
│                                                                        │
│                                                                        │
│                     Rendered Slide Canvas                              │
│                      (Kitty Graphics Image)                            │
│                                                                        │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│ Slide 4 / 28  │ Next: System Architecture │ 00:08:45 │ [?] Help        │
└────────────────────────────────────────────────────────────────────────┘
```
- Slide canvas automatically occupies 100% of the upper window area.
- Minimal bottom status bar shows:
  - Slide position: `Slide X of Y` + progress indicator (`[===>   ]`).
  - Next slide title preview.
  - Elapsed presentation timer.
- Pressing `h` toggles the bottom bar on/off for zero-distraction presentation.

---

### 2.2 Mode 2: Presenter Console (Split View)
Optimized for dual-monitor setups or rehearsal on a single screen.
```
┌───────────────────────────────────────────────┬────────────────────────┐
│                                               │ Next Slide             │
│                                               │ ┌────────────────────┐ │
│                                               │ │ [Thumbnail Mini]   │ │
│                                               │ └────────────────────┘ │
│             Current Slide Canvas              ├────────────────────────┤
│            (Kitty Graphics Image)             │ Speaker Notes          │
│                                               │                        │
│                                               │ • Emphasize 15ms latency│
│                                               │ • Mention cosmic-text  │
│                                               │ • Ask audience for Q&A │
│                                               │                        │
├───────────────────────────────────────────────┴────────────────────────┤
│ Slide 4 / 28  │ 00:08:45 Elapsed │ Local Time: 14:32:05 │ Tab: Sorter  │
└────────────────────────────────────────────────────────────────────────┘
```
- **Left Pane (70%)**: Current slide rendered at high resolution.
- **Top Right (30%)**: Scaled-down thumbnail of the upcoming slide.
- **Middle Right**: Scrollable speaker notes extracted from `ppt/notesSlides/`.
- **Bottom Bar**: Presentation stopwatch, target duration warning, local clock.

---

### 2.3 Mode 3: Visual Slide Sorter (Thumbnail Grid)
Activated via `Tab` or `t` for quick visual overview and jumping across sections.
```
┌────────────────────────────────────────────────────────────────────────┐
│ Slide Sorter (Use Arrow keys to select, Enter to jump, Esc to exit)   │
├──────────────┬──────────────┬──────────────┬──────────────┬────────────┤
│  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │            │
│  │ 1.Title│  │  │ 2.Intro│  │  │ 3.Arch │  │  │*4.Perf*│  │            │
│  └────────┘  │  └────────┘  │  └────────┘  │  └────────┘  │            │
├──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
│  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │            │
│  │ 5.Demo │  │  │ 6.Code │  │  │ 7.Road │  │  │ 8.Q&A  │  │            │
│  └────────┘  │  └────────┘  │  └────────┘  │  └────────┘  │            │
└──────────────┴──────────────┴──────────────┴──────────────┴────────────┘
```
- Dynamic grid adjusts columns and rows to match terminal size.
- Highlighted slide has a distinct active border.
- Instant jump on `Enter`.

---

## 3. Navigation & Keybindings

Keybindings are designed to feel intuitive for both presentation clickers and Vim users:

| Action | Primary Keys | Vim / Secondary Keys |
| :--- | :--- | :--- |
| **Next Slide** | `Space`, `Enter`, `Right`, `PageDown` | `j`, `l`, Mouse Click |
| **Previous Slide** | `Backspace`, `Left`, `PageUp` | `k`, `h`, Mouse Scroll Up |
| **First Slide** | `Home` | `g`, `gg` |
| **Last Slide** | `End` | `G` |
| **Direct Jump** | `[Number] + Enter` (e.g. `1` `2` `Enter`) | `[Number] + G` |
| **Toggle Presenter View** | `p` | `n` (Notes) |
| **Toggle Slide Sorter** | `Tab` | `t` |
| **Fuzzy Find / TOC Search**| `/` | `Ctrl+F` |
| **Screen Blackout** | `b` | `.` (Focus on speaker) |
| **Screen Whiteout** | `w` | `,` |
| **Reset / Pause Timer** | `r` (Reset), `s` (Pause/Resume) | - |
| **Help Overlay** | `?` | `F1` |
| **Quit** | `q`, `Esc` | `Ctrl+C` |

---

## 4. Background Pre-rendering & Caching Architecture

To achieve zero-latency slide flips (<16ms), the viewer implements a speculative rendering worker:

```
                    ┌─────────────────────────┐
                    │      Active Slide: N    │
                    │   (Displayed in Kitty)  │
                    └─────────────────────────┘
                                ▲
                 ┌──────────────┴──────────────┐
                 │                             │
    ┌────────────────────────┐    ┌────────────────────────┐
    │  Cached Slide: N - 1   │    │  Cached Slide: N + 1   │
    │  (Ready in Pixmap RAM) │    │  (Ready in Pixmap RAM) │
    └────────────────────────┘    └────────────────────────┘
                 │                             │
    ┌────────────────────────┐    ┌────────────────────────┐
    │  Pre-rendering: N - 2  │    │  Pre-rendering: N + 2  │
    │   (Worker Thread)      │    │   (Worker Thread)      │
    └────────────────────────┘    └────────────────────────┘
```

### Caching Specifications:
1. **Sliding Window**: Keeps an in-memory cache of at least 5 rendered slide pixmaps centered around current slide $N$: $[N-2, N-1, N, N+1, N+2]$.
2. **Worker Pool**: A dedicated background thread uses `tiny-skia` to rasterize upcoming slides before the user advances.
3. **Cache Eviction**: Slides beyond the active window distance are dropped to maintain memory usage below 150 MB.

---

## 5. Event Loop & Terminal Resize Handling

The event loop runs on `crossterm` event polling with non-blocking ticks:

```rust
loop {
    terminal.draw(|frame| {
        render_ui_layout(frame, &app_state);
    })?;

    // Poll for events with a 50ms tick rate (for timer updates)
    if event::poll(Duration::from_millis(50))? {
        match event::read()? {
            Event::Key(key) => handle_key_input(key, &mut app_state),
            Event::Resize(cols, rows) => handle_terminal_resize(cols, rows, &mut app_state),
            Event::Mouse(mouse) => handle_mouse(mouse, &mut app_state),
        }
    }
}
```

### Resize Debouncing
When dragging a terminal window border, dozens of `Resize` events fire per second:
- Resize events update layout bounds immediately.
- High-resolution re-rasterization is **debounced by 80ms** to prevent CPU thrashing during continuous window resize.
