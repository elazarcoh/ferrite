# UI Polish & Help System Spec
**Date:** 2026-03-19

## Problem

The sprite editor and config dialog have four usability gaps:

1. **Frame strip is hard to navigate** — frames shown as a single horizontal scrolling row of small thumbnails; no sense of the full sheet layout.
2. **Sprite editor sidebar has no full scroll** — tag map section can be cut off when the window is small.
3. **No visual polish** — default egui appearance; no rounding, spacing, or theming.
4. **No in-app help** — users must guess what fields mean (tag map, grid cols/rows, scale, etc.).

## Goals

- Show the full sprite sheet PNG with a grid overlay in the sprite editor central panel; clicking a cell selects that frame.
- Wrap the entire sprite editor sidebar in a single unrestricted `ScrollArea::vertical()`.
- Apply a polished theme (rounded corners, consistent spacing, green accent) with a dark/light toggle shared across both dialogs.
- Add inline hint text to simple fields and `?` tooltip icons to complex sections in both dialogs.

## Non-Goals

- Drag-to-reorder frames or tags.
- Animated GIF export.
- Per-pet theme settings.
- Custom font or icon loading.

---

## Design

### 1. Shared Theme Module (`src/tray/ui_theme.rs`)

New file. Three public functions:

#### `apply_theme(ctx: &egui::Context, dark: bool)`

Calls `ctx.set_visuals()` with a hand-tuned `Visuals` struct.

**Both modes share:**
- `window_rounding = Rounding::same(8.0)`
- Widget rounding: `Rounding::same(4.0)` for interactive, `Rounding::same(6.0)` for noninteractive
- `style.spacing.item_spacing = vec2(8.0, 6.0)`
- `style.spacing.button_padding = vec2(10.0, 5.0)`
- `style.spacing.window_margin = Margin::same(12.0)`
- Selected/active accent: `Color32::from_rgb(72, 200, 120)` (green)

**Dark palette:**
- Window background: `Color32::from_rgb(18, 18, 30)` (`#12121e`)
- Panel background: `Color32::from_rgb(24, 24, 42)`
- Widget fill: `Color32::from_rgb(35, 35, 60)`
- Border: `Color32::from_rgba_premultiplied(100, 120, 200, 60)`
- Text: `Color32::from_rgb(210, 215, 230)`
- Weak text (hints): `Color32::from_rgb(100, 110, 140)`

**Light palette:**
- Window background: `Color32::from_rgb(244, 244, 248)` (`#f4f4f8`)
- Panel background: `Color32::from_rgb(255, 255, 255)`
- Widget fill: `Color32::from_rgb(235, 235, 245)`
- Border: `Color32::from_rgba_premultiplied(140, 150, 200, 120)`
- Text: `Color32::from_rgb(30, 30, 50)`
- Weak text (hints): `Color32::from_rgb(130, 130, 160)`

#### `help_icon(ui: &mut egui::Ui, tooltip: &str)`

Renders a small `?` circle. On hover, shows a styled tooltip with the provided text.

```rust
pub fn help_icon(ui: &mut egui::Ui, tooltip: &str) {
    ui.add(egui::Label::new(
        egui::RichText::new(" ? ")
            .small()
            .color(ui.visuals().weak_text_color()),
    ))
    .on_hover_text(tooltip);
}
```

Callers place it inline after a section label using `ui.horizontal(|ui| { ui.label("Section"); help_icon(ui, "..."); })`.

#### `hint(ui: &mut egui::Ui, text: &str)`

Renders one line of small, italic, weak-colored text below a widget.

```rust
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(
        egui::RichText::new(text).small().italics().weak(),
    ));
}
```

#### `dark_light_toggle(ui: &mut egui::Ui, dark: &mut bool, ctx: &egui::Context)`

Renders a `☀` / `☾` button. On click: flips `*dark` and calls `apply_theme(ctx, *dark)`.

---

### 2. App Integration (`src/app.rs`)

`App` gains one field:

```rust
dark_mode: bool,   // default: true
```

Both `ConfigWindowState` and `SpriteEditorViewport` gain a matching `dark_mode: bool` field. On each `App::update()` call, `dark_mode` is copied into both viewport states so a toggle in either dialog immediately propagates.

`src/tray/mod.rs` gains `pub mod ui_theme;`.

---

### 3. Sprite Editor (`src/tray/sprite_editor.rs`)

#### 3a. Full PNG Grid View (central panel)

The horizontal frame strip is **replaced** by:

1. Draw the full sprite sheet texture scaled to fill the available central panel width, preserving aspect ratio.
2. Over the texture, use `ui.painter()` to draw:
   - `(cols + 1)` vertical lines and `(rows + 1)` horizontal lines — semi-transparent (`Color32::from_rgba_premultiplied(200, 200, 200, 60)`).
   - A rect stroke around the selected cell — green accent, 2px.
   - Frame index number in the top-left corner of each cell — small, semi-transparent.
3. `ui.allocate_rect(image_rect, egui::Sense::click())` captures mouse clicks on the sheet area. On click:
   - `col = ((pos.x - rect.left()) / cell_w) as usize`
   - `row = ((pos.y - rect.top()) / cell_h) as usize`
   - `selected_frame = (row * cols + col).min(total_frames - 1)`
4. The prev/next buttons and animation preview continue to work using `selected_frame`.

The `SpriteEditorViewport` struct keeps the existing `selected_frame: usize` field. No new state is needed.

#### 3b. Sidebar Scroll

The entire left sidebar content (grid sliders through tag map) is wrapped in one `ScrollArea::vertical().show(ui, |ui| { ... })` with no `max_height` cap. The existing 250px-capped tag list scroll is removed — the outer scroll covers everything.

Sidebar panel fixed width: `200.0`.

#### 3c. Help in Sprite Editor

| Location | Type | Text |
|----------|------|------|
| "Grid" section header | `help_icon` | `"Sets how the PNG is divided into frames. Cols × Rows = total frame count."` |
| "Tags" section header | `help_icon` | `"Tags group frames into named animations (e.g. 'idle', 'walk'). Select a tag to edit its frame range and direction."` |
| Direction field | `hint` | `"Forward plays frames left-to-right. PingPong bounces back and forth."` |
| "Tag Map" section header | `help_icon` | `"Maps pet behaviors to your tag names. idle and walk are required; others fall back to idle if not set."` |

#### 3d. Dark/Light Toggle

Added to the top panel alongside the Save and Export buttons:
```rust
ui_theme::dark_light_toggle(ui, &mut self.dark_mode, ctx);
```

`apply_theme(ctx, self.dark_mode)` is called at the top of the viewport show function.

---

### 4. Config Dialog (`src/tray/config_window.rs`)

#### 4a. Help in Config Dialog

| Location | Type | Text |
|----------|------|------|
| Sheet selector label | `help_icon` | `"Choose a sprite sheet from your installed library, or use 'New from PNG' to import one."` |
| Scale field | `hint` | `"Pixel upscale factor. 2× is recommended for 32px sprites."` |
| Walk speed field | `hint` | `"How fast the pet walks across the screen (pixels/second)."` |
| Flip walk left checkbox | `hint` | `"Mirror the sprite horizontally when walking left. Only needed if your sheet has no left-facing frames."` |
| "Tag Map" section header | `help_icon` | `"Maps pet behaviors to your tag names. idle and walk are required; others fall back to idle if not set."` |

#### 4b. Dark/Light Toggle

Same `dark_light_toggle` call added to the top bar of the config dialog. `apply_theme(ctx, self.dark_mode)` called at viewport top.

---

## File Changes Summary

| File | Change |
|------|--------|
| `src/tray/ui_theme.rs` | **New** — `apply_theme`, `help_icon`, `hint`, `dark_light_toggle` |
| `src/tray/mod.rs` | Add `pub mod ui_theme` |
| `src/tray/sprite_editor.rs` | Replace frame strip with PNG grid view; full sidebar scroll; help icons/hints; dark_mode field + toggle |
| `src/tray/config_window.rs` | Help icons/hints; dark_mode field + toggle; apply_theme |
| `src/app.rs` | Add `dark_mode: bool` (default `true`); sync to viewport states |

## Testing

No new tests. All 138 existing tests must continue to pass. Visual rendering is not unit-tested. Manual verification: open both dialogs, toggle dark/light, click sheet cells, scroll sidebar, hover help icons.
