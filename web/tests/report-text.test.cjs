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
missing.report.sample_checks.ancestry = [];
const missingSnapshot = { ...snapshot, result: missing };
const executablePath = process.env.CHROMIUM_PATH || path.join(root, '.look-tmp/browsers/chromium-1248/chrome-linux64/chrome');

async function serveWeb() {
  const server = http.createServer((req, res) => {
    const url = req.url.split('?')[0];
    if (['/styles.css', '/visuals.css', '/responsive.css', '/print.css', '/app.js', '/helix.js'].includes(url)) {
      res.writeHead(200, { 'Content-Type': url.endsWith('.css') ? 'text/css' : 'text/javascript' });
      res.end(fs.readFileSync(path.join(root, 'web', url)));
    } else if (url.startsWith('/art/')) {
      res.writeHead(200, { 'Content-Type': url.endsWith('.mp4') ? 'video/mp4' : 'image/webp' });
      res.end(fs.readFileSync(path.join(root, 'web', url)));
    } else {
      const reportId = req.url.match(/\/reports\/([^/]+)\/report/)?.[1];
      const html = fs.readFileSync(path.join(root, 'web/index.html'), 'utf8');
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(reportId ? html.replace('window.pendingReportId=null', `window.pendingReportId=${JSON.stringify(reportId)}`) : html);
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  return server;
}

async function testFixture(browser, errors, [name, data, reportSnapshot]) {
  const context = await browser.newContext({ viewport: { width: 1344, height: 752 } });
  const server = await serveWeb();
  try {
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(`${name}: ${error.message}`));
    await page.route('**/api/files', route => route.fulfill({ json: [] }));
    await page.route('**/api/panel', route => route.fulfill({ json: { variants: 77, genes: [], searched_traits: 39 } }));
    await page.route('**/api/scans/*/report.json', route => route.fulfill({ json: data }));
    await page.route(`**/reports/${data.report_id}`, route => route.fulfill({ json: reportSnapshot }));
    await page.goto(`http://127.0.0.1:${server.address().port}/reports/${data.report_id}/report`);
    await page.locator('.story-chapter').first().waitFor({ timeout: 10000 });

    const durations = await page.evaluate(() => [0.8, 1, 12, 600].map(value => window.helixscanReportTest.readableDuration(value)));
    assert.deepEqual(durations, ['under a second', '1 second', '12 seconds', '10 minutes']);
    const opening = await page.evaluate(report => window.helixscanReportTest.personalHighlights(report), data.report);
    assert.ok(opening.length >= 3 && opening.length <= 5, `${name}: expected 3–5 personal highlights`);
    assert.ok(opening.every(h => h.icon && h.href.startsWith('#chapter-')));
    assert.equal(await page.locator('.personal-highlights svg:not(.highlight-arrow) use').evaluateAll(uses => uses.every(use => document.getElementById(use.getAttribute('href').slice(1)))), true, `${name}: every highlight icon resolves`);
    assert.equal(await page.locator('.personal-highlights a').evaluateAll(links => links.every(link => document.querySelector(link.hash))), true, `${name}: every highlight opens an existing chapter`);
    assert.ok(opening[0].text.includes('blue-eye-associated allele'));
    assert.doesNotMatch(opening[0].text, /blue\/non-blue/);
    assert.ok(opening.slice(0, 3).every(h => !/See the observed/.test(h.text)));
    assert.equal(opening.filter(h => h.icon === 'report').length, 1);
    if (name === 'normal') {
      assert.equal(opening.length, 4);
      assert.deepEqual(opening.map(h => h.title), [
        'Your eye-colour marker',
        'Your lactose-digestion marker',
        'CYP2C19 intermediate metabolizer',
        'Your closest reference comparison'
      ]);
      assert.ok(opening.at(-1).text.includes('rough'));
      assert.equal(opening.at(-1).icon, 'globe');
    } else {
      assert.equal(opening.length, 3);
      assert.ok(!opening.some(h => h.icon === 'globe'));
    }
    const guards = await page.evaluate(report => {
      const unclear = structuredClone(report);
      const eye = unclear.variants.find(v => v.trait_group === 'Eye colour (blue/brown)');
      eye.called = false;
      eye.genotype = null;
      const unknownEye = structuredClone(report);
      unknownEye.variants.find(v => v.trait_group === 'Eye colour (blue/brown)').genotype = 'A/A';
      return [unclear, unknownEye].map(r => window.helixscanReportTest.personalHighlights(r).some(h => h.title === 'Your eye-colour marker'));
    }, data.report);
    assert.deepEqual(guards, [false, false], `${name}: unclear or unsupported eye-colour genotypes stay out of the opening`);

    await page.emulateMedia({ media: 'print' });
    const print = await page.evaluate(() => {
      const pages = [...document.querySelectorAll('.print-page')];
      return {
        count: pages.length,
        expected: 2 + document.querySelectorAll('.print-chapter-banner').length
          + Math.ceil(document.querySelectorAll('.print-evidence-card').length / 4)
          + Math.ceil(document.querySelectorAll('.print-table-page tbody tr').length / 18),
        medicineGroups: [...document.querySelectorAll('.print-chapter-medicines .print-medicines')].map(group => group.children.length),
        overflows: pages.filter(p => p.scrollHeight > p.clientHeight + 1).map(p => ({ classes: p.className, scroll: p.scrollHeight, client: p.clientHeight }))
      };
    });
    assert.equal(print.count, print.expected, `${name}: expected only authored booklet pages`);
    assert.ok(print.count > 2 && print.count < 100, `${name}: unexpected page count ${print.count}`);
    assert.ok(print.medicineGroups.length > 0 && print.medicineGroups.every(count => count <= 3), `${name}: medicine pagination regressed`);
    assert.deepEqual(print.overflows, [], `${name}: booklet pages overflow`);
    for (const media of ['screen', 'print']) {
      await page.emulateMedia({ media });
      const rendered = await page.locator('body').innerText();
      assert.doesNotMatch(rendered, /\b(?:undefined|NaN)\b/, `${name} ${media} has an unrenderable value`);
    }
    await page.emulateMedia({ media: 'screen' });
    for (const target of ['[data-detail="sample-ancestry"]', '.story-card']) {
      await page.locator(target).first().click();
      const overlay = await page.locator('#detail-panel').evaluate(panel => {
        const panelRect = panel.getBoundingClientRect();
        const closeRect = document.getElementById('detail-close').getBoundingClientRect();
        const scrimRect = document.getElementById('detail-scrim').getBoundingClientRect();
        return { inside: panelRect.left >= 0 && panelRect.right <= innerWidth && panelRect.top >= 0 && panelRect.bottom <= innerHeight + 1,
          closeVisible: closeRect.width > 0 && closeRect.height > 0,
          stacked: Number(getComputedStyle(panel).zIndex) > Number(getComputedStyle(document.getElementById('detail-scrim')).zIndex),
          scrim: scrimRect.width === innerWidth && scrimRect.height === innerHeight };
      });
      assert.equal(Object.values(overlay).every(Boolean), true, `${name}: evidence overlay stack and bounds`);
      assert.doesNotMatch(await page.locator('#detail-panel').innerText(), /\b(?:undefined|NaN)\b/);
      await page.locator('#detail-close').click();
      assert.equal(await page.locator('#detail-panel').isVisible(), false);
    }
    console.log(`${name}: ${opening.length} highlights; ${print.count} authored, overflow-free booklet pages`);
  } finally {
    await context.close();
    await new Promise(resolve => server.close(resolve));
  }
}

async function test() {
  const browser = await chromium.launch({ headless: true, executablePath, args: ['--no-sandbox'] });
  const errors = [];
  try {
    for (const fixture of [['normal', saved, snapshot], ['missing-fields', missing, missingSnapshot]]) {
      await testFixture(browser, errors, fixture);
    }
    assert.deepEqual(errors, []);
  } finally {
    await browser.close();
  }
  console.log('PASS: guarded personal opening, bounded overflow-free booklet, complete text and readable durations');
}

test().catch(error => { console.error(error); process.exitCode = 1; });
