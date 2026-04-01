# Sprite Tag Mapping UI — Design Spec

**Date:** 2026-04-01
**Status:** Approved

---

## Problem

Custom sprite sheets often use Aseprite tag names that don't match SM state names
(e.g., the sheet has `walk_right` but the SM state is `walk`). The SM compiler's
fallback chain handles these mismatches at runtime, but there was previously no UI
to create explicit mappings. The old tag-map panel in `sprite_editor.rs` was removed
(TODO at line 772) and never replaced.

Without the UI, custom sheets with non-standard tag names have unresolved states in
the SM Coverage panel with no way to fix them without manually editing the spritesheet
JSON.

---

## Goal

Make every row in the SM Coverage panel editable with an inline tag dropdown. Users
can explicitly map any SM state to any tag from the current spritesheet, or clear an
override to revert to automatic name-matching.

---

## Design

### Interaction model: inline dropdown (Option A)

Every state row in the SM Coverage panel becomes interactive. Clicking a row activates
an `egui::ComboBox` showing all tags available in the loaded spritesheet.

**Row anatomy:**
```
[✓/✗/○]  [state name]  [label: required/optional]  [ComboBox ▾]
```

**Dropdown contents (in order):**
1. `(auto)` — always first; selecting this clears any explicit override
2. All tag names from the loaded `SpriteSheet.tags`, sorted alphabetically

**Visual differentiation:**
- Rows with an explicit override set show a left-side colour accent
- Rows using auto-match show the ComboBox with muted text (`(auto)`)
- Status icon (✓/✗/○) recomputes live after every selection change

### State transitions on selection

| Selection | Effect on `sm_mappings` | Resulting status |
|-----------|------------------------|-----------------|
| Tag name `T` | `sm_mappings[sm][state] = T` | ✓ (explicit) |
| `(auto)` | `sm_mappings[sm].remove(state)` | ✓ auto (if name matches) or ✗/○ |

**Reverts to auto:** Clearing an override falls through to `SpriteSheet::resolve_tag()`'s
auto-match rule — if a tag with the exact state name exists, the state resolves
automatically; otherwise it becomes unresolved.

### Persistence

Changes are written to `SpriteEditorViewport.sm_mappings` immediately on selection.
They are persisted to the spritesheet JSON (`meta.smMappings`) only when the user
clicks the existing "Save" button — no new save path is introduced.

### SM scope

Mappings are per-SM. The existing SM selector dropdown at the top of the panel
determines which SM's mappings are shown and edited.

---

## Architecture

**This is a single-file change.** All required data is already present:

| Data | Location |
|------|----------|
| In-memory mappings | `SpriteEditorViewport.sm_mappings: HashMap<String, HashMap<String, String>>` |
| Active SM | `SpriteEditorViewport.selected_sm_name: Option<String>` |
| Available tags | `SpriteEditorState.state.tags: Vec<FrameTag>` |
| Persistence | `save()` at line 171 already syncs `sm_mappings` → `SpriteEditorState` → JSON |
| Resolution logic | `SpriteSheet::resolve_tag()` in `crates/ferrite-core/src/sprite/sheet.rs` |

No changes to `ferrite-core`, `PetConfig`, or any other module.

---

## File Changed

| File | Change |
|------|--------|
| `src/tray/sprite_editor.rs` | Replace read-only coverage rows (lines 434–477) with `egui::ComboBox` rows; remove TODO(Task-13) comment at line 772 |
| `tests/e2e/test_ui_kittest.rs` | Add 2 new UI tests |

---

## Testing

Both tests go in `tests/e2e/test_ui_kittest.rs` using `egui_kittest`:

1. **`tag_mapping_set_explicit_override`** — Render the SM Coverage panel with a minimal
   loaded `SpriteEditorViewport` (sprite with at least one tag, SM with at least one state
   whose name doesn't match). Interact with the ComboBox to select a tag. Assert:
   `viewport.sm_mappings[sm_name][state_name] == tag_name`

2. **`tag_mapping_clear_override_reverts_to_auto`** — Pre-populate an explicit mapping.
   Render the panel, select `(auto)` from the dropdown. Assert:
   `viewport.sm_mappings[sm_name].get(state_name) == None`

No changes needed to `ferrite-core` tests — `SpriteSheet::resolve_tag()` is already
covered by inline unit tests.
