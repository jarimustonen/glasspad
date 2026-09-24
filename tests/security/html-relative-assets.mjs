// Exercise authored full-document URLs inside the null-origin iframe on both mounts.
import { chromium } from "playwright";

const [space] = process.argv.slice(2);
if (!space) throw Error("expected space URL");
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const requests = [];
  page.on("request", r => requests.push(r.url()));
  await page.goto(`${space}/`);
  const frame = page.frameLocator("#gp-artifact");
  await frame.locator("#photo").evaluate(img => img.decode());
  if (!(await frame.locator("#photo").evaluate(img => img.naturalWidth > 0))) throw Error("root AVIF failed");
  if (await frame.locator("#photo").getAttribute("src") !== "assets/sub/photo.avif") throw Error("authored HTML changed");
  if (await frame.locator("#styled").evaluate(el => getComputedStyle(el).color) !== "rgb(7, 8, 9)") throw Error("relative CSS failed");
  if (await frame.locator("#scripted").textContent() !== "loaded") throw Error("relative JS failed");
  for (const asset of ["sub/photo.avif", "site.css", "app.js"]) {
    if (!requests.includes(`${space}/_c/assets/${asset}`)) throw Error(`missing iframe asset request ${asset}`);
  }
  if (await frame.locator("body").evaluate(el => el.ownerDocument.defaultView.origin) !== "null") throw Error("iframe origin widened");
  await frame.locator('a[href="#section"]').click();
  if (!(await frame.locator("body").evaluate(el => el.ownerDocument.defaultView.location.hash) === "#section")) throw Error("fragment lost");
  // Full documents navigate to the shell explicitly; do not change base href or target.
  const beforeGuide = requests.length;
  await frame.locator("#guide-link").click();
  await page.waitForURL(`${space}/guide`);
  await frame.locator("#nested-photo").evaluate(img => img.decode());
  if (!(await frame.locator("#nested-photo").evaluate(img => img.naturalWidth > 0))) throw Error("second HTML page AVIF failed");
  if (!requests.slice(beforeGuide).includes(`${space}/_c/assets/sub/photo.avif`)) throw Error("second page request missed alias");
  console.log(`PASS  Full HTML relative assets, links and fragments in null-origin iframe: ${space}`);
} finally {
  await browser.close();
}
