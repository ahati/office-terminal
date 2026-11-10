# PRD 04: Interactive TUI & Presenter Mode

## 1. Overview

The Interactive TUI turns `kitty-pptx` from a static image viewer into a live presentation console. Built on **`ratatui`** and **`crossterm`**, it combines graphics display with terminal widgets, slide navigation, speaker notes, an elapsed timer, and a visual slide sorter.

**Architectural role**: the TUI is a *consumer* of the Render Pipeline ([PRD 02a](./02a-composition-core-and-render-pipeline.md) §5) and the Display Adapter ([PRD 03](./03-terminal-graphics-protocol.md)). It owns layout, input, and presentation state — **it does not schedule rendering, manage image ids, or emit protocol bytes itself**. All interfaces it consumes are specified in [PRD 06](./06-component-interface-contracts.md).

---

## 2. Display Modes & Layouts

### 2.1 Mode 1: Fullscreen Slide View (Default)
Optimized for sharing screen or presenting to an audience.
```
┌────────────────────────────────────────────────────────────────────────┐
│                                                                        │
│                                                                        │
│                     Rendered Slide Canvas                              │
│                   (terminal-resident image)                            │
│                                                                        │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│ Slide 4 / 28  │ Next: System Architecture │ 00:08:45 │ [?] Help        │
└────────────────────────────────────────────────────────────────────────┘
```
- Slide canvas occupies the upper area; placement is computed as a `CellRect` and handed to the driver (`present`).
- Status bar: slide position + progress, next-slide title, elapsed timer.
- `H` toggles the status bar for zero-distraction presenting.

### 2.2 Mode 2: Presenter Console (Split View)
For dual-monitor setups or rehearsal on a single screen.
```
┌───────────────────────────────────────────────┬────────────────────────┐
│                                               │ Next Slide             │
│                                               │ ┌────────────────────┐ │
│                                               │ │ [Thumbnail Mini]   │ │
│                                               │ └────────────────────┘ │
│             Current Slide Canvas              ├────────────────────────┤
│           (terminal-resident image)           │ Speaker Notes          │
│                                               │                        │
│                                               │ • Emphasize 15ms flip  │
│                                               │ • Mention determinism  │
│                                               │ • Ask audience for Q&A │
├───────────────────────────────────────────────┴────────────────────────┤
│ Slide 4 / 28  │ 00:08:45 Elapsed │ Local Time: 14:32:05 │ Tab: Sorter  │
└────────────────────────────────────────────────────────────────────────┘
```
- Left pane (~70%): current slide at full render target.
- Top right: next-slide thumbnail — a **separate render target** (`320×180` spec) with its own pipeline cache key space (PRD 02a §5.1).
- Middle right: scrollable speaker notes.
- Bottom bar: stopwatch, target-duration warning badge (config `[presenter] target_duration_mins`), local clock.

### 2.3 Mode 3: Visual Slide Sorter (Thumbnail Grid)
`Tab` / `t` for a visual overview and quick jumps.
```
┌────────────────────────────────────────────────────────────────────────┐
│ Slide Sorter (Arrow keys: select · Enter: jump · Esc: exit)           │
├──────────────┬──────────────┬──────────────┬──────────────┬────────────┤
│  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │            │
│  │ 1.Title│  │  │ 2.Intro│  │  │ 3.Arch │  │ │*4.Perf* │  │            │
│  └────────┘  │  └────────┘  │  └────────┘  │  └────────┘  │            │
├──────────────┼──────────────┼──────────────┼──────────────┼────────────┤
│  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │  ┌────────┐  │            │
│  │ 5.Demo │  │  │ 6.Code │  │  │ 7.Road │  │  │ 8.Q&A  │  │            │
│  └────────┘  │  └────────┘  │  └────────┘  │  └────────┘  │            │
└──────────────┴──────────────┴──────────────┴──────────────┴────────────┘
```
- Grid adapts columns/rows to terminal size; selected slide gets a distinct border.
- Opening the sorter issues a thumbnail-target `prefetch` for the visible grid window plus one row of look-ahead; the full-resolution cache is untouched (separate key space).
- `Enter` jumps; `Esc` returns.

---

## 3. Navigation & Keybindings

| Action | Primary | Vim / Secondary |
| :--- | :--- | :--- |
| **Next slide** | `Space`, `Enter`, `Right`, `PageDown` | `j`, `l` (skips hidden slides) |
| **Previous slide** | `Backspace`, `Left`, `PageUp` | `k`, `h` (skips hidden slides) |
| **First slide** | `gg` (prefix: `g` waits for second key) | `Home` |
| **Last slide** | `G` | `End` |
| **Direct jump** | number prefix + `Enter` (e.g. `1` `2` `Enter`) | number prefix + `G` (can jump to hidden slides) |
| **Toggle presenter view** | `p` | `n` (notes) |
| **Toggle sorter** | `Tab` | `t` |
| **Toggle status bar** | `H` | — |
| **Fuzzy find / TOC** | `/` | `Ctrl+F` |
| **Screen blackout / whiteout** | `b` / `w` | `.` / `,` |
| **Warning drawer** | `W` (Shift-W) | — |
| **Reviewer comments drawer** | `c` | — |
| **Play embedded media** | `M` (Shift-M) | Launch external player (e.g. `mpv`) |
| **Timer: reset / pause** | `r` / `s` | — |
| **Help overlay** | `?` | `F1` |
| **Quit** | `q`, `Esc` | `Ctrl+C` |

*(Note on keybinding hygiene: `w` is PowerPoint's canonical screen-whiteout key; the warning drawer is bound to `W`. `h` is strictly previous slide while `H` toggles the status bar.)*

### 3.1 Hidden Slide Semantics
PowerPoint decks often contain hidden slides (`<p:sld show="0">`, carried as `Slide.hidden: bool` in the model):
- **Linear Navigation (`Space`, `j`, `k`, `Enter`)**: Hidden slides are **automatically skipped**, matching PowerPoint presentation behavior.
- **Visual Slide Sorter (`Tab`)**: Hidden slides are displayed with a dimmed border and a `[#N (hidden)]` badge so the presenter can see their position.
- **Direct Jump (`<N>G`)**: Explicit jumps directly target the requested slide, even if hidden.

All bindings are rebindable via `[keybindings]` in the config ([PRD 05](./05-cli-and-export.md) §3).

---

## 4. Pipeline Consumption

The TUI holds `Arc<dyn RenderPipeline>` and a driver handle ([PRD 06](./06-component-interface-contracts.md) §7). Its total interaction with rendering:

| UI event | Pipeline / driver call |
| :--- | :--- |
| Slide change to `n` | `prefetch(PrefetchPlan { center: n, behind: 2, ahead: 2, stage: true }, full_spec)` |
| Sorter opened | `prefetch({ center: visible_window, stage: true }, thumb_spec)` |
| Flip requested (next/prev) | check `entry(n)` is `Ready { staged: true }` → `driver.present(id, Cells(rect))`; if `Rendering`, show current slide until ready (never a blank frame; PRD 03 §5.3) |
| Presenter view on | thumbnail `entry(n+1)` → small placement in right pane |
| Blackout `b` | present nothing; clear slide cells + fill black (image stays staged for instant restore) |

No other component behavior lives here: cache sizing, eviction, worker counts, staging policy, and invalidation are owned by the pipeline (PRD 02a §5).

---

## 5. Event Loop, Resize & Terminal Lifecycle

```rust
loop {
    terminal.draw(|frame| render_ui_layout(frame, &app_state))?;
    if event::poll(Duration::from_millis(50))? {   // 50 ms tick drives the timer
        match event::read()? {
            Event::Key(key)     => handle_key_input(key, &mut app_state)?,
            Event::Resize(..)   => handle_resize(&mut app_state)?,
            Event::Mouse(m)     => handle_mouse(m, &mut app_state)?,
        }
    }
}
```

**Resize handling — simplified.** Because the terminal scales resident images client-side (PRD 03 §4.2), a resize is a *placement* event, not a *rendering* event:

1. Recompute layout; recompute the slide `CellRect`.
2. `driver.reposition(id, Cells(new_rect))` — one escape sequence, no re-raster, no re-transmit.
3. Raster caches remain valid; nothing is invalidated unless the render **target spec itself** changes (`ActualPixels` toggle → `InvalidateCause::TargetChanged`, PRD 02a §5.3).

*(The previous revision's 80 ms debounced re-rasterization pipeline is removed — see PRD 02a §5.3.)*

**Lifecycle**: raw mode + alternate screen on entry; on exit (any path, incl. `SIGINT` and panic) the TUI restores modes and the driver runs its cleanup contract (PRD 03 §9).

---

## 6. Status, Warnings & Failure Surfaces

- **Warning badges**: `WarningEvent`s ([PRD 06](./06-component-interface-contracts.md) §4) from the current slide surface as a `⚠ n` badge in the status bar; `W` opens the warning drawer (element, feature, fidelity class).
- **Reviewer comments**: slides containing reviewer comments (`Slide.comments`) display a `💬` badge; `c` toggles the slide comments drawer.
- **Pipeline failure** (`entry(n) == Failed`): the slide area shows the placeholder presentation (PRD 02d §3) with the `RenderError` summary; navigation remains functional.
- **Display degradation**: on `DisplayError::Io` mid-session, the TUI attempts the half-block driver (PRD 03 §10) and posts a status message; if unavailable, it exits with code 3.
