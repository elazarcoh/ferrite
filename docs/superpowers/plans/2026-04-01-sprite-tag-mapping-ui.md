# Sprite Tag Mapping UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the read-only SM Coverage panel in the sprite editor with an inline editable ComboBox per state row, letting users map SM state names to spritesheet tag names without editing JSON by hand.

**Architecture:** Single-file UI change in `src/tray/sprite_editor.rs` — the read-only rows (lines 434–477) are replaced with editable rows backed by `SpriteEditorViewport.sm_mappings`. A pure `update_tag_mapping()` helper encapsulates the mutation logic and is unit-tested inline. Docs in two files are updated to explain the feature.

**Tech Stack:** Rust, egui (`ComboBox`, `selectable_value`, `Frame`, `help_icon`), `egui_kittest` for smoke test.

---

## File Map

| File | Action | What changes |
|------|--------|-------------|
| `src/tray/sprite_editor.rs` | Modify lines 434–477, add fn, add tests | Editable rows, help icon, legend, `update_tag_mapping()` helper, inline unit tests |
| `assets/README.md` | Modify | Add section on custom tag names and how to map them |
| `crates/ferrite-web/guides/custom-sprites.md` | Modify | Expand with sprite editor walkthrough + tag mapping section |
| `tests/e2e/test_ui_kittest.rs` | Modify | Add smoke test for coverage panel |

---

## Task 0 — Update documentation

**Files:**
- Modify: `assets/README.md`
- Modify: `crates/ferrite-web/guides/custom-sprites.md`

- [ ] **Step 1: Expand `assets/README.md`**

After the existing tag name list, append:

```markdown
## Custom Tag Names

If your sprite uses different tag names (e.g. `walk_right` instead of `walk`), you can
map them in the Sprite Editor's **SM Coverage** panel without editing the JSON by hand.

1. Open the tray menu → **Open…** → **Sprites** tab → select your sprite
2. In the left panel, select the state machine you're using from the **SM:** dropdown
3. The **SM Coverage** panel shows each SM state and its current animation tag
4. For any state with a `✗` (missing) or the wrong tag, click the dropdown on that row
   and select the tag from your spritesheet
5. Click **Save** to write the mapping to the spritesheet JSON

**Example:** Sheet has `walk_right`, SM expects `walk` → select `walk_right` in the
`walk` row's dropdown.
```

- [ ] **Step 2: Expand `crates/ferrite-web/guides/custom-sprites.md`**

Replace the file contents with:

```markdown
# Custom Sprites

Ferrite supports Aseprite-exported spritesheets. Export your animation as a spritesheet
with JSON metadata and import it via the tray menu.

## Exporting from Aseprite

1. File → Export Sprite Sheet
2. Layout: Pack or By Rows
3. Output: check **JSON Data**, format **Hash**
4. Save as `yourpet.png` — the JSON is saved automatically as `yourpet.json`

## Importing into Ferrite

Open the tray icon → **Open…** → **Sprites** tab → **Import…**. The spritesheet appears
in the gallery and can be assigned to a pet from the **Config** tab.

## Tag Names

The SM Coverage panel (left panel in the Sprite Editor) shows which SM states are
matched to animation tags in your spritesheet. States can match tags in two ways:

- **Auto-match** (✓ auto) — the state name and tag name are identical
- **Explicit mapping** (✓ with left bar) — you set an override in the Coverage panel

If a state shows **✗** (red, required) or **○** (yellow, optional), no tag is mapped
to it. To fix this:

1. Select the SM from the **SM:** dropdown in the left panel
2. In the **SM Coverage** section, find the unresolved state row
3. Click the dropdown on that row and pick the tag from your spritesheet
4. Click **Save**

The mapping is stored in the spritesheet JSON (`meta.smMappings`) alongside your
animation data.
```

- [ ] **Step 3: Commit**

```bash
git add assets/README.md crates/ferrite-web/guides/custom-sprites.md
git commit -m "docs: explain custom tag names and tag mapping UI"
```

---

## Task 1 — Add `update_tag_mapping()` helper and failing unit tests

**Files:**
- Modify: `src/tray/sprite_editor.rs`

The mutation logic is extracted into a pure helper so it can be tested without rendering egui.

- [ ] **Step 1: Add `update_tag_mapping` to `sprite_editor.rs`**

Add this function at the bottom of the file, replacing the TODO comment at line 772:

```rust
/// Sets or clears an explicit tag mapping for one SM state.
///
/// - `tag = Some("walk_right")` → inserts `sm_mappings[sm_name][state_name] = "walk_right"`
/// - `tag = None`               → removes the entry (reverts to auto-match)
pub(crate) fn update_tag_mapping(
    sm_mappings: &mut std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    sm_name: &str,
    state_name: &str,
    tag: Option<&str>,
) {
    match tag {
        Some(t) => {
            sm_mappings
                .entry(sm_name.to_string())
                .or_default()
                .insert(state_name.to_string(), t.to_string());
        }
        None => {
            if let Some(m) = sm_mappings.get_mut(sm_name) {
                m.remove(state_name);
            }
        }
    }
}
```

- [ ] **Step 2: Add failing unit tests below the function**

```rust
#[cfg(test)]
mod tests {
    use super::update_tag_mapping;
    use std::collections::HashMap;

    #[test]
    fn tag_mapping_set_explicit_override() {
        let mut mappings: HashMap<String, HashMap<String, String>> = HashMap::new();
        update_tag_mapping(&mut mappings, "default", "walk", Some("walk_right"));
        assert_eq!(mappings["default"]["walk"], "walk_right");
    }

    #[test]
    fn tag_mapping_clear_override_reverts_to_auto() {
        let mut mappings: HashMap<String, HashMap<String, String>> = HashMap::new();
        update_tag_mapping(&mut mappings, "default", "walk", Some("walk_right"));
        update_tag_mapping(&mut mappings, "default", "walk", None);
        assert_eq!(mappings["default"].get("walk"), None);
    }
}
```

- [ ] **Step 3: Run tests — expect FAIL (function not yet called, but should compile and pass)**

```bash
cargo test -p ferrite tag_mapping
```

Expected: both tests **pass** (the function is pure logic, no UI needed — they should pass immediately after Step 1).

- [ ] **Step 4: Commit**

```bash
git add src/tray/sprite_editor.rs
git commit -m "feat(sprite-editor): add update_tag_mapping helper with unit tests"
```

---

## Task 2 — Replace read-only coverage rows with editable ComboBox rows

**Files:**
- Modify: `src/tray/sprite_editor.rs` lines 433–477

- [ ] **Step 1: Replace the SM Coverage panel block**

Find the block starting at line 433:
```rust
// SM coverage panel — shown when an SM is selected
if let Some(sm_name) = s.selected_sm_name.clone()
    && let Some(sm) = gallery.get(&sm_name) {
        ui.separator();
        ui.label(egui::RichText::new("SM Coverage").strong());
        ...
        for (state_name, state_def) in &sm.states {
            ...
            ui.horizontal(|ui| {
                ui.colored_label(color, icon);
                ui.label(label);
            });
        }
    }
```

Replace it entirely with:

```rust
// SM coverage panel — shown when an SM is selected
if let Some(sm_name) = s.selected_sm_name.clone()
    && let Some(sm) = gallery.get(&sm_name) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SM Coverage").strong());
            crate::tray::ui_theme::help_icon(
                ui,
                "States are matched to spritesheet tags by name. If your tags have \
                 different names, use the dropdown on each row to map them explicitly. \
                 '(auto)' means the state name and tag name match — no override needed.",
            );
        });

        let mut sorted_tags: Vec<String> =
            s.state.tags.iter().map(|t| t.name.clone()).collect();
        sorted_tags.sort();

        // Stable display order
        let mut state_entries: Vec<(&String, &crate::sprite::sm_compiler::CompiledState)> =
            sm.states.iter().collect();
        state_entries.sort_by_key(|(n, _)| n.as_str());

        let mut mapping_change: Option<(String, Option<String>)> = None;

        for (state_name, state_def) in &state_entries {
            let explicit = s.sm_mappings
                .get(&sm_name)
                .and_then(|m| m.get(state_name.as_str()))
                .cloned();
            let auto_matches = sorted_tags.iter().any(|t| t == *state_name);
            let resolved = explicit.as_deref().or_else(|| {
                if auto_matches { Some(state_name.as_str()) } else { None }
            });

            let (icon, color) = match resolved {
                Some(_) => ("✓", egui::Color32::LIGHT_GREEN),
                None if state_def.required => ("✗", egui::Color32::LIGHT_RED),
                None => ("○", egui::Color32::LIGHT_YELLOW),
            };

            let has_explicit = explicit.is_some();
            let mut selected = explicit.clone().unwrap_or_else(|| "(auto)".to_string());
            let old_selected = selected.clone();

            let stroke = if has_explicit {
                egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 160, 80))
            } else {
                egui::Stroke::NONE
            };

            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(4.0, 1.0))
                .stroke(stroke)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(color, icon);
                        let suffix = if resolved.is_none() && state_def.required {
                            " required"
                        } else if resolved.is_none() {
                            " optional"
                        } else {
                            ""
                        };
                        ui.label(format!("{}{}", state_name, suffix));

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                let cb = egui::ComboBox::from_id_salt((
                                    "tag_map",
                                    sm_name.as_str(),
                                    state_name.as_str(),
                                ))
                                .selected_text(selected.clone())
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut selected,
                                        "(auto)".to_string(),
                                        "(auto)",
                                    );
                                    for tag in &sorted_tags {
                                        ui.selectable_value(
                                            &mut selected,
                                            tag.clone(),
                                            tag.as_str(),
                                        );
                                    }
                                });
                                if selected == "(auto)" {
                                    cb.response.on_hover_text(
                                        "No explicit mapping. Uses the tag named identically \
                                         to this state. Select a tag to override.",
                                    );
                                }
                            },
                        );
                    });
                });

            if selected != old_selected {
                mapping_change = Some((
                    state_name.to_string(),
                    if selected == "(auto)" { None } else { Some(selected) },
                ));
            }
        }

        // Apply any mapping change outside the borrow on sm.states
        if let Some((state_name, tag)) = mapping_change {
            update_tag_mapping(
                &mut s.sm_mappings,
                &sm_name,
                &state_name,
                tag.as_deref(),
            );
            s.dirty = true;
        }

        // Legend
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::LIGHT_GREEN, "✓");
            ui.label("resolved");
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_RED, "✗");
            ui.label("required missing");
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_YELLOW, "○");
            ui.label("optional missing");
            ui.separator();
            ui.label(egui::RichText::new("│ = explicit override").weak().small());
        });
    }
```

- [ ] **Step 2: Verify the import path for `CompiledState`**

Check what's already imported at the top of `sprite_editor.rs`. If `CompiledState` isn't imported, check the correct path:

```bash
grep -n "CompiledState\|use crate::sprite" src/tray/sprite_editor.rs | head -10
```

If not imported, add to imports:
```rust
use crate::sprite::sm_compiler::CompiledState;
```
And replace `crate::sprite::sm_compiler::CompiledState` in the Vec type with just `CompiledState`.

- [ ] **Step 3: Build to check for compile errors**

```bash
cargo build -p ferrite 2>&1 | grep "^error"
```

Expected: no errors. Fix any type/import errors that appear.

- [ ] **Step 4: Run unit tests — both must still pass**

```bash
cargo test -p ferrite tag_mapping
```

Expected: `tag_mapping_set_explicit_override ... ok`, `tag_mapping_clear_override_reverts_to_auto ... ok`

- [ ] **Step 5: Commit**

```bash
git add src/tray/sprite_editor.rs
git commit -m "feat(sprite-editor): inline tag ComboBox in SM Coverage panel"
```

---

## Task 3 — Add kittest smoke test for the coverage panel

**Files:**
- Modify: `tests/e2e/test_ui_kittest.rs`

- [ ] **Step 1: Write the failing smoke test**

Add to `tests/e2e/test_ui_kittest.rs`:

```rust
#[test]
fn sm_coverage_panel_renders_with_editable_rows() {
    use egui_kittest::kittest::Queryable;
    use ferrite::{
        sprite::editor_state::{EditorTag, SpriteEditorState},
        tray::sprite_editor::{render_sprite_editor_panel, SpriteEditorViewport},
    };
    use image::RgbaImage;
    use std::path::PathBuf;

    // Build a minimal SpriteEditorState with one tag named "idle"
    let mut state = SpriteEditorState::new(
        PathBuf::from("test.png"),
        RgbaImage::new(16, 16),
    );
    state.tags.push(EditorTag {
        name: "idle".to_string(),
        from: 0,
        to: 0,
        direction: ferrite::sprite::sheet::TagDirection::Forward,
        flip_h: false,
        color: 0,
    });

    let mut viewport = SpriteEditorViewport::new(state);

    // The panel must render without panicking — basic smoke test
    let mut harness = egui_kittest::Harness::new(move |ctx| {
        render_sprite_editor_panel(ctx, &mut viewport);
    });
    harness.run();
    // Verify the SM selector label is present (panel rendered)
    assert!(harness.get_by_label("SM:").is_ok());
}
```

- [ ] **Step 2: Run — expect compile error or test failure, diagnose**

```bash
cargo test --test e2e sm_coverage_panel_renders
```

If `SpriteEditorViewport` or `EditorTag` are not `pub` from the test's perspective, adjust visibility or imports. `TagDirection` import path may also need adjustment — check with:

```bash
grep -rn "pub enum TagDirection\|pub use.*TagDirection" crates/ferrite-core/src/ src/
```

Fix any visibility/import issues.

- [ ] **Step 3: Run — expect pass**

```bash
cargo test --test e2e sm_coverage_panel_renders
```

Expected: `sm_coverage_panel_renders_with_editable_rows ... ok`

- [ ] **Step 4: Run full e2e suite — no regressions**

```bash
cargo test --test e2e 2>&1 | tail -8
```

Expected: same results as before this task (1 pre-existing `drag_sends_drag_start_and_end_events` failure, all others pass).

- [ ] **Step 5: Commit**

```bash
git add tests/e2e/test_ui_kittest.rs
git commit -m "test(sprite-editor): smoke test for SM Coverage panel rendering"
```

---

## Task 4 — Final verification and cleanup

**Files:** none new

- [ ] **Step 1: Run the full integration suite**

```bash
cargo test --test integration 2>&1 | tail -5
```

Expected: 73 passed, 0 failed (or same as baseline — 2 stress failures are pre-existing timing issues).

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -p ferrite -- -D warnings -A dead-code 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 3: Confirm the spec TODO is removed**

```bash
grep -n "Task-13" src/tray/sprite_editor.rs
```

Expected: no output (the TODO comment at line 772 was replaced by `update_tag_mapping` in Task 1).

- [ ] **Step 4: Verify all spec requirements are met**

```bash
# help_icon present in coverage panel
grep -n "help_icon" src/tray/sprite_editor.rs

# ComboBox present in coverage panel
grep -n "tag_map.*ComboBox\|ComboBox.*tag_map" src/tray/sprite_editor.rs

# dirty flag set in coverage panel
grep -n "dirty = true" src/tray/sprite_editor.rs

# Legend present
grep -n "explicit override" src/tray/sprite_editor.rs
```

Expected: one match each.

- [ ] **Step 5: Final commit if any small fixes were needed**

```bash
git add -p   # stage only what changed
git commit -m "fix(sprite-editor): <description of fix>"
```

---

## Verification commands (run after all tasks)

```bash
cargo test -p ferrite tag_mapping           # 2 unit tests
cargo test --test e2e sm_coverage           # smoke test
cargo test --test e2e                       # full e2e (1 pre-existing failure ok)
cargo test --test integration               # no regressions
cargo clippy -p ferrite -- -D warnings -A dead-code
```
