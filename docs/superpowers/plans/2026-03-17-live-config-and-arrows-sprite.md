# Live Config Dialog & Arrows Sprite — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the test sprite as "arrows" in the gallery, and make the config dialog apply changes live (non-modal, no Save/Cancel).

**Architecture:** The gallery filter that hides `test_pet` is removed and its display name is mapped to "arrows". The config dialog's blocking inner message loop is removed; it returns an `HWND` immediately and routes through the existing app message loop. Changes are transmitted via a `crossbeam_channel::Sender<AppEvent>` stored in `DialogCtx` and handled by the app as `AppEvent::ConfigChanged`.

**Tech Stack:** Rust, windows-sys 0.61 (Win32 raw FFI), crossbeam-channel 0.5

---

## Chunk 1: arrows sprite in gallery

### Task 1: Expose test_pet as "arrows" in gallery

**Files:**
- Modify: `src/window/sprite_gallery.rs:65-71` (embedded_stems filter + load display name)
- Modify: `tests/integration/test_sprite_gallery.rs` (update gallery_load_skips_test_pet test)

- [ ] **Step 1: Update the failing test — rename and extend to verify "arrows" appears**

Open `tests/integration/test_sprite_gallery.rs`. Replace the test `gallery_load_skips_test_pet`:

```rust
#[test]
fn gallery_load_shows_arrows_not_test_pet() {
    let _sprites_dir = temp_sprites_dir();
    let gallery = SpriteGallery::load();
    let names: Vec<&str> = gallery.entries.iter().map(|e| e.display_name.as_str()).collect();
    // "test_pet" must not appear as a display name — it's remapped to "arrows"
    assert!(!names.contains(&"test_pet"), "test_pet must not appear as display name");
    // "arrows" (the renamed test_pet) must be present
    assert!(names.contains(&"arrows"), "arrows must appear in user-visible gallery");
    // eSheep is embedded and should appear
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("esheep")));
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test gallery_load_shows_arrows_not_test_pet -- --test-threads=1
```

Expected: FAIL — `arrows` not found in gallery entries.

- [ ] **Step 3: Implement — remove filter, add display name mapping**

In `src/window/sprite_gallery.rs`, in `embedded_stems()`, remove the `test_pet` filter:

```rust
// Before:
let mut stems: Vec<String> = jsons
    .intersection(&pngs)
    .filter(|s| *s != "test_pet")
    .cloned()
    .collect();

// After:
let mut stems: Vec<String> = jsons
    .intersection(&pngs)
    .cloned()
    .collect();
```

In `SpriteGallery::load()`, update the embedded sprite loop to map the display name:

```rust
// Before:
entries.push(GalleryEntry {
    key: SpriteKey::Embedded(stem.clone()),
    display_name: stem,
    source: SourceKind::BuiltIn,
    thumbnail: None,
});

// After:
let display_name = if stem == "test_pet" {
    "arrows".to_string()
} else {
    stem.clone()
};
entries.push(GalleryEntry {
    key: SpriteKey::Embedded(stem.clone()),
    display_name,
    source: SourceKind::BuiltIn,
    thumbnail: None,
});
```

- [ ] **Step 4: Run all gallery tests**

```bash
cargo test sprite_gallery -- --test-threads=1
```

Expected: all 5 gallery tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/window/sprite_gallery.rs tests/integration/test_sprite_gallery.rs
git commit -m "feat: expose test_pet as 'arrows' in sprite gallery"
```

---

## Chunk 2: Non-modal live config dialog

### Task 2: Add ConfigChanged event

**Files:**
- Modify: `src/event.rs`

- [ ] **Step 1: Add `ConfigChanged` variant**

In `src/event.rs`, add the new variant:

```rust
use crate::config::schema::Config;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Tick(u32),
    ConfigReloaded(Config),
    ConfigChanged(Config),   // ← add this line
    TrayAddPet,
    TrayRemovePet { pet_id: String },
    TrayOpenConfig,
    TrayQuit,
    PetClicked { pet_id: String },
    PetDragStart { pet_id: String, cursor_x: i32, cursor_y: i32 },
    PetDragEnd { pet_id: String, velocity: (f32, f32) },
    Quit,
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build 2>&1 | grep "^error"
```

Expected: no errors (the new variant is unused yet; warnings OK).

- [ ] **Step 3: Commit**

```bash
git add src/event.rs
git commit -m "feat: add AppEvent::ConfigChanged for live config apply"
```

---

### Task 3: Refactor config dialog to non-modal

**Files:**
- Modify: `src/tray/config_window.rs`

This task removes the blocking modal loop and wires live config-change notifications.

- [ ] **Step 1: Add imports**

At the top of `src/tray/config_window.rs`, add:

```rust
use crate::event::AppEvent;
use crossbeam_channel::Sender;
```

(These go alongside the existing `use crate::...` lines.)

- [ ] **Step 2: Add `tx` to `DialogCtx`, add `send_config_changed`, and replace `show_config_dialog` (atomic)**

These three changes touch the same call chain and must be made together — adding `tx` to `DialogCtx::new()` breaks the existing `show_config_dialog` caller until `show_config_dialog` itself is replaced.

**2a — Update `DialogCtx` struct and `new()`:**

Add `tx: Sender<AppEvent>` field:

```rust
struct DialogCtx {
    state: ConfigDialogState,
    gallery: SpriteGallery,
    chip_hwnds: Vec<HWND>,
    preview_hwnd: HWND,
    preview_sheet: Option<crate::sprite::sheet::SpriteSheet>,
    preview_anim: AnimationState,
    dark_bg_brush: HBRUSH,
    ctrl_brush: HBRUSH,
    card_brush: HBRUSH,
    tx: Sender<AppEvent>,           // ← new
}
```

Update `DialogCtx::new()` signature and body:

```rust
unsafe fn new(config: Config, tx: Sender<AppEvent>) -> Box<Self> {
    let state = ConfigDialogState::new(config);
    let gallery = SpriteGallery::load();
    Box::new(DialogCtx {
        state,
        gallery,
        chip_hwnds: Vec::new(),
        preview_hwnd: std::ptr::null_mut(),
        preview_sheet: None,
        preview_anim: AnimationState::new(""),
        dark_bg_brush: CreateSolidBrush(clr_bg()),
        ctrl_brush: CreateSolidBrush(clr_bg_ctrl()),
        card_brush: CreateSolidBrush(clr_bg()),
        tx,
    })
}
```

**2b — Add `send_config_changed` helper** near the other helpers:

```rust
/// Send the current config to the app for live apply.
unsafe fn send_config_changed(ctx: &DialogCtx) {
    let _ = ctx.tx.send(AppEvent::ConfigChanged(ctx.state.config.clone()));
}
```

**2c — Replace `show_config_dialog`** with the non-modal version:

```rust
/// Open the config dialog non-modally. Returns the dialog HWND (null on failure).
/// The caller must NOT block — the dialog routes through the app's existing message loop.
pub fn show_config_dialog(parent: HWND, config: &Config, tx: Sender<AppEvent>) -> HWND {
    register_classes();
    unsafe {
        let ctx = DialogCtx::new(config.clone(), tx);
        // Transfer ownership to Win32; reclaimed in WM_DESTROY via Box::from_raw.
        let ctx_ptr = Box::into_raw(ctx);

        let cls = wide(DLG_CLASS);
        let title = wide("My Pet \u{2014} Configure");
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | WS_VISIBLE;

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            cls.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT, CW_USEDEFAULT,
            560, 440,
            parent,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            // Reclaim the box to avoid a leak.
            drop(Box::from_raw(ctx_ptr));
            return std::ptr::null_mut();
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);
        setup_dialog_controls(hwnd, &mut *ctx_ptr);
        center_window(hwnd);
        hwnd   // return immediately — no message loop
    }
}
```

- [ ] **Step 5: Remove Save/Cancel — all at once (constants, create_controls, handle_command, WM_DRAWITEM)**

All references to `DLG_OK` and `DLG_CANCEL` must be removed in a single atomic edit to keep the code compilable at every step.

In `create_controls`, delete:
```rust
push_btn!("Cancel", DLG_CANCEL, 358, 395, 80, 28);
push_btn!("Save",   DLG_OK,    450, 395, 80, 28);
```

Delete the constant definitions:
```rust
const DLG_OK:     i32 = 1;
const DLG_CANCEL: i32 = 2;
```

In `handle_command`, delete these two arms:
```rust
DLG_OK => {
    read_fields(hwnd, &mut ctx.state);
    ctx.state.accept();
    DestroyWindow(hwnd);
}
DLG_CANCEL => {
    ctx.state.cancel();
    DestroyWindow(hwnd);
}
```

In `config_wnd_proc`'s `WM_DRAWITEM` handler, delete the arms that match `DLG_OK` and `DLG_CANCEL`.

After this step, `DLG_OK` and `DLG_CANCEL` must appear nowhere in the file.

- [ ] **Step 6: Add EN_KILLFOCUS handling for numeric fields in `handle_command`**

In `handle_command`, add a new arm for the edit controls. Add this **before** the `id if id >= 2000` catch-all arm:

```rust
ID_EDIT_SCALE | ID_EDIT_X | ID_EDIT_Y | ID_EDIT_SPEED => {
    if notify == EN_KILLFOCUS as u16 {
        read_fields(hwnd, &mut ctx.state);
        send_config_changed(ctx);
    }
}
```

- [ ] **Step 8: Send ConfigChanged on gallery selection, add/remove pet**

In `handle_command`, in the `ID_LIST_GALLERY` arm, add `send_config_changed(ctx)` after `state.select_sprite(key)`:

```rust
if sel < ctx.gallery.entries.len() {
    let key = ctx.gallery.entries[sel].key.clone();
    ctx.state.select_sprite(key);
    load_preview_for_sprite(ctx);
    refresh_fields(hwnd, &ctx.state);
    send_config_changed(ctx);   // ← add
} else if sel == ctx.gallery.entries.len() {
    if browse_and_install(hwnd, ctx).is_some() {
        let new_idx = ctx.gallery.entries.len() - 1;
        populate_gallery_listbox(hwnd, &ctx.gallery);
        let lb = GetDlgItem(hwnd, ID_LIST_GALLERY);
        SendMessageW(lb, LB_SETCURSEL, new_idx, 0);
        let key = ctx.gallery.entries[new_idx].key.clone();
        ctx.state.select_sprite(key);
        load_preview_for_sprite(ctx);
        refresh_fields(hwnd, &ctx.state);
        send_config_changed(ctx);   // ← add
    }
}
```

In the `ID_BTN_ADD_PET` arm:

```rust
ID_BTN_ADD_PET => {
    read_fields(hwnd, &mut ctx.state);
    ctx.state.add_pet();
    refresh_pet_chips(hwnd, ctx);
    refresh_fields(hwnd, &ctx.state);
    send_config_changed(ctx);   // ← add
}
```

In the `ID_BTN_REMOVE_PET` arm:

```rust
ID_BTN_REMOVE_PET => {
    ctx.state.remove_selected();
    refresh_pet_chips(hwnd, ctx);
    refresh_fields(hwnd, &ctx.state);
    send_config_changed(ctx);   // ← add
}
```

In the chip (`id >= 2000`) arm — **no** ConfigChanged needed (chip click only changes which pet is selected in the dialog, not the config itself).

- [ ] **Step 9: Update WM_CLOSE — read fields and notify before destroying**

Replace the existing `WM_CLOSE` handler:

```rust
WM_CLOSE => {
    let ctx = get_ctx(hwnd);
    if !ctx.is_null() {
        // Capture any value the user may have typed without clicking away.
        read_fields(hwnd, &mut (*ctx).state);
        send_config_changed(&*ctx);
    }
    DestroyWindow(hwnd);
    0
}
```

- [ ] **Step 10: Update WM_DESTROY — use Box::from_raw to reclaim DialogCtx**

Replace the existing `WM_DESTROY` handler:

```rust
WM_DESTROY => {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogCtx;
    if !ptr.is_null() {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        KillTimer(hwnd, TIMER_ANIM);
        // Reclaim ownership and drop — this frees gallery HBITMAPs and brushes.
        let ctx = Box::from_raw(ptr);
        ctx.gallery.destroy_thumbnails();
        ctx.destroy_brushes();
        // ctx dropped here
    }
    0
}
```

- [ ] **Step 11: Verify it compiles**

```bash
cargo build 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 12: Commit**

```bash
git add src/tray/config_window.rs
git commit -m "feat: make config dialog non-modal with live ConfigChanged notifications"
```

---

### Task 4: Update app.rs — handle ConfigChanged, store dialog HWND

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add `config_dialog_hwnd` field to `App`**

In the `App` struct:

```rust
pub struct App {
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    pets: HashMap<String, PetInstance>,
    _tray: SystemTray,
    _watcher: notify::RecommendedWatcher,
    last_tick_ms: std::time::Instant,
    timer_id: usize,
    config_dialog_hwnd: windows_sys::Win32::Foundation::HWND,  // ← new
}
```

In `App::new()`, initialize it:

```rust
Ok(App {
    tx,
    rx,
    pets,
    _tray: tray,
    _watcher: watcher,
    last_tick_ms: std::time::Instant::now(),
    timer_id: 0,
    config_dialog_hwnd: std::ptr::null_mut(),  // ← new
})
```

- [ ] **Step 2: Update `TrayOpenConfig` handler**

Replace the existing `TrayOpenConfig` arm in `handle_event`:

```rust
AppEvent::TrayOpenConfig => {
    unsafe {
        if !self.config_dialog_hwnd.is_null()
            && windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(self.config_dialog_hwnd) != 0
        {
            // Dialog already open — bring it to the front.
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(
                self.config_dialog_hwnd,
            );
        } else {
            let current = config::load(&config::config_path()).unwrap_or_default();
            self.config_dialog_hwnd = crate::tray::config_window::show_config_dialog(
                std::ptr::null_mut(),
                &current,
                self.tx.clone(),
            );
        }
    }
}
```

- [ ] **Step 3: Add `ConfigChanged` handler**

In `handle_event`, add the new arm (place it next to `ConfigReloaded`):

```rust
AppEvent::ConfigChanged(cfg) => {
    self.apply_config(cfg.clone())?;
    if let Err(e) = config::save(&config::config_path(), &cfg) {
        log::warn!("auto-save config failed: {e}");
    }
}
```

- [ ] **Step 4: Build and verify**

```bash
cargo build 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 5: Run all tests**

```bash
cargo test -- --test-threads=1
```

Expected: all 35 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat: app handles ConfigChanged with live apply and auto-save; non-modal dialog HWND stored"
```

---

## Verification

After all tasks:

```bash
cargo test -- --test-threads=1
cargo build --release
```

Manual smoke test:
1. Run `cargo run`
2. Right-click tray → Configure
3. Dialog opens (non-modal — pet still animates behind it)
4. Click "arrows" in gallery → pet changes to arrows sprite immediately
5. Change scale/speed → edit a field, press Tab to move focus → pet updates live
6. Click "+ Add pet" → second pet appears on screen immediately
7. Click "Remove pet" → pet disappears immediately
8. Close dialog with × → all changes persist after restart (verify config.toml updated)
9. Re-open Configure → dialog comes to front instead of opening a second window
