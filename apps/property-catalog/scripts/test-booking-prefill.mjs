// Run after `dx build --web --release` with Playwright installed.
// All API writes are intercepted; no database or signed-in account is needed.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname } from 'node:path';
import { createRequire } from 'node:module';
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || 'playwright');
const dist = resolve(process.env.APP_DIST || 'target/dx/buildry-property-catalog/release/web/public');
const server = createServer(async (req, res) => {
  const path = new URL(req.url, 'http://localhost').pathname;
  const file = resolve(dist, '.' + (path === '/book' ? '/index.html' : path));
  if (!file.startsWith(dist + '/')) { res.writeHead(403).end(); return; }
  try {
    const bytes = await readFile(file);
    res.setHeader('Content-Type', ({ '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css' })[extname(file)] || 'application/octet-stream');
    res.end(bytes);
  } catch { res.writeHead(404).end(); }
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ headless: true, channel: process.env.BROWSER_CHANNEL || 'chrome' });
const account = { first_name: 'Alex', last_name: 'Guest', phone: '5195550123', email: 'alex@example.test' };
const labels = ['Legal first name', 'Legal last name', 'Phone number', 'Email address'];
const values = Object.values(account);
const autocompletes = ['given-name', 'family-name', 'tel', 'email'];
try {
  for (const scenario of ['full', 'partial', 'anonymous', 'unavailable', 'malformed', 'delayed', 'cleared']) {
    const page = await browser.newPage();
    page.setDefaultTimeout(8000);
    let release, meRequests = 0, payload;
    const gate = new Promise(resolve => { release = resolve; });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/api/**', async route => {
      const req = route.request(), path = new URL(req.url()).pathname;
      if (path === '/api/ui-revision') return route.fulfill({ body: 'fixture' });
      if (path === '/api/me') {
        meRequests++;
        if (['delayed', 'cleared'].includes(scenario)) await gate;
        if (scenario === 'anonymous') return route.fulfill({ status: 401, body: 'Unauthorized' });
        if (scenario === 'unavailable') return route.abort('failed');
        if (scenario === 'malformed') return route.fulfill({ body: 'invalid json' });
        return route.fulfill({ json: scenario === 'partial' ? { first_name: 'Alex', phone: null } : account });
      }
      if (path === '/api/tours/options') return route.fulfill({ json: {
        today: '2026-11-14', last_date: '2026-12-14', now_time: '09:00',
        houses: [{ id: 1, name: 'Test house', rooms: [{ id: 2, name: 'Test room', monthly_rent: 1000 }] }]
      } });
      if (path === '/api/guest-listings') return route.fulfill({ json: [{
        id: 1, name: 'Test house', area: 'Waterloo', tours_available: true,
        rooms: [{ id: 2, name: 'Test room', kind: 'Bedroom', bathroom_access: '', monthly_price: 1000, price_is_estimate: false }]
      }] });
      if (path === '/api/tours/availability') return route.fulfill({ json: { available_slots: ['10:00', '11:00'] } });
      if (path === '/api/tours' && req.method() === 'POST') {
        payload = req.postDataJSON();
        return route.fulfill({ status: 400, body: 'Fixture validation failure' });
      }
      throw new Error(`Unexpected API request: ${req.method()} ${path}`);
    });
    const meFinished = page.waitForEvent(scenario === 'unavailable' ? 'requestfailed' : 'requestfinished', {
      predicate: request => new URL(request.url()).pathname === '/api/me'
    });
    await page.goto(`${base}/book?property_id=1&room_id=2&year=2027&start=jan&months=4`);
    const field = index => page.locator(`input[autocomplete="${autocompletes[index]}"]`);
    await page.locator('input[autocomplete=given-name]').waitFor();
    const assertValues = async expected => {
      // Waiting for the final expected value also waits for the account response/render.
      await page.waitForFunction(({autocompletes, expected}) => autocompletes.every((name, i) =>
        document.querySelector(`input[autocomplete="${name}"]`)?.value === expected[i]
      ), { autocompletes, expected });
      for (let i = 0; i < labels.length; i++) assert.equal(await field(i).inputValue(), expected[i]);
    };
    if (scenario === 'full') {
      await assertValues(values);
      for (let i = 0; i < labels.length; i++) await field(i).fill(['Sam', 'Visitor', '4165550100', 'sam@example.test'][i]);
      await page.getByRole('button', { name: /^Tour date:/ }).click();
      await page.locator('.date-picker-day:not([disabled])').first().click();
      await page.getByRole('button', { name: /^Start time/ }).click();
      await page.getByRole('option', { name: '10:00 AM', exact: true }).click();
      // Native validation leaves the user's cleared field intact.
      await field(0).fill('');
      await page.getByRole('button', { name: 'Book tour', exact: true }).click();
      assert.equal(payload, undefined);
      await assertValues(['', 'Visitor', '4165550100', 'sam@example.test']);
      await field(0).fill('Sam');
      await page.getByRole('button', { name: 'Book tour', exact: true }).click();
      await page.getByRole('alert').filter({ hasText: 'Fixture validation failure' }).waitFor();
      assert.equal(payload.legal_first_name, 'Sam');
      assert.equal(payload.legal_last_name, 'Visitor');
      assert.equal(payload.guest_phone, '4165550100');
      assert.equal(payload.guest_email, 'sam@example.test');
      await page.getByRole('button', { name: /^Tour date:/ }).click();
      await page.locator('.date-picker-day:not([disabled])').nth(1).click();
      await assertValues(['Sam', 'Visitor', '4165550100', 'sam@example.test']);
    } else if (scenario === 'delayed') {
      await field(0).fill('Someone else');
      await field(2).fill('4165550100');
      release();
      await assertValues(['Someone else', 'Guest', '4165550100', 'alex@example.test']);
    } else if (scenario === 'cleared') {
      for (let i = 0; i < labels.length; i++) { await field(i).fill('Temporary'); await field(i).fill(''); }
      release();
      await meFinished;
      await page.waitForTimeout(100);
      await assertValues(['', '', '', '']);
    } else {
      await meFinished;
      await page.waitForTimeout(100);
      await assertValues(scenario === 'partial' ? ['Alex', '', '', ''] : ['', '', '', '']);
    }
    await meFinished;
    assert.equal(meRequests, 1, `${scenario}: account should load once`);
    assert.deepEqual(errors, [], `${scenario}: browser errors`);
    await page.close();
    console.log(`PASS ${scenario}`);
  }
} finally {
  await browser.close();
  await new Promise(resolve => server.close(resolve));
}
