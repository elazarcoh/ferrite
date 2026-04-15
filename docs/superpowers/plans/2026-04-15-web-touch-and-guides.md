# Web Touch Support + Guides Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add mobile touch support to the webapp and website, and redesign the website guides with a card-grid index, proper Markdown rendering via pulldown-cmark, and Playwright-captured screenshots.

**Architecture:** Six independent tasks: two small touch fixes (webapp index.html, website pet_canvas.rs), then guides in four steps (markdown engine, card grid, screenshot capture script, asset commit + markdown updates). Each task ends with a green build and a commit.

**Tech Stack:** Rust, Dioxus 0.7, Tailwind CSS v3, pulldown-cmark 0.12, web-sys, Playwright/TypeScript.

---

## File Map

| File | Change |
|------|--------|
| `crates/ferrite-webapp/index.html` | Add `touch-action: none` to canvas CSS |
| `crates/ferrite-web/src/components/pet_canvas.rs` | Non-passive pointermove listener + `prevent_default` during drag |
| `crates/ferrite-web/Cargo.toml` | Add `pulldown-cmark`, add web-sys features |
| `crates/ferrite-web/src/pages/guide_page.rs` | Replace hand-rolled parser with pulldown-cmark; widen layout; back link; screenshot image styles |
| `crates/ferrite-web/src/pages/guide_index.rs` | 2-column card grid with gradient + screenshot image per guide |
| `crates/ferrite-web/guides/getting-started.md` | Add `![…](…)` image line |
| `crates/ferrite-web/guides/custom-sprites.md` | Add `![…](…)` image line |
| `crates/ferrite-web/guides/state-machines.md` | Add `![…](…)` image line |
| `crates/ferrite-web/guides/configuration.md` | Add `![…](…)` image line |
| `crates/ferrite-web/assets/guides/` | New directory — 4 committed PNG screenshots |
| `tests/webapp/scripts/capture-screenshots.ts` | New Playwright capture script |
| `BACKLOG.md` | Mark M-01, M-02, W-01, W-02 complete |

---

## Task 1: M-01 — Webapp touch-action

**Files:**
- Modify: `crates/ferrite-webapp/index.html`

- [ ] **Step 1: Add `touch-action: none` to the canvas rule**

In `crates/ferrite-webapp/index.html`, find:
```css
canvas { width: 100%; height: 100%; }
```
Replace with:
```css
canvas { width: 100%; height: 100%; touch-action: none; }
```

- [ ] **Step 2: Verify build**

```bash
cargo check -p ferrite-webapp --target wasm32-unknown-unknown
```

Expected: clean compile.

- [ ] **Step 3: Commit**

```bash
git add crates/ferrite-webapp/index.html
git commit -m "fix(webapp): add touch-action:none to canvas for mobile touch support"
```

---

## Task 2: M-02 — Website non-passive pointer listeners

**Files:**
- Modify: `crates/ferrite-web/Cargo.toml`
- Modify: `crates/ferrite-web/src/components/pet_canvas.rs`

**Context:** The pet canvas listens for `pointermove` at document level to implement drag. On mobile, the browser defaults to passive listeners and scrolls the page during the drag gesture. Fix: register `pointermove` (and `pointerdown` for consistency) as non-passive, and call `e.prevent_default()` inside `pointermove` when dragging.

- [ ] **Step 1: Add web-sys features**

In `crates/ferrite-web/Cargo.toml`, find the `web-sys` dependency and add `"EventTarget"` and `"AddEventListenerOptions"` to its features list:

```toml
web-sys = { version = "0.3", features = [
    "Window", "Document", "HtmlCanvasElement", "HtmlImageElement",
    "CanvasRenderingContext2d", "Performance",
    "EventTarget", "AddEventListenerOptions",
] }
```

- [ ] **Step 2: Update `setup_drag` in `pet_canvas.rs`**

Replace the entire `setup_drag` function with the following. The only functional changes are: (a) `pointermove` is registered with `{ passive: false }`, and (b) `e.prevent_default()` is called when dragging.

```rust
fn setup_drag(doc: &web_sys::Document, state: Rc<RefCell<PetWebState>>) {
    // pointerdown — hit-test pet bounding box and start drag
    {
        let state = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
            let (mx, my) = (e.client_x(), e.client_y());
            let s = state.borrow();
            let pet_w = s.sheet.frames.first().map(|f| (f.w as f64 * 2.0) as i32).unwrap_or(64);
            let pet_h = s.sheet.frames.first().map(|f| (f.h as f64 * 2.0) as i32).unwrap_or(80);
            let hit = mx >= s.x && mx <= s.x + pet_w && my >= s.y && my <= s.y + pet_h;
            if hit {
                drop(s);
                let mut s = state.borrow_mut();
                let offset = (mx - s.x, my - s.y);
                s.runner.grab(offset);
                s.is_dragging = true;
                s.drag_offset = offset;
                let now = web_sys::window().unwrap().performance().unwrap().now();
                s.vel_prev = None;
                s.vel_cur = Some(((mx, my), now));
            }
        });
        doc.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref()).unwrap();
        cb.forget();
    }

    // pointermove — NON-PASSIVE so prevent_default() can suppress browser scroll during drag
    {
        let state = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
            let mut s = state.borrow_mut();
            if s.is_dragging {
                e.prevent_default(); // suppress touch scroll while dragging pet
                let (mx, my) = (e.client_x(), e.client_y());
                s.x = mx - s.drag_offset.0;
                s.y = my - s.drag_offset.1;
                let now = web_sys::window().unwrap().performance().unwrap().now();
                s.vel_prev = s.vel_cur.take();
                s.vel_cur = Some(((mx, my), now));
            }
        });
        let options = web_sys::AddEventListenerOptions::new();
        options.set_passive(false);
        doc.add_event_listener_with_callback_and_add_event_listener_options(
            "pointermove",
            cb.as_ref().unchecked_ref(),
            &options,
        ).unwrap();
        cb.forget();
    }

    // pointerup — compute velocity and release into physics
    {
        let state = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |_e: web_sys::PointerEvent| {
            let mut s = state.borrow_mut();
            if s.is_dragging {
                let velocity = match (&s.vel_prev, &s.vel_cur) {
                    (Some(((x0, y0), t0)), Some(((x1, y1), t1))) => {
                        let dt = ((t1 - t0) / 1000.0).max(0.001) as f32;
                        ((x1 - x0) as f32 / dt, (y1 - y0) as f32 / dt)
                    }
                    _ => (0.0, 0.0),
                };
                s.runner.release(velocity);
                s.is_dragging = false;
                s.vel_prev = None;
                s.vel_cur = None;
            }
        });
        doc.add_event_listener_with_callback("pointerup", cb.as_ref().unchecked_ref()).unwrap();
        cb.forget();
    }
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p ferrite-web
```

Expected: clean compile, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/ferrite-web/Cargo.toml crates/ferrite-web/src/components/pet_canvas.rs
git commit -m "fix(web): non-passive pointermove listener prevents scroll during pet drag on mobile"
```

---

## Task 3: Guide pages — pulldown-cmark renderer

**Files:**
- Modify: `crates/ferrite-web/Cargo.toml`
- Modify: `crates/ferrite-web/src/pages/guide_page.rs`

- [ ] **Step 1: Add pulldown-cmark dependency**

In `crates/ferrite-web/Cargo.toml`, add to `[dependencies]`:

```toml
pulldown-cmark = { version = "0.12", default-features = false }
```

- [ ] **Step 2: Rewrite `guide_page.rs`**

Replace the entire file content:

```rust
use dioxus::prelude::*;
use crate::app::Route;

const GETTING_STARTED: &str = include_str!("../../guides/getting-started.md");
const CUSTOM_SPRITES: &str  = include_str!("../../guides/custom-sprites.md");
const STATE_MACHINES: &str  = include_str!("../../guides/state-machines.md");
const CONFIGURATION: &str   = include_str!("../../guides/configuration.md");

fn markdown_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, opts);
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

#[component]
pub fn GuidePage(slug: String) -> Element {
    let content = match slug.as_str() {
        "getting-started" => GETTING_STARTED,
        "custom-sprites"  => CUSTOM_SPRITES,
        "state-machines"  => STATE_MACHINES,
        "configuration"   => CONFIGURATION,
        _ => "# 404\nGuide not found.",
    };

    rsx! {
        article { class: "max-w-3xl mx-auto px-6 py-12",
            Link {
                to: Route::GuideIndex {},
                class: "inline-flex items-center gap-1 text-indigo-600 text-sm font-semibold mb-10 hover:text-indigo-800 transition-colors",
                "← Guides"
            }
            div {
                class: "prose prose-slate prose-headings:font-bold prose-a:text-indigo-600 prose-code:bg-slate-100 prose-code:rounded prose-code:px-1 prose-code:text-sm prose-pre:bg-slate-900 prose-pre:text-slate-100 prose-pre:rounded-xl max-w-none [&_img]:rounded-xl [&_img]:border [&_img]:border-slate-200 [&_img]:my-6 [&_img]:w-full [&_img]:shadow-sm",
                dangerous_inner_html: markdown_to_html(content)
            }
        }
    }
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p ferrite-web
```

Expected: clean compile.

- [ ] **Step 4: Verify in browser**

The `dx serve` process at http://localhost:8081/ferrite/guides/custom-sprites should now render bold text, numbered lists, code blocks, and inline code from the guide markdown.

- [ ] **Step 5: Commit**

```bash
git add crates/ferrite-web/Cargo.toml crates/ferrite-web/src/pages/guide_page.rs
git commit -m "feat(web): replace hand-rolled markdown parser with pulldown-cmark; widen guide layout"
```

---

## Task 4: Guide index — card grid

**Files:**
- Modify: `crates/ferrite-web/src/pages/guide_index.rs`

- [ ] **Step 1: Rewrite `guide_index.rs`**

Replace the entire file content:

```rust
use dioxus::prelude::*;
use crate::app::Route;

#[component]
pub fn GuideIndex() -> Element {
    // (slug, title, subtitle, gradient-start, gradient-end)
    let guides = [
        ("getting-started", "Getting Started",  "Install and run your first pet",   "#6366f1", "#8b5cf6"),
        ("custom-sprites",  "Custom Sprites",   "Import your own artwork",           "#0ea5e9", "#06b6d4"),
        ("state-machines",  "State Machines",   "Animate pet behaviour",             "#f59e0b", "#f97316"),
        ("configuration",   "Configuration",    "Tweak speed, scale and more",       "#10b981", "#059669"),
    ];

    rsx! {
        div { class: "max-w-3xl mx-auto px-6 py-16",
            p { class: "text-sm font-semibold text-indigo-600 tracking-widest uppercase mb-2",
                "Ferrite"
            }
            h1 { class: "text-4xl font-extrabold text-slate-900 mb-3", "Guides" }
            p { class: "text-slate-500 text-lg mb-10",
                "Everything you need to know about Ferrite"
            }
            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-6",
                for (slug, title, subtitle, c1, c2) in guides {
                    Link {
                        to: Route::GuidePage { slug: slug.to_string() },
                        class: "group block rounded-2xl border border-slate-200 overflow-hidden hover:shadow-xl hover:-translate-y-0.5 transition-all duration-150",
                        // Screenshot image with gradient fallback — CSS background-image
                        // tries the PNG first; falls back to the gradient if file is absent.
                        div {
                            style: format!(
                                "height:112px; background-image: url('/ferrite/assets/guides/{slug}.png'), linear-gradient(135deg, {c1}, {c2}); background-size: cover; background-position: center top;"
                            ),
                        }
                        div { class: "p-5",
                            h2 { class: "font-bold text-slate-900 group-hover:text-indigo-700 transition-colors text-lg",
                                "{title}"
                            }
                            p { class: "text-sm text-slate-500 mt-1", "{subtitle}" }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p ferrite-web
```

Expected: clean compile.

- [ ] **Step 3: Verify in browser**

Navigate to http://localhost:8081/ferrite/guides — should show a 2-column card grid. Before screenshots exist the gradient fills the card image area. Clicking a card opens the guide article.

- [ ] **Step 4: Commit**

```bash
git add crates/ferrite-web/src/pages/guide_index.rs
git commit -m "feat(web): guide index 2-column card grid with gradient/screenshot backgrounds"
```

---

## Task 5: Screenshot capture script

**Files:**
- Create: `tests/webapp/scripts/capture-screenshots.ts`

**Prerequisites:** The webapp `dist/` must be built (`cd crates/ferrite-webapp && trunk build`) and the serve command must be running (`npx serve -l 8080 ../../crates/ferrite-webapp/dist` from `tests/webapp/`).

Tab bar pixel positions at 1280×720 viewport (approximate — adjust if clicks miss):
- "⚙ Config": x=45, y=20
- "🖼 Sprites": x=130, y=20
- "🤖 State Machine": x=250, y=20
- "▶ Simulation": x=400, y=20

- [ ] **Step 1: Create the script**

Create `tests/webapp/scripts/capture-screenshots.ts`:

```typescript
/**
 * Playwright screenshot capture for ferrite-web guide images.
 *
 * Run after building the webapp dist:
 *   cd tests/webapp
 *   npx serve -l 8080 ../../crates/ferrite-webapp/dist &
 *   npx ts-node scripts/capture-screenshots.ts
 *
 * Output: crates/ferrite-web/assets/guides/{slug}.png
 */

import { chromium } from '@playwright/test';
import * as path from 'path';
import * as fs from 'fs';

const BASE_URL = 'http://localhost:8080';
const OUT_DIR = path.resolve(__dirname, '../../../crates/ferrite-web/assets/guides');

async function waitForApp(page: any): Promise<void> {
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== 'undefined',
    { timeout: 15000 }
  );
  // Extra wait for first render
  await page.waitForTimeout(800);
}

// Tab bar pixel positions at 1280×720. Adjust if tabs shift.
const TAB_POSITIONS: Record<string, { x: number; y: number }> = {
  config:    { x: 45,  y: 20 },
  sprites:   { x: 130, y: 20 },
  sm:        { x: 250, y: 20 },
  simulation:{ x: 400, y: 20 },
};

const CAPTURES = [
  {
    slug: 'getting-started',
    tab: 'simulation',
    settle: 1200, // ms — let pet animation settle
  },
  {
    slug: 'custom-sprites',
    tab: 'sprites',
    settle: 500,
  },
  {
    slug: 'state-machines',
    tab: 'sm',
    settle: 500,
  },
  {
    slug: 'configuration',
    tab: 'config',
    settle: 500,
  },
];

(async () => {
  fs.mkdirSync(OUT_DIR, { recursive: true });

  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.setViewportSize({ width: 1280, height: 720 });

  console.log(`Navigating to ${BASE_URL}…`);
  await page.goto(BASE_URL);
  await waitForApp(page);

  for (const { slug, tab, settle } of CAPTURES) {
    const pos = TAB_POSITIONS[tab];
    console.log(`Clicking ${tab} tab at (${pos.x}, ${pos.y})…`);
    await page.mouse.click(pos.x, pos.y);
    await page.waitForTimeout(settle);

    const outPath = path.join(OUT_DIR, `${slug}.png`);
    await page.screenshot({ path: outPath });
    console.log(`  → saved ${outPath}`);
  }

  await browser.close();
  console.log('Done.');
})();
```

- [ ] **Step 2: Run the script to capture screenshots**

```bash
# In one terminal — serve the webapp
cd tests/webapp
npx serve -l 8080 ../../crates/ferrite-webapp/dist

# In another terminal — run the capture
cd tests/webapp
npx ts-node scripts/capture-screenshots.ts
```

Expected output:
```
Navigating to http://localhost:8080…
Clicking simulation tab at (400, 20)…
  → saved .../assets/guides/getting-started.png
Clicking sprites tab at (130, 20)…
  → saved .../assets/guides/custom-sprites.png
Clicking sm tab at (250, 20)…
  → saved .../assets/guides/state-machines.png
Clicking config tab at (45, 20)…
  → saved .../assets/guides/configuration.png
Done.
```

If a click misses its tab (screenshot shows wrong content), open the webapp at 1280×720 and use browser DevTools to find the correct pixel position for each tab, then update `TAB_POSITIONS` in the script.

- [ ] **Step 3: Verify screenshots look correct**

Open each PNG in `crates/ferrite-web/assets/guides/` and confirm it shows the expected tab.

- [ ] **Step 4: Commit the script**

```bash
git add tests/webapp/scripts/capture-screenshots.ts
git commit -m "chore(webapp): add Playwright screenshot capture script for guide images"
```

---

## Task 6: Commit screenshots and add image references to guides

**Files:**
- Add: `crates/ferrite-web/assets/guides/*.png` (4 files from Task 5)
- Modify: `crates/ferrite-web/guides/getting-started.md`
- Modify: `crates/ferrite-web/guides/custom-sprites.md`
- Modify: `crates/ferrite-web/guides/state-machines.md`
- Modify: `crates/ferrite-web/guides/configuration.md`

- [ ] **Step 1: Add image reference to `getting-started.md`**

Insert after the first paragraph (after the line ending in "A sheep will appear on your desktop!"):

```markdown
![Ferrite simulation — the pet walks along your desktop surfaces](/ferrite/assets/guides/getting-started.png)
```

- [ ] **Step 2: Add image reference to `custom-sprites.md`**

Insert after the `# Custom Sprites` heading line (before "Ferrite supports Aseprite-exported spritesheets."):

```markdown
![Sprite editor showing the eSheep spritesheet gallery](/ferrite/assets/guides/custom-sprites.png)
```

- [ ] **Step 3: Add image reference to `state-machines.md`**

Insert after the `# State Machines` heading:

```markdown
![State machine editor with TOML DSL and state graph](/ferrite/assets/guides/state-machines.png)
```

- [ ] **Step 4: Add image reference to `configuration.md`**

Insert after the `# Configuration` heading:

```markdown
![Configuration panel showing pet settings](/ferrite/assets/guides/configuration.png)
```

- [ ] **Step 5: Verify in browser**

The `dx serve` process should hot-reload. Navigate to http://localhost:8081/ferrite/guides/getting-started — the screenshot should appear below the first paragraph, full-width with rounded corners and a subtle border.

- [ ] **Step 6: Commit everything**

```bash
git add crates/ferrite-web/assets/guides/ crates/ferrite-web/guides/
git commit -m "feat(web): add guide screenshots and embed in markdown"
```

---

## Task 7: Update BACKLOG.md

- [ ] **Step 1: Mark M-01, M-02, W-01, W-02 as done**

In `BACKLOG.md`, find the Mobile / Touch and Website — Guides Pages sections and change all four `- [ ]` lines to `- [x]`.

- [ ] **Step 2: Commit**

```bash
git add BACKLOG.md
git commit -m "chore: mark M-01 M-02 W-01 W-02 complete in BACKLOG.md"
```
