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
  const file = resolve(dist, '.' + (path.startsWith('/properties') ? '/index.html' : path));
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
const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=', 'base64');
try {
  for (const room of [false, true]) {
    const page = await browser.newPage();
    page.setDefaultTimeout(8000);
    let stored = { id: 901, name: 'Autosave fixture', address: '123 Test St', notes: '', amenities: [], photos: [], spaces: [{ id: 1, name: 'Room 1', kind: 'Bedroom', amenities: [], photos: [], current_monthly_rent: 1000 }] };
    let writes = 0, active = 0, maxActive = 0, failNext = false, holdNext = false, release;
    const target = () => room ? stored.spaces[0] : stored;
    await page.route('**/api/**', async route => {
      const req = route.request(), path = new URL(req.url()).pathname;
      const json = data => route.fulfill({ json: data });
      if (path === '/api/me') return json({ id: 1, role: 'user', onboarding_completed: true, installed_apps: ['properties'] });
      if (path === '/api/properties/901/access') return json({ can_manage: true, capabilities: ['details.write', 'pricing.write'] });
      if (path === '/api/property-managers/staff') return json([]);
      if (path === '/api/ui-revision') return route.fulfill({ body: 'fixture' });
      if (path === '/api/images') return json(`/images/${++writes}`);
      if (path === '/api/properties' && req.method() === 'GET') return json([stored]);
      if (path === '/api/properties/901' && req.method() === 'PUT') {
        const data = req.postDataJSON(); writes++; maxActive = Math.max(maxActive, ++active);
        if (holdNext) { holdNext = false; await new Promise(r => { release = r; }); }
        active--;
        if (failNext) { failNext = false; return route.fulfill({ status: 500, body: 'Fixture save failure' }); }
        stored = data; return json(stored);
      }
      throw new Error(`Unexpected API request: ${req.method()} ${path}`);
    });
    await page.route('**/images/*', route => route.fulfill({ contentType: 'image/png', body: png }));
    const poll = async fn => { for (let n = 0; n < 150; n++) { if (fn()) return; await page.waitForTimeout(40); } assert.fail('Timed out waiting for persistence'); };
    const saved = () => page.locator('.availability-message').filter({ hasText: /^Saved$/ }).waitFor();
    const open = async () => {
      await page.goto(`${base}/properties/901${room ? '/rooms/1' : ''}`);
      await page.getByText('What this place offers', { exact: true }).click();
    };
    const edit = async (label, value) => {
      await page.getByRole('button', { name: `Edit ${label}`, exact: true }).click();
      await page.getByRole('textbox', { name: label, exact: true }).fill(value);
      await page.getByRole('textbox', { name: label, exact: true }).press('Enter');
    };
    await open();
    await page.getByRole('button', { name: 'Wi-Fi', exact: true }).click();
    await saved(); await open();
    assert.equal(await page.getByRole('button', { name: 'Wi-Fi', exact: true }).getAttribute('aria-pressed'), 'true');
    await page.getByRole('button', { name: 'Wi-Fi', exact: true }).click(); await saved();
    assert.deepEqual(target().amenities, []);
    await page.getByRole('textbox', { name: 'Add an amenity', exact: true }).fill('Piano');
    await page.getByRole('button', { name: 'Add amenity', exact: true }).click(); await saved();
    assert.deepEqual(target().amenities, ['Piano']);
    await page.getByRole('button', { name: 'Remove Piano', exact: true }).click(); await saved();
    assert.deepEqual(target().amenities, []);
    // Hold a text save open while amenities and uploaded photos change.
    holdNext = true;
    await edit(room ? 'Name' : 'Property name', 'Edited name');
    await poll(() => !!release);
    await page.getByRole('button', { name: 'Heating', exact: true }).click();
    await page.getByLabel('Upload gallery photos', { exact: true }).setInputFiles([
      { name: 'one.png', mimeType: 'image/png', buffer: png },
      { name: 'two.png', mimeType: 'image/png', buffer: png }
    ]);
    await page.getByText('2 of 2 files processed', { exact: true }).waitFor();
    release(); await saved();
    assert.equal(target().name, 'Edited name');
    assert.deepEqual(target().amenities, ['Heating']);
    assert.equal(target().photos.length, 2);
    assert.equal(maxActive, 1, 'property saves must stay serialized');
    await open();
    assert.equal(await page.locator('.gallery-photo').count(), 2);
    const original = [...target().photos];
    // Exercise the same reorder event the shared drag controller dispatches.
    await page.locator('.gallery-sort-result').evaluate(el => { el.value = '0,1'; el.dispatchEvent(new Event('input', { bubbles: true })); });
    await saved(); assert.deepEqual(target().photos, [...original].reverse());
    await page.getByRole('button', { name: 'Remove photo 1', exact: true }).click(); await saved();
    assert.deepEqual(target().photos, [original[0]]);
    await page.getByRole('button', { name: 'Undo', exact: true }).click(); await saved();
    assert.deepEqual(target().photos, [...original].reverse());
    if (room) await page.getByText('More room details', { exact: true }).click();
    await page.getByRole('button', { name: `Edit ${room ? 'Room' : 'Property'} notes`, exact: true }).click();
    await page.getByRole('textbox', { name: room ? 'Room notes' : 'Property notes', exact: true }).fill('Notes persist too');
    await page.getByRole('button', { name: `Save ${room ? 'Room' : 'Property'} notes`, exact: true }).click(); await saved();
    assert.equal(target().notes, 'Notes persist too');
    if (room) {
      await page.getByRole('button', { name: 'Edit Base monthly rent', exact: true }).click();
      await page.getByRole('spinbutton', { name: 'Base monthly rent', exact: true }).fill('1200');
      await page.getByRole('spinbutton', { name: 'Base monthly rent', exact: true }).press('Enter'); await saved();
      assert.equal(target().current_monthly_rent, 1200);
    }
    failNext = true;
    await page.getByRole('button', { name: 'Heating', exact: true }).click();
    await page.getByText('Could not save changes: Fixture save failure', { exact: true }).waitFor();
    assert.deepEqual(target().amenities, ['Heating']);
    await page.getByRole('button', { name: room ? 'Save room' : 'Save property', exact: true }).click(); await saved();
    assert.deepEqual(target().amenities, []);
    await open();
    assert.equal(await page.getByRole('button', { name: 'Heating', exact: true }).getAttribute('aria-pressed'), 'false');
    assert.deepEqual(target().photos, [...original].reverse());
    assert.equal(target().notes, 'Notes persist too');
    // New resources retain their explicit creation contract, including media.
    const previousWrites = writes;
    await page.goto(`${base}/properties/${room ? '901/rooms/new' : 'new'}`);
    await page.getByText('What this place offers', { exact: true }).click();
    await page.getByRole('button', { name: 'Wi-Fi', exact: true }).click();
    await page.getByText('Unsaved changes', { exact: true }).waitFor();
    await page.getByLabel('Upload gallery photos', { exact: true }).setInputFiles({ name: 'new.png', mimeType: 'image/png', buffer: png });
    await page.getByText('1 of 1 files processed', { exact: true }).waitFor();
    await page.waitForTimeout(150);
    assert.equal(writes, previousWrites + 1, 'new media uploads but its resource must wait for explicit Save');
    console.log(`PASS ${room ? 'room' : 'property'}: amenities, photos, ordering, undo, text, reload, serialization, retry, creation`);
    await page.close();
  }
} finally { await browser.close(); await new Promise(r => server.close(r)); }
