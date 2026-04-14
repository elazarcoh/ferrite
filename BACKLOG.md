# Ferrite Backlog

Consolidated tracking for all open work items across the project.

---

## Engine Cleanup

Minor refactors deferred from the core-computation-centralization PR.
Files are all in desktop-only code (`src/`); no ferrite-core changes needed.

- [x] **B1 — DragState struct** (`src/window/wndproc.rs`)
  `HwndData` has 8 loose drag-related fields. Consolidate into an inner `DragState` struct.

- [x] **B2 — Deduplicate distance formulas** (`src/app.rs` ~line 691)
  `cursor_dist` and `other_pet_dist` each write out `((dx*dx + dy*dy) as f32).sqrt()` inline. Extract a small helper or closure.

- [x] **B3 — ScaledDimensions helper** (`src/app.rs`)
  `(value as f32 * cfg.scale).round() as i32` appears in multiple places in `PetInstance::tick` and `collect_collidables`. Centralise rounding.

- [x] **B4 — InteractionEvent coordinate frame doc** (`src/event.rs`)
  `PetDragStart` carries screen-space cursor coordinates; the pet-relative offset is computed later in `App::handle_event`. Add a doc comment clarifying the coordinate frame.

---

## Webapp Bugs

Discovered via interactive browser inspection on 2026-04-07.
See `docs/superpowers/plans/2026-04-07-webapp-backlog.md` for full root-cause notes.
Implementation plans: PR-A (`2026-04-07-webapp-pr-a.md`), PR-B/C/D (not yet written).

### Critical

- [ ] **B-01** — Selected item text invisible (tabs, pet list, gallery) — `ui_theme.rs`: `selection.stroke` = same color as `selection.bg_fill`
- [ ] **B-02** — SM code editor ~30px wide at narrow viewports — `sm_editor.rs`: panel min-widths consume all space
- [ ] **B-03** — Sprite editor never loads in webapp — `app_window.rs` / `ferrite-webapp/src/app.rs`: `sprite_editor` never set from `None`
- [ ] **B-04** — State Graph always "No valid SM selected" — `sm_editor.rs`: graph only renders when `selected_sm.is_some()`, "New SM" sets it to `None`
- [ ] **B-05** — New SM doesn't appear in list until saved — `sm_editor.rs`: `list_names()` only sees saved SMs

### Layout / UX

- [ ] **B-06** — Pet starts below the floor (y=800 from desktop config, floor at SIM_FLOOR_Y=500) — `simulation.rs`
- [ ] **B-07** — Floor line appears mid-canvas (SIM_FLOOR_Y hardcoded to 500, not relative to panel height) — `simulation.rs`
- [ ] **B-08** — Pet barely visible at bottom (consequence of B-06) — `simulation.rs`
- [ ] **B-09** — Tab bar clips at ~474px viewport — `app_window.rs`: no overflow handling
- [ ] **B-10** — Import/Export Bundle buttons visually orphaned — `simulation.rs`: bare `TopBottomPanel` with no grouping
- [ ] **B-11** — ✕ close button does nothing in browser — `app_window.rs`: should be `#[cfg(not(target_arch="wasm32"))]`

### Missing / Incomplete Web Adaptation

- [ ] **B-12** — Config not persisted across reloads (`localStorage` may not load on init)
- [ ] **B-13** — `embedded://default` shown raw in SM dropdown — needs friendly display name in `WebSmStorage`
- [ ] **B-14** — X/Y config fields expose raw simulation coords (y=800 meaningless to users)
- [ ] **B-15** — No loading indicator while WASM initialises
- [ ] **B-16** — No favicon
- [ ] **B-17** — "Edit…" pet button does nothing visible in webapp
- [ ] **B-18** — No simulation controls (pause/play, reset, speed)
- [ ] **B-19** — Theme toggle icon (✵/□) not intuitive — may be font rendering
- [ ] **B-20** — No error feedback for invalid SM TOML (no inline highlighting)
- [ ] **B-21** — Toolbar buttons have no tooltips (theme/close area)
- [x] **B-22** — Can't import sprite PNG in webapp — fixed in PR #19
- [x] **B-23** — Simulation tab visible in desktop app — fixed in PR #19
- [x] **B-24** — Desktop gallery click shows "Select a sprite to edit" — fixed in PR #19

---

## Planned PR Groupings (webapp)

| PR | Items | Status |
|----|-------|--------|
| PR-A | B-01, B-02, B-06, B-07, B-08, B-11 | plan written, not started |
| PR-B | B-03, B-04, B-05, B-10, B-13 | plan not written |
| PR-C | B-09, B-12, B-14, B-15, B-16 | plan not written |
| PR-D | B-17, B-19, B-20, B-21 | plan not written |
| — | B-18 (simulation controls) | unplanned |
