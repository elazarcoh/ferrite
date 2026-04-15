/**
 * Playwright screenshot capture for ferrite-web guide images.
 *
 * Run after building the webapp dist:
 *   cd tests/webapp
 *   npx serve -l 8080 ../../crates/ferrite-webapp/dist &
 *   npx tsx scripts/capture-screenshots.ts
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
    '() => typeof window.__ferrite !== "undefined"',
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
