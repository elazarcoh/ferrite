# Config Dialog Redesign

**Date:** 2026-03-16
**Status:** Approved

## Goal

Replace the current Win95-era config dialog with a modern, sprite-first design that lets users browse, preview, and install custom sprites rather than typing file paths.

## Decisions Made

| Question | Decision |
|---|---|
| Layout | Option C — sprite-first, pets as chips at top, gallery + animated preview as centerpiece, settings inline |
| Visual style | Dark theme (VS Code-ish: #1e1e1e bg, #007acc accent) |
| Custom sprite storage | Copy PNG+JSON to `%APPDATA%\my-pet\sprites\` on install — permanent gallery entry |
| Preview animation | Animated — idle tag loops in preview pane at 100 ms/tick |
| Code structure | Split B — three focused files (pure model / gallery logic / Win32 glue) |

---

## Dialog Layout

```
┌─ My Pet — Configure ────────────────────────────────────────────────────┐
│                                                                          │
│  PETS                                                                    │
│  [🐑 eSheep ×]  [+ Add pet]                                             │
│  ─────────────────────────────────────────────────────────────────────  │
│  SPRITE FOR eSheep                                                       │
│                                                                          │
│  ┌─────────────────────┐  ┌────────────────────────────────────────┐   │
│  │ ▣ eSheep   built-in │  │                                        │   │
│  │   test_pet built-in │  │           [animated idle frame]        │   │
│  │   my_cat   custom   │  │                  🐑                    │   │
│  │   📁 Browse…        │  │        idle · frame 1/2 · 40×40 ● live │   │
│  └─────────────────────┘  │                                        │   │
│                            ├────────────────────────────────────────┤   │
│                            │ FRAMES 51  TAGS 11  SIZE 40×40         │   │
│                            │ SOURCE built-in                        │   │
│                            └────────────────────────────────────────┘   │
│  ─────────────────────────────────────────────────────────────────────  │
│  Scale [2×]   X [100]   Y [800]   Speed [80]                           │
│                                                          [Cancel] [Save] │
└──────────────────────────────────────────────────────────────────────────┘
```

Dialog size: ~560 × 440 px.

---

## File Structure

### 1. `src/config/dialog_state.rs` (extracted from `tray/config_window.rs`)

Pure model — no Win32 imports. Fully testable.

```rust
pub struct ConfigDialogState {
    pub config: Config,
    pub selected_pet: usize,
    pub selected_sprite: Option<SpriteKey>,  // currently highlighted in gallery
    pub result: DialogResult,
}

pub enum SpriteKey {
    Embedded(String),       // e.g. "esheep", "test_pet"
    Installed(PathBuf),     // in %APPDATA%\my-pet\sprites\
}

pub enum DialogResult { None, Ok, Cancel }
```

Methods (all pure, no Win32):
- `add_pet()` → appends default pet, selects it
- `remove_selected_pet()` → removes current pet, clamps index
- `select_pet(index)` → bounds-checked selection
- `select_sprite(key: SpriteKey)` → updates selected pet's `sheet_path`
- `update_scale(s: &str) -> bool`
- `update_x(s: &str) -> bool`
- `update_y(s: &str) -> bool`
- `update_walk_speed(s: &str) -> bool`
- `accept()` / `cancel()`

All existing `config_dialog_e2e` tests continue to pass against this module.

---

### 2. `src/window/sprite_gallery.rs` (new)

Discovers and manages available sprites. No dialog state.

```rust
pub struct GalleryEntry {
    pub key: SpriteKey,
    pub display_name: String,           // stem of the file, e.g. "eSheep"
    pub source_label: &'static str,     // "built-in" | "custom"
    pub thumbnail: Option<HBITMAP>,     // 28×28 first frame of idle tag, None until loaded
}

pub struct SpriteGallery {
    pub entries: Vec<GalleryEntry>,
}
```

**`SpriteGallery::load() -> Self`**
- Starts with all embedded sprites (via `assets::embedded_sheet` stem list: `["esheep", "test_pet"]`)
- Scans `%APPDATA%\my-pet\sprites\*.json` for installed custom sprites
- Does NOT load thumbnails yet (lazy)

**`SpriteGallery::load_thumbnail(entry: &mut GalleryEntry)`**
- Loads the spritesheet for the entry
- Renders the first frame of the `idle` tag (or frame 0 if no idle tag) at native resolution
- Creates a 28×28 DIBSection via `StretchDIBits`, stores `HBITMAP`
- Called on demand (when a gallery card is about to be painted)

**`SpriteGallery::install(json_path: &Path) -> Result<GalleryEntry>`**
- Validates: parse JSON, check paired `.png` exists at same stem
- Creates `%APPDATA%\my-pet\sprites\` if absent
- Copies `<stem>.json` and `<stem>.png` into that directory
- If a file with the same name already exists, overwrites
- Returns the new `GalleryEntry` (thumbnail not yet loaded)

**`SpriteGallery::appdata_sprites_dir() -> PathBuf`**
- Returns `%APPDATA%\my-pet\sprites\`
- Testable via environment variable override in tests

---

### 3. `src/tray/config_window.rs` (rewritten — Win32 glue only)

All Win32 interaction. Dark theme. Drives animation via `WM_TIMER`.

#### Dark theme implementation

| Element | Technique |
|---|---|
| Dialog background | Window class `hbrBackground = CreateSolidBrush(RGB(30,30,30))` |
| STATIC label text | `WM_CTLCOLORSTATIC` → `SetTextColor(#858585)` + return dark brush |
| EDIT control bg/text | `WM_CTLCOLOREDIT` → `SetTextColor(#cccccc)`, `SetBkColor(#3c3c3c)` |
| Cancel button | `WM_CTLCOLORBTN` → standard draw, dark bg brush |
| Save button | `BS_OWNERDRAW` + `WM_DRAWITEM` → filled `#007acc` rect + white text |
| Pet chips | Custom child window class `"PetChip"` — `WM_PAINT` draws rounded rect via `RoundRect`, handles `WM_LBUTTONDOWN` for × click |
| Gallery cards | `LBS_OWNERDRAWFIXED` listbox — `WM_DRAWITEM` draws thumbnail + name + selection highlight |
| Preview pane | Custom child window class `"SpritePreview"` — `WM_PAINT` BitBlts current animation frame |

#### Animation timer

- `SetTimer(hwnd, TIMER_ANIM, 100, NULL)` on `WM_CREATE`
- `WM_TIMER` → tick `AnimationState` by 100 ms → call `InvalidateRect(preview_hwnd, NULL, FALSE)` + `UpdateWindow(preview_hwnd)`
- `SpritePreview::WM_PAINT` → reads current frame from `AnimationState` → renders via off-screen DIBSection + `BitBlt`
- `KillTimer` on `WM_DESTROY`

#### Custom sprite browse + install flow

1. User clicks "Browse…" gallery entry
2. `GetOpenFileNameW` filtered to `*.json`
3. `SpriteGallery::install()` — validate + copy to `%APPDATA%`
4. New `GalleryEntry` appended to gallery; listbox refreshed
5. New entry auto-selected; animation state reset to new sheet's idle tag
6. Preview begins animating immediately

#### Control IDs (extended from current)

```
ID_LIST_GALLERY  = 101   (owner-draw LBS for sprite cards)
ID_BTN_ADD_PET   = 102
ID_BTN_REMOVE_PET= 103
ID_EDIT_SCALE    = 106
ID_EDIT_X        = 108
ID_EDIT_Y        = 109
ID_EDIT_SPEED    = 110   (new)
DLG_OK           = 1
DLG_CANCEL       = 2
TIMER_ANIM       = 1001
```

Pet chips are child windows (not control IDs) — each chip is a `HWND` child of the dialog, identified by pet index stored in `GWLP_USERDATA`.

---

## Data Flow

```
show_config_dialog(parent, config)
  └─ ConfigDialogState::new(config)
  └─ SpriteGallery::load()               ← discovers embedded + installed sprites
  └─ CreateWindowExW (dialog)
       └─ WM_CREATE → create_controls()
            └─ populate_gallery(listbox, gallery)
            └─ create_pet_chips(chips, state)
            └─ SetTimer(TIMER_ANIM, 100)
       └─ WM_TIMER → anim.tick(100) → InvalidateRect(preview)
       └─ WM_DRAWITEM (gallery listbox) → draw_gallery_card(...)
       └─ WM_COMMAND
            ID_BTN_ADD_PET   → state.add_pet() → refresh chips
            ID_BTN_REMOVE_PET→ state.remove_selected_pet() → refresh chips
            gallery select   → state.select_sprite(key) → reset anim → refresh preview info
            Browse card      → gallery.install(path) → append entry → select it
            DLG_OK           → read_fields() → state.accept() → DestroyWindow
            DLG_CANCEL       → state.cancel() → DestroyWindow
       └─ WM_DESTROY → KillTimer → drop gallery bitmaps
  └─ returns Some(state.config) or None
```

---

## Testing

### Existing tests (unchanged)
All `config_dialog_e2e` tests operate on `ConfigDialogState` directly — they continue to pass without modification.

### New unit tests

| Test | Location | What it verifies |
|---|---|---|
| `install_sprite_copies_files` | `sprite_gallery` | JSON+PNG copied to temp appdata dir |
| `install_sprite_rejects_missing_png` | `sprite_gallery` | Returns Err if .png absent |
| `install_sprite_overwrites_existing` | `sprite_gallery` | Second install of same name succeeds |
| `gallery_load_finds_installed` | `sprite_gallery` | After install, `load()` returns the entry |
| `dialog_state_select_sprite_updates_path` | `dialog_state` | `select_sprite(key)` updates `sheet_path` |
| `dialog_state_update_walk_speed` | `dialog_state` | Parses valid/invalid values |

---

## Out of Scope

- Dark mode auto-detection (follows system theme) — future work
- Animation tags other than idle in preview — future work
- Drag-to-reorder pets — future work
- Multi-select remove — future work
