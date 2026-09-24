// Real-host fixture: copy issues/base-template-gallery/designer-package/samples/board.md
// and tests/security/board-fixture.md (as explicit.md) into a temporary `board/` dir;
// write `template: board` to board/glasspad.yaml, then start
// `glasspad loopback serve --port 38931 /path/to/board`.
// GLASSPAD_PORT=38931 node tests/security/board.mjs
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
const port = process.env.GLASSPAD_PORT;
assert.ok(port, 'GLASSPAD_PORT required');
const browser = await chromium.launch({ headless: true });
try {
  for (const width of [360, 1280]) for (const theme of ['light', 'dark']) for (const js of [true, false]) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, javaScriptEnabled: js });
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${port}/board/${js ? 'board' : '_c/board'}${!js && theme === 'dark' ? '?gp_theme=dark' : ''}`);
    const frame = js ? page.frameLocator('iframe') : page;
    await frame.locator('.gp-board').waitFor();
    if (!js && theme === 'dark') assert.equal(await page.locator('html').getAttribute('data-theme'), 'dark');
    if (js) {
      // The shell theme toggle controls the sandboxed artifact through the real bridge.
      if (theme === 'dark') {
        const toggle = page.getByRole('button', { name: /theme/i });
        await toggle.click(); // auto → light
        await toggle.click(); // light → dark
        await frame.locator('html[data-theme="dark"]').waitFor();
      }
      await frame.locator('.gp-board[data-gp-enhanced]').waitFor();
      assert.deepEqual(await frame.locator('.gp-board-lane > h2').allTextContents().then(a => a.map(s => s.replace(/\d+\s*items?/, '').trim())), ['Done', 'Next', 'Blocked', 'Later', 'Dependency view']);
      assert.deepEqual(await frame.locator('.gp-board-lane[data-status]').evaluateAll(nodes => nodes.map(n => n.dataset.status)), ['done', 'next', 'blocked', 'future']);
      assert.equal(await frame.locator('.gp-board-tally > li').count(), 4);
      assert.equal(await frame.locator('.gp-board-meta').count(), 6);
      assert.equal(await frame.locator('.gp-board-lane.is-wide .gp-diagram').count(), 1);
    } else {
      assert.equal(await frame.locator('.gp-board:not([data-gp-enhanced]) > h2').count(), 5);
      assert.equal(await frame.locator('.gp-board-lane').count(), 0);
      assert.equal(await frame.locator('.gp-board a').count() > 0, true);
    }
    const dimensions = await frame.locator('body').evaluate(body => ({ page: document.documentElement.scrollWidth, view: innerWidth, diagram: body.querySelector('.gp-diagram').getBoundingClientRect().width }));
    assert.ok(dimensions.page <= dimensions.view + 1, `page overflow at ${width}/${theme}/${js}: ${JSON.stringify(dimensions)}`);
    assert.ok(dimensions.diagram <= dimensions.view, `diagram overflow at ${width}`);
    if (js && width === 360) {
      const lane = frame.locator('.gp-board-lane').first();
      const next = frame.locator('.gp-board-lane').nth(1);
      assert.ok((await next.boundingBox()).y > (await lane.boundingBox()).y, 'phone lanes stack');
    }
    if (js) {
      const colors = await frame.locator('.gp-board-lane[data-status]').evaluateAll(nodes => nodes.map(n => getComputedStyle(n).getPropertyValue('--gp-status').trim()));
      assert.ok(colors.every(Boolean), 'status colors present');
      assert.equal(new Set(colors).size, 4, 'status palette distinct');
      assert.deepEqual(await frame.locator('.gp-board-lane[data-status] > h2').evaluateAll(nodes => nodes.map(n => getComputedStyle(n, '::before').content.split(' / ')[0].replaceAll('"', ''))), ['✓', '→', '!', '…']);
      const link = frame.locator('.gp-board a').first();
      await link.focus();
      assert.equal(await link.evaluate(n => n.matches(':focus-visible')), true, 'keyboard link focus');
      await page.goto(`http://127.0.0.1:${port}/board/explicit`);
      const f = page.frameLocator('iframe');
      await f.locator('.gp-board[data-gp-enhanced]').waitFor();
      const statuses = await f.locator('.gp-board-lane').evaluateAll(nodes => nodes.map(n => n.dataset.status || 'neutral'));
      assert.deepEqual(statuses, ['blocked', 'done', 'next', 'future', 'done', 'next', 'blocked', 'future', 'neutral']);
      assert.equal(await f.locator('.gp-board-lane.is-wide[data-status]').count(), 4);
      assert.equal(await f.locator('input[type=checkbox]:checked:disabled').count(), 1);
      assert.equal(await f.locator('.gp-board-tally > li').count(), 8);
      assert.equal(await f.locator('.gp-board-meta').count(), 2);
      assert.equal(await f.locator('.gp-board-head > ul').count(), 0);
      assert.equal(await f.locator('.gp-board > ul:not(.gp-board-tally) > li').count(), 1);
      assert.equal(await f.locator('.gp-board > ul:not(.gp-board-tally)').evaluate(n => getComputedStyle(n).display), 'grid');
      assert.equal(await f.locator('.gp-board-lane[aria-labelledby]').count(), 9);
      assert.ok(await f.locator('.gp-board > ul:not(.gp-board-tally)').evaluate(n => n.compareDocumentPosition(document.querySelector('.gp-board-lanes')) & Node.DOCUMENT_POSITION_FOLLOWING));
      const wide = await f.locator('body').evaluate(body => ({ page: document.documentElement.scrollWidth, view: innerWidth, table: body.querySelector('.gp-board-lane.is-wide table').scrollWidth }));
      assert.ok(wide.page <= wide.view + 1, `wide table escaped at ${width}: ${JSON.stringify(wide)}`);
      const blocked = await f.locator('.gp-board-lane.is-wide.gp-status-blocked').evaluate(n => getComputedStyle(n).borderColor);
      const expected = await f.locator('.gp-board-lane.is-wide.gp-status-blocked').evaluate(n => { const probe = document.createElement('span'); probe.style.color = 'var(--gp-status)'; n.appendChild(probe); const color = getComputedStyle(probe).color; probe.remove(); return color; });
      assert.equal(blocked, expected, 'wide blocked lane has status stroke');
      assert.ok((await f.locator('.gp-board-meta').last().innerText()).includes('Aino'));
      assert.ok((await f.locator('.gp-board-lane').nth(2).innerText()).includes('owner: Noor: keep raw text'));
      assert.ok((await f.locator('.gp-board-lane').nth(8).innerText()).includes('Inconnu'));
      const written = await f.locator('.gp-board-tally > li').allTextContents();
      assert.ok(written.some(s => s.startsWith('Blocked: 1 Diagram blocked')) && written.some(s => s.startsWith('Done: 1 Table done')));
      await page.emulateMedia({ media: 'print' });
      assert.equal(await f.locator('.gp-board-lanes').evaluate(n => getComputedStyle(n).display), 'block');
      await page.pdf({ printBackground: true });
    } else {
      await page.goto(`http://127.0.0.1:${port}/board/_c/explicit`);
      const w = await page.locator('body').evaluate(() => ({ page: document.documentElement.scrollWidth, view: innerWidth }));
      assert.ok(w.page <= w.view + 1, `no-script table escaped at ${width}: ${JSON.stringify(w)}`);
      assert.ok((await page.locator('.gp-board > ul:not(.gp-board-tally)').first().innerText()).includes('Intro task'));
      assert.equal(await page.locator('input[type=checkbox]:checked:disabled').count(), 1);
    }
    console.log(`PASS board ${width}px ${theme} script=${js}`);
    await context.close();
  }
} finally { await browser.close(); }
