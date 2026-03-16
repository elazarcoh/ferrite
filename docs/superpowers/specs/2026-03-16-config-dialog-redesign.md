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

Dialog size: **560 × 440 px** (up from current 510 × 390 to accommodate the gallery + preview layout).

---

## File Structure

### 1. `src/config/dialog_state.rs` (extracted from `tray/config_window.rs`)

Pure model — no Win32 imports. Fully testable.

```rust
pub struct ConfigDialogState {
    pub config: Config,
    pub selected: usize,            // index into config.pets (kept from current code)
    pub selected_sprite: SpriteKey, // currently highlighted gallery entry
    pub result: DialogResult,
}

/// Identifies a sprite in the gallery.
pub enum SpriteKey {
    Embedded(String),   // stem, e.g. "esheep" → sheet_path "embedded://esheep"
    Installed(PathBuf), // absolute path to the installed .json file
}

impl SpriteKey {
    /// Convert to the sheet_path string stored in PetConfig.
    pub fn to_sheet_path(&self) -> String {
        match self {
            SpriteKey::Embedded(stem) => format!("embedded://{stem}"),
            SpriteKey::Installed(p)   => p.to_string_lossy().into_owned(),
        }
    }

    /// Parse a sheet_path string back into a SpriteKey.
    pub fn from_sheet_path(s: &str) -> Self {
        if let Some(stem) = s.strip_prefix("embedded://") {
            SpriteKey::Embedded(stem.to_string())
        } else {
            SpriteKey::Installed(PathBuf::from(s))
        }
    }
}

pub enum DialogResult { None, Ok, Cancel }
```

**Methods** (all pure, no Win32):

- `new(config: Config) -> Self` — initializes `selected = 0`, `selected_sprite` derived from `config.pets[0].sheet_path` via `SpriteKey::from_sheet_path`
- `selected_pet(&self) -> Option<&PetConfig>` — unchanged from current
- `add_pet()` — appends default pet (eSheep config), sets `selected` to new index
- `remove_selected()` — removes current pet, clamps `selected` (unchanged API from current code)
- `select(index: usize)` — bounds-checked (unchanged API from current code)
- `select_sprite(key: SpriteKey)` — updates `self.selected_sprite` and calls `update_sheet_path(key.to_sheet_path())`
- `update_sheet_path(path: String)` — sets selected pet's `sheet_path` (unchanged from current)
- `update_scale(s: &str) -> bool` — unchanged from current (range 1–4)
- `update_x(s: &str) -> bool` — unchanged from current
- `update_y(s: &str) -> bool` — unchanged from current
- `update_walk_speed(s: &str) -> bool` — parses as `f32`; valid range 1.0–500.0 inclusive; rejects non-numeric, zero, and negative; returns false without mutating state on invalid input
- `accept()` / `cancel()` — unchanged

**Backward compatibility:** The existing `config_dialog_e2e` tests call `state.selected`, `state.select(...)`, `state.remove_selected()`, and `state.selected_pet()`. All these names are preserved exactly. The only addition is `selected_sprite` and `select_sprite()`; no renames.

---

### 2. `src/window/sprite_gallery.rs` (new)

Discovers and manages available sprites. Holds Win32 HBITMAP thumbnails; must be created and destroyed on the Win32 thread.

```rust
/// Whether a sprite is bundled with the app or user-installed.
pub enum SourceKind { BuiltIn, Custom }

pub struct GalleryEntry {
    pub key: SpriteKey,             // defined in crate::config::dialog_state; imported here
    pub display_name: String,       // filename stem, e.g. "eSheep"
    pub source: SourceKind,
    pub thumbnail: Option<HBITMAP>, // 28×28, None until load_thumbnail() called
}

/// Sentinel last entry — not a real sprite.
pub struct BrowseEntry;

pub struct SpriteGallery {
    pub entries: Vec<GalleryEntry>, // real sprites only; Browse is rendered separately
}
```

The "Browse…" card is **not** a `GalleryEntry`. It is drawn as a fixed final row by `WM_DRAWITEM` when the item index equals `entries.len()`, with the listbox having `entries.len() + 1` items. This keeps `entries` clean and avoids a sentinel key.

**`SpriteGallery::load() -> Self`**
- Discovers embedded sprites by iterating `Assets::iter()` (from `rust_embed`), collecting stems where both `<stem>.json` and `<stem>.png` are present (skips `test_pet` — internal only, not shown to users)
- Scans `appdata_sprites_dir()` for `*.json` files and adds `SourceKind::Custom` entries
- Does not load thumbnails; `thumbnail = None` for all entries

**`SpriteGallery::load_thumbnail(entry: &mut GalleryEntry)`**
- Loads the spritesheet for the entry (via `assets::embedded_sheet` or filesystem)
- Finds the `idle` tag; falls back to frame 0 if absent
- Renders that frame's source rect into a 28×28 DIBSection via `StretchDIBits`
- Stores the `HBITMAP` in `entry.thumbnail`
- Called lazily, just before painting a gallery card for the first time

**`SpriteGallery::destroy_thumbnails(&mut self)`**
- Calls `DeleteObject(bmp)` for every `entry.thumbnail` that is `Some`
- Sets each to `None`
- **Must be called from `WM_DESTROY`** before the struct is dropped to avoid GDI handle leaks

**`SpriteGallery::install(json_path: &Path) -> Result<GalleryEntry>`**
- Validates: parse the JSON (must be valid `SpriteSheet` JSON), check `<stem>.png` exists adjacent to the `.json`
- Creates `appdata_sprites_dir()` if absent
- Copies `<stem>.json` and `<stem>.png` into that directory (overwrites if same name already exists)
- Returns a new `GalleryEntry` with `thumbnail = None`; caller must call `load_thumbnail` before painting

**`SpriteGallery::appdata_sprites_dir() -> PathBuf`**
- Returns `%APPDATA%\my-pet\sprites\`
- In tests, overridable via `MY_PET_SPRITES_DIR` environment variable

---

### 3. `src/tray/config_window.rs` (rewritten — Win32 glue only)

Owns the window, all child HWNDs, `SpriteGallery`, and `AnimationState`. No pure logic.

#### Animation state in the preview

The dialog owns two items used for preview animation:

```rust
// Stored alongside the HWND state (e.g. in a heap-allocated context struct
// accessed via GWLP_USERDATA):
preview_sheet: Option<SpriteSheet>,   // currently previewed sheet
preview_anim:  AnimationState,        // ticked by WM_TIMER
```

On gallery selection change:
1. Load the new `SpriteSheet` from the selected `GalleryEntry`
2. Resolve idle tag name: find the first tag in `sheet.tags` whose name equals `"idle"` (case-insensitive); fall back to the first tag in the sheet; fall back to `""` (static first-frame display) if the sheet has no tags
3. Set `preview_anim = AnimationState::new(idle_tag_name)` (frame index 0, elapsed_ms 0)
4. Store sheet in `preview_sheet`
5. `InvalidateRect(preview_hwnd, NULL, FALSE)`

On `WM_TIMER` (100 ms):
1. If `preview_sheet` is `None`: skip — do nothing
2. Call `preview_anim.tick(&sheet, 100)` — the `bool` return value is intentionally ignored; we always repaint at a steady 10 Hz to avoid visible stutter on first-frame hold
3. `InvalidateRect(preview_hwnd, NULL, FALSE)` + `UpdateWindow(preview_hwnd)`

On `SpritePreview::WM_PAINT`:
1. If `preview_sheet` is `None`: fill the pane with the dark background color and return
2. Get absolute frame index via `preview_anim.absolute_frame(&sheet)`
3. Get frame rect from `sheet.frames[abs]`
4. Blit source pixels into off-screen DIBSection scaled to preview pane size
5. `BitBlt` to the preview window DC

#### Dark theme implementation

| Element | Technique |
|---|---|
| Dialog background | Window class `hbrBackground = CreateSolidBrush(RGB(30,30,30))` |
| STATIC label text | `WM_CTLCOLORSTATIC` → `SetTextColor(RGB(133,133,133))` + return dark brush |
| EDIT control bg/text | `WM_CTLCOLOREDIT` → `SetTextColor(RGB(204,204,204))`, `SetBkColor(RGB(60,60,60))` + return matching brush |
| Gallery listbox bg | `WM_CTLCOLORLISTBOX` → `SetBkColor(RGB(30,30,30))` + return dark brush |
| Save button (primary) | `BS_OWNERDRAW` + `WM_DRAWITEM` → filled `RGB(0,122,204)` rect + white text |
| Cancel button | `BS_OWNERDRAW` + `WM_DRAWITEM` → filled `RGB(60,60,60)` rect + `RGB(204,204,204)` text. **Note:** `WM_CTLCOLORBTN` is ignored on Vista+ with visual themes active; `BS_OWNERDRAW` must be used for both buttons to achieve consistent dark rendering. |
| Pet chips | Custom child window class `"PetChip"` — `WM_PAINT` draws `RoundRect` border + label; `WM_LBUTTONDOWN` on the `×` area notifies parent via `WM_COMMAND` |
| Gallery cards | `LBS_OWNERDRAWFIXED` + `WM_MEASUREITEM` (fixed height 44 px) + `WM_DRAWITEM` → draws thumbnail via `BitBlt` + name + source label + selection highlight |
| Preview pane | Custom child window class `"SpritePreview"` — `WM_PAINT` BitBlts current animation frame |

#### Gallery listbox details

The listbox has `entries.len() + 1` string items (index `entries.len()` is the Browse sentinel). In `WM_MEASUREITEM` all items have fixed height 44 px. In `WM_DRAWITEM`:
- Items `0..entries.len()`: draw thumbnail (or placeholder rect if thumbnail is `None`), display_name, source label
- Item `entries.len()`: draw the Browse card (folder icon + "Browse…" text, dashed border)
- Selection highlight: selected item draws `RGB(9,71,113)` background, `2px RGB(0,122,204)` left border

#### Retired control IDs

The following IDs from the current `config_window.rs` are removed:
- `ID_EDIT_PATH = 104` — replaced by gallery listbox
- `ID_BTN_BROWSE = 105` — Browse is now a gallery listbox entry
- `ID_EDIT_TAG = 107` — tag editing removed from UI (tag map managed automatically)

#### Control IDs

```
ID_LIST_GALLERY  = 101   // owner-draw LBS — sprite gallery + Browse sentinel
ID_BTN_ADD_PET   = 102
ID_BTN_REMOVE_PET= 103
ID_EDIT_SCALE    = 106
ID_EDIT_X        = 108
ID_EDIT_Y        = 109
ID_EDIT_SPEED    = 110   // new: walk_speed field
DLG_OK           = 1     // IDOK
DLG_CANCEL       = 2     // IDCANCEL
TIMER_ANIM       = 1001
```

Pet chips are child `HWND`s (not control IDs). Each stores its pet index in `GWLP_USERDATA`.

---

## Data Flow

```
show_config_dialog(parent, config)
  └─ ConfigDialogState::new(config)
  └─ SpriteGallery::load()           ← Assets::iter() + %APPDATA% scan
  └─ CreateWindowExW (dialog)
       └─ WM_CREATE
            ├─ create_controls()
            ├─ populate_gallery_listbox(gallery)   // entries.len()+1 items
            ├─ create_pet_chips(chips, state)
            ├─ load preview sheet for current sprite
            └─ SetTimer(TIMER_ANIM, 100)
       └─ WM_MEASUREITEM → return 44
       └─ WM_DRAWITEM (gallery) → draw_gallery_card(...)
       └─ WM_TIMER(TIMER_ANIM) → preview_anim.tick(&sheet, 100) → InvalidateRect(preview)
       └─ WM_COMMAND
            ID_BTN_ADD_PET    → state.add_pet() → refresh chips
            ID_BTN_REMOVE_PET → state.remove_selected() → refresh chips
            gallery LBN_SELCHANGE
              item < entries.len() → state.select_sprite(key) → reset anim → refresh info
              item == entries.len()→ browse_and_install() → append entry → select it
            DLG_OK     → read_fields() → state.accept() → DestroyWindow
            DLG_CANCEL → state.cancel() → DestroyWindow
       └─ WM_DESTROY
            ├─ KillTimer(TIMER_ANIM)
            └─ gallery.destroy_thumbnails()   // DeleteObject all HBITMAPs
  └─ returns Some(state.config) or None
```

---

## Testing

### Existing tests (all pass without modification)

All `config_dialog_e2e` tests operate on `ConfigDialogState` directly using the preserved API (`state.selected`, `state.select()`, `state.remove_selected()`, `state.selected_pet()`).

### New unit tests

| Test | Module | What it verifies |
|---|---|---|
| `install_sprite_copies_files` | `sprite_gallery` | JSON+PNG copied to `MY_PET_SPRITES_DIR` override dir |
| `install_sprite_rejects_missing_png` | `sprite_gallery` | Returns `Err` if `.png` absent adjacent to `.json` |
| `install_sprite_overwrites_existing` | `sprite_gallery` | Second install of same stem succeeds |
| `gallery_load_finds_installed` | `sprite_gallery` | After `install()`, `load()` returns the entry |
| `gallery_load_skips_test_pet` | `sprite_gallery` | `test_pet` is not in `entries` |
| `dialog_state_select_sprite_updates_path` | `dialog_state` | `select_sprite(Embedded("esheep"))` → `sheet_path == "embedded://esheep"` |
| `dialog_state_update_walk_speed_valid` | `dialog_state` | `"80"` and `"80.5"` accepted |
| `dialog_state_update_walk_speed_invalid` | `dialog_state` | `"0"`, `"-1"`, `"abc"`, `"501"` rejected |
| `sprite_key_roundtrip` | `dialog_state` | `from_sheet_path(key.to_sheet_path()) == key` for both variants |

---

## Out of Scope

- Dark mode auto-detection (system theme) — future work
- Preview of animation tags other than idle — future work
- Drag-to-reorder pets — future work
- Multi-select remove — future work
