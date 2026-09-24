// Real-host check: serve a temp `indexspace/` with template: index, the index
// sample as index.md, sibling prose.md/report.md, and a raw HTML link card
// linking to ./prose.html with a secondary ./report.html link (bridge maps
// these explicit .html link stems to the same-space Markdown pages).
// GLASSPAD_PORT=<port> node tests/security/index.mjs
import assert from 'node:assert/strict';
import { chromium } from 'playwright';

const port = process.env.GLASSPAD_PORT;
assert.ok(port, 'GLASSPAD_PORT required');
const browser = await chromium.launch({ headless: true });
try {
  for (const width of [360, 1280]) for (const theme of ['light', 'dark']) for (const js of [true, false]) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, javaScriptEnabled: js });
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${port}/indexspace/${js ? 'index' : '_c/index'}${!js && theme === 'dark' ? '?gp_theme=dark' : ''}`);
    const frame = js ? page.frameLocator('iframe') : page;
    await frame.locator('.gp-index').waitFor();
    if (js && theme === 'dark') {
      const toggle = page.getByRole('button', { name: /theme/i });
      await toggle.click(); await toggle.click();
      await frame.locator('html[data-theme="dark"]').waitFor();
    }
    assert.equal(await frame.locator('.gp-index > h2').count(), 5);
    assert.equal(await frame.locator('.gp-index > ul').count(), 4);
    assert.equal(await frame.locator('.gp-index > ul:first-of-type > li').count(), 2);
    const card = frame.locator('.gp-index > ul:first-of-type > li').first();
    const later = frame.locator('.gp-index > ul').nth(1).locator('li').first();
    const backgrounds = await Promise.all([card, later].map(n => n.evaluate(el => getComputedStyle(el).backgroundColor)));
    assert.notEqual(backgrounds[0], backgrounds[1], 'first group featured');
    const grid = await card.locator('..').evaluate(el => getComputedStyle(el).gridTemplateColumns.split(' ').length);
    assert.equal(grid, width === 360 ? 1 : 2, 'responsive first group');
    const dimensions = await frame.locator('body').evaluate(() => ({ page: document.documentElement.scrollWidth, view: innerWidth }));
    assert.ok(dimensions.page <= dimensions.view + 1, `overflow ${width}/${theme}/${js}: ${JSON.stringify(dimensions)}`);
    assert.ok((await frame.locator('.gp-index').innerText()).includes('Restricted records'));
    const primary = card.locator('a').first();
    assert.equal(await primary.getAttribute('href'), './prose.md', 'sample Markdown href is preserved');
    assert.equal(await primary.evaluate(el => el.tagName), 'A');
    assert.ok((await primary.evaluate(el => getComputedStyle(el, '::after').position)) === 'absolute', 'card overlay is anchored');
    await primary.focus();
    assert.equal(await primary.evaluate(el => el.matches(':focus-visible')), true);
    assert.equal(await card.evaluate(el => getComputedStyle(el).outlineStyle), 'solid', 'card focus ring');
    const extra = frame.locator('.gp-index > ul').last().locator('li');
    assert.equal(await extra.locator('a').count(), 2);
    assert.equal(await extra.locator('a').nth(1).getAttribute('href'), './report.html');
    assert.equal(await extra.locator('button').count(), 1);
    if (js) {
      await extra.locator('button').click();
      assert.equal(await extra.locator('button').getAttribute('data-clicked'), 'yes', 'authored control is clickable above card link');
    }
    // The overlay does not steal secondary-link clicks. Without JS, the
    // fragment remains readable, but bridge-based same-space swapping is absent.
    if (js) {
      await extra.locator('a').nth(1).click();
      await page.frameLocator('iframe').locator('h1').filter({ hasText: 'Report' }).waitFor();
      assert.equal(await page.frameLocator('iframe').locator('h1').first().innerText(), 'Report');
      await page.goto(`http://127.0.0.1:${port}/indexspace/index`);
      const f = page.frameLocator('iframe');
      await f.locator('.gp-index').waitFor();
      const destination = f.locator('.gp-index > ul').last().locator('li');
      const box = await destination.boundingBox();
      await destination.click({ position: { x: box.width - 20, y: 12 } });
      await page.frameLocator('iframe').locator('h1').filter({ hasText: 'Prose' }).waitFor();
    }
    await page.goto(`http://127.0.0.1:${port}/indexspace/_c/index`);
    await page.emulateMedia({ media: 'print' });
    assert.equal(await page.locator('.gp-index > ul').first().evaluate(el => getComputedStyle(el).display), 'block');
    const pdf = await page.pdf({ printBackground: true });
    assert.ok(pdf.length > 1000);
    console.log(`PASS index ${width}px ${theme} script=${js}`);
    await context.close();
  }
} finally { await browser.close(); }
