# SM Import & Per-Pet SM Switching — Design Spec

**Date:** 2026-04-01
**Status:** Approved

---

## Problem

Ferrite has a state machine (SM) gallery and a per-pet SM selector in the config UI, but
the plumbing between them is incomplete:

1. Changing a pet's SM via the dropdown triggers `ConfigChanged` → `apply_config()` rebuilds
   the entire `PetInstance` (window destroyed and recreated — visible flicker, position lost).
2. Three `AppEvent` variants (`SMImported`, `SMChanged`, `SMCollectionChanged`) are unhandled.
3. `BundleImported` is logged-only — the SM from the bundle is not auto-assigned to the pet,
   and open UI windows don't refresh their SM dropdowns after an import.

---

## Goals

- SM switching is instant and non-destructive: position and window survive, only the
  state-chart is replaced.
- Bundle imports auto-assign the bundled SM to the pet whose sprite was just imported.
- Open config windows refresh their SM dropdown when the gallery changes.
- All stub event arms are wired up.

---

## Design

### Part A — `SMRunner::replace_sm()`

Add a method to `SMRunner` (`crates/ferrite-core/src/sprite/sm_runner.rs`):

```rust
pub fn replace_sm(&mut self, new_sm: Arc<CompiledSM>)
```

Resets: `active` (→ new SM's `default_fallback`), `previous_named`, `state_time_ms`,
`step_index`, `walk_remaining_px`, `next_transition_ms`, `force_state`, `release_force`,
`step_mode`, `step_advance`, `transition_log`, `current_tag`.

Preserves: `facing`, `walk_speed`, `rng`, `last_vars`.

Position (`x`, `y`) lives on `PetInstance`, not `SMRunner` — untouched automatically.

### Part B — Surgical diff in `apply_config()`

In `src/app.rs`, `apply_config()` currently rebuilds all pets. Add a fast path: when
only `state_machine` differs (sheet, scale, and walk_speed are unchanged), call
`pet.runner.replace_sm()` and `continue` instead of full rebuild.

A `resolve_sm(name, gallery) -> Arc<CompiledSM>` helper encapsulates the
`"embedded://default"` vs gallery lookup logic (with fallback + warning on unknown name).

### Part C — Bundle import auto-assign

Handle `AppEvent::BundleImported { sprite_id, sm_name: Some(name) }`:
1. Find the pet whose `cfg.sheet_path` contains `sprite_id`.
2. Look up the compiled SM from `SmGallery::get(name)`.
3. Call `pet.runner.replace_sm(sm)` and update `pet.cfg.state_machine`.
4. Persist config to disk.
5. Fire `notify_sm_collection_changed()` to refresh open windows.

If no pet uses that sprite, skip 2–4 and still fire the notification.

### Part D — SM gallery dirty flag

`AppWindowState` (`src/tray/app_window.rs`) gains `pub sm_gallery_dirty: bool`.

`notify_sm_collection_changed()` on `App` sets this flag if an app window is open.

The config tab render in `src/tray/config_window.rs` clears the flag at the start of
its SM section. Since `SmGallery::load()` is already called inline each render, the
next frame naturally picks up any new SMs.

### Part E — Remaining stub events

`SMImported { name }` → log + `notify_sm_collection_changed()`.

`SMChanged { pet_id, sm_name }` → direct hot-swap via `replace_sm()` + persist config.

`SMCollectionChanged` → `notify_sm_collection_changed()`.

---

## Testing

| Test | Location | What it verifies |
|------|----------|-----------------|
| `replace_sm_resets_state` | inline unit test in `sm_runner.rs` | All state fields reset, new default active |
| `replace_sm_preserves_facing` | inline unit test | `facing` survives swap |
| `replace_sm_preserves_walk_speed` | inline unit test | `walk_speed` survives swap |
| `sm_hot_swap_preserves_position` | `tests/integration/test_sm_switching.rs` | x/y unchanged after config SM change |
| `sm_hot_swap_does_not_rebuild_window` | same file, `#[cfg(windows)]` | HWND is same before and after |
| `bundle_import_auto_assigns_sm` | same file | BundleImported → pet SM updated, default state active |

---

## Files Changed

| File | Change |
|------|--------|
| `crates/ferrite-core/src/sprite/sm_runner.rs` | Add `replace_sm()` + unit tests |
| `src/app.rs` | Hot-swap path, event handlers, `resolve_sm()`, `notify_sm_collection_changed()` |
| `src/tray/app_window.rs` | `sm_gallery_dirty: bool` field |
| `src/tray/config_window.rs` | Clear dirty flag in SM section render |
| `tests/integration/test_sm_switching.rs` | New integration tests |
