# Web: Touch Support + Guides Visual Redesign

## Touch Support

### M-01: Webapp (`crates/ferrite-webapp/index.html`)
Add `touch-action: none` to the `canvas` CSS rule. Viewport meta already present.

### M-02: Website (`crates/ferrite-web/src/components/pet_canvas.rs`)
Register `pointerdown` and `pointermove` listeners with `{ passive: false }`. Call `e.prevent_default()` in `pointermove` while dragging to suppress browser scroll. `pointer events` already handle touch — this is the only missing piece.

---

## Guides Redesign

### W-01: Visual design
- **Guide index** (`guide_index.rs`): replace plain link list with 2-column card grid. Each card: screenshot image (64px tall), title, subtitle.
- **Guide article** (`guide_page.rs`): widen to `max-w-3xl`, add "← Guides" back link, render screenshots full-width with `rounded-lg border border-slate-200`, dark code blocks (`bg-slate-900 text-slate-100 rounded-lg`).
- **Markdown renderer**: replace hand-rolled parser with `pulldown-cmark`. Must handle: headings h1–h3, bold, italic, inline code, fenced code blocks, unordered lists, images `![alt](src)`.

### W-02: Screenshots
- **Capture script**: `tests/webapp/scripts/capture-screenshots.ts` — Playwright script that builds/serves the webapp, waits for `window.__ferrite`, navigates to the right tab for each guide, screenshots, saves PNG to `crates/ferrite-web/assets/guides/<slug>.png`.
- **Subjects**:
  - `getting-started.png` — Simulation tab, pet on floor
  - `custom-sprites.png` — Sprites tab, gallery visible
  - `state-machines.png` — State Machines tab, editor visible
  - `configuration.png` — Config tab, config panel visible
- **Referenced in markdown** as `![alt](/ferrite/assets/guides/<slug>.png)` (Dioxus serves `assets/` under the base path).
- **Static assets directory**: `crates/ferrite-web/assets/guides/` — committed PNG files.

### Card colour scheme (guide index)
Each card has a unique gradient on the screenshot placeholder / image header:

| Guide | Gradient |
|-------|----------|
| getting-started | indigo → purple `#6366f1 → #8b5cf6` |
| custom-sprites | sky → cyan `#0ea5e9 → #06b6d4` |
| state-machines | amber → orange `#f59e0b → #f97316` |
| configuration | emerald → green `#10b981 → #059669` |

The gradient shows as the card background when no screenshot is present; once screenshots are committed it sits behind a semi-transparent overlay.

---

## Files

| File | Change |
|------|--------|
| `crates/ferrite-webapp/index.html` | Add `touch-action: none` to canvas CSS |
| `crates/ferrite-web/src/components/pet_canvas.rs` | Non-passive listeners + `prevent_default` during drag |
| `crates/ferrite-web/Cargo.toml` | Add `pulldown-cmark = "0.12"` |
| `crates/ferrite-web/src/pages/guide_page.rs` | pulldown-cmark renderer, wider layout, back link, screenshot styles |
| `crates/ferrite-web/src/pages/guide_index.rs` | 2-column card grid |
| `crates/ferrite-web/guides/*.md` | Add `![…](…)` image line per guide |
| `crates/ferrite-web/assets/guides/` | New dir — 4 committed PNGs |
| `tests/webapp/scripts/capture-screenshots.ts` | New Playwright capture script |
| `BACKLOG.md` | Mark M-01, M-02, W-01, W-02 complete |
