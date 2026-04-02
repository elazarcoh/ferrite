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
- A legend at the bottom of the panel explains: `✓ resolved · ✗ required missing · ○ optional missing · left bar = explicit override`

### Discoverability and help text

Tag mapping is a non-obvious feature. The spec requires these affordances:

1. **Section help icon** — place `ui_theme::help_icon()` next to the "SM Coverage" heading with
   text: *"States are matched to spritesheet tags by name. If your tags have different names, use
   the dropdown on each row to map them explicitly. '(auto)' means the state name and tag name
   match — no override needed."*

2. **Clickable row affordance** — rows show a subtle highlight on hover so users discover
   they are interactive (standard `egui` hover response is sufficient; no extra code needed).

3. **`(auto)` tooltip** — when `(auto)` is the selected value in the ComboBox, display a
   tooltip on hover: *"No explicit mapping. Uses the tag named identically to this state. Select
   a tag to override."*

4. **Unsaved-changes indicator** — after any mapping change the existing "● unsaved changes"
   indicator (already present in the sprite editor) must activate, so users know to hit Save.

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
| `src/tray/sprite_editor.rs` | Replace read-only coverage rows (lines 434–477) with `egui::ComboBox` rows + help icon + legend; remove TODO(Task-13) comment at line 772 |
| `tests/e2e/test_ui_kittest.rs` | Add 2 new UI tests |
| `crates/ferrite-web/guides/custom-sprites.md` | Expand with sprite editor walkthrough and tag mapping section (see below) |
| `assets/README.md` | Add note on non-standard tag names and how to map them |

### Doc updates required

**`crates/ferrite-web/guides/custom-sprites.md`** must cover:
- How to open the Sprite Editor (tray → Open → Sprites tab)
- What the SM Coverage panel shows and what ✓/✗/○ mean
- How to use the tag dropdown to fix unresolved states (✗ or ○)
- That changes must be saved with the Save button to persist

**`assets/README.md`** must add (after the existing tag name list):
- A note that these are the *default* tag names recognised automatically
- That custom tag names can be mapped in the Sprite Editor's SM Coverage panel
- Example: *"If your sprite uses `walk_right` instead of `walk`, open the sprite in the
  Sprite Editor, find the `walk` row in the SM Coverage panel, and select `walk_right`
  from the dropdown."*

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
