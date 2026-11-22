const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

let chromium;
try {
  // Optional global test installation used by the UI test environment.
  ({ chromium } = require('/home/genescan/.local/lib/node24/lib/node_modules/openclaw/node_modules/playwright-core'));
} catch {
  ({ chromium } = require('playwright-core'));
}

const root = path.resolve(__dirname, '../..');
const fixtureDir = process.env.HELIXSCAN_UI_FIXTURES || '/home/genescan/helixscan/helixscan-runs';
const saved = JSON.parse(fs.readFileSync(path.join(fixtureDir, 'demo-saved-report.json'), 'utf8'));
const snapshot = JSON.parse(fs.readFileSync(path.join(fixtureDir, 'demo-saved-snapshot.json'), 'utf8'));
const missing = JSON.parse(JSON.stringify(saved));
delete missing.report.sample_checks.ancestry_summary;
const missingSnapshot = { ...snapshot, result: missing };
const executablePath = process.env.CHROMIUM_PATH || path.join(root, '.look-tmp/browsers/chromium-1248/chrome-linux64/chrome');
async function test() {
const browser = await chromium.launch({ headless: true, executablePath, args: ['--no-sandbox'] });
const errors = [];
for (const [name, data, reportSnapshot] of [['normal', saved, snapshot], ['missing-fields', missing, missingSnapshot]]) {
  const context = await browser.newContext({ viewport: { width: 1344, height: 752 } });
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(error.message));
  const server = http.createServer((req, res) => {
    const url = req.url.split('?')[0];
    if (['/styles.css', '/visuals.css', '/responsive.css', '/print.css', '/app.js', '/helix.js'].includes(url)) {
      res.writeHead(200, { 'Content-Type': url.endsWith('.css') ? 'text/css' : 'text/javascript' });
      res.end(fs.readFileSync(path.join(root, 'web', url)));
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(fs.readFileSync(path.join(root, 'web/index.html')));
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  await page.route('**/api/files', route => route.fulfill({ json: [] }));
  await page.route('**/api/panel', route => route.fulfill({ json: { variants: 77, genes: [], searched_traits: 39 } }));
  await page.route('**/api/scans/*/report.json', route => route.fulfill({ json: data }));
  await page.route('**/reports/*', route => route.fulfill({ json: reportSnapshot }));
  await page.goto(`http://127.0.0.1:${server.address().port}/reports/${data.report_id}/report`);
  await page.evaluate(id => window.loadReportById(id), data.report_id);
  await page.locator('.story-chapter').first().waitFor();
  const durations = await page.evaluate(() => [0.8, 1, 12, 600].map(value => window.helixscanReportTest.readableDuration(value)));
  assert.deepEqual(durations, ['under a second', '1 second', '12 seconds', '10 minutes']);
  for (const media of ['screen', 'print']) {
    await page.emulateMedia({ media });
    const rendered = await page.locator('body').innerText();
    assert.doesNotMatch(rendered, /\b(?:undefined|NaN)\b/, `${name} ${media} has an unrenderable value`);
  }
  await page.emulateMedia({ media: 'screen' });
  await page.locator('[data-detail="sample-ancestry"]').first().click();
  assert.doesNotMatch(await page.locator('#detail-panel').innerText(), /\b(?:undefined|NaN)\b/);
  await page.locator('#detail-close').click();
  await page.locator('.story-card').first().click();
  assert.doesNotMatch(await page.locator('#detail-panel').innerText(), /\b(?:undefined|NaN)\b/);
  await context.close();
  await new Promise(resolve => server.close(resolve));
}
await browser.close();
assert.deepEqual(errors, []);
console.log('PASS: no undefined/NaN text in report, booklet, or details; duration wording covered');
}
test().catch(error => { console.error(error); process.exitCode = 1; });
