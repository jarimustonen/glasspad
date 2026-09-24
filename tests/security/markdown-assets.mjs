// Run against the live loopback and local hosted read routes from test-security.sh.
// Assert browser-resolved requests, not just serialized render strings.
import { chromium } from "playwright";

const [shell, space] = process.argv.slice(2);
if (!shell || !space) throw Error("expected shell URL and space root");
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const requests = [];
  page.on("request", (request) => requests.push(request.url()));
  await page.goto(shell);
  const frame = page.frameLocator("#gp-artifact");
  const image = frame.locator('img[alt="Screenshot"]');
  await image.waitFor();
  await image.evaluate((img) => img.decode());
  if (!(await image.evaluate((img) => img.naturalWidth > 0))) {
    throw Error("AVIF did not decode in the iframe");
  }
  const expectedImage = `${space}/assets/sub/photo.avif`;
  if (!requests.includes(expectedImage)) {
    throw Error(`image request missed allowed route: ${JSON.stringify(requests)}`);
  }
  const raw = await frame.locator("#raw-html-url").getAttribute("src");
  if (raw !== "./assets/sub/photo.avif") throw Error(`raw HTML changed: ${raw}`);
  const dataRequest = page.waitForRequest((r) => r.url() === `${space}/assets/sub/data.json`);
  await frame.getByText("Data", { exact: true }).click();
  await dataRequest;
  const asset = await page.request.get(`${space}/assets/sub/data.json`);
  if (asset.status() !== 200 || !(await asset.text()).includes('"x":1')) {
    throw Error("ordinary asset link did not reach the scanned asset");
  }
  console.log(`PASS  Markdown AVIF and link resolve in sandbox to ${space}/assets/`);
} finally {
  await browser.close();
}
