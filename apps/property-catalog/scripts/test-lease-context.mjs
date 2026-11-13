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
  const file = resolve(dist, '.' + (path.startsWith('/leases') ? '/index.html' : path));
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
const details = Object.fromEntries('start_date end_date term_type other_term monthly_rent rent_due_day rent_payment_method parking condo services utilities rent_deposit key_deposit smoking insurance additional_terms unit_description occupants notes landlord_notice_email landlord_contact_details laundry_fee electricity_payer heat_payer water_payer rent_discount partial_rent nsf_fee key_description'.split(' ').map(key => [key, '']));
for (const key of 'email_notices gas_included ac_included storage_included laundry_included guest_parking_included'.split(' ')) details[key] = false;
const template = (id, landlord_id) => ({ id, name: `Template ${id}`, property_id: null, room_id: 0, lease_type: 'room', landlord_id, details, landlord_signed_at: null, landlord_signature: null });
const house = (id, rooms) => ({ id, name: `House ${id}`, address: '123 Test St', notes: '', spaces: rooms.map(id => ({id, name: `Room ${id}`, kind: 'Bedroom', current_monthly_rent: 1000})) });
const houses = [house(1, [9001, 9002]), house(2, [9003])];
const owners = [1,2].map(id => ({id, legal_name: `Owner ${id}`, service_address: '', email: '', phone: '', signer_name: '', signer_title: '', signature_authorized: true, property_ids: [id]}));
const tour = {id: 71, property_id: 2, property_name: 'House 2', room_id: 9003, rental_start: '2032-05-01', rental_months: 7, quoted_monthly_rent: 1350, tour_date: '2031-01-01', slot_start: '10:00', guest_name: 'Alex Guest', guest_phone: '5195550100', guest_email: 'alex@example.test'};
try {
  for (const scenario of ['vacancy', 'manual', 'multiple', 'tour', 'incompatible', 'ordinary']) {
    const page = await browser.newPage();
    page.setDefaultTimeout(10000);
    let payload;
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    const templates = scenario === 'incompatible' ? [template(2,2)] : [template(1,1), ...(scenario === 'multiple' ? [template(3,1)] : []), template(2,2)];
    await page.route('**/api/**', async route => {
      const req = route.request(), path = new URL(req.url()).pathname;
      const json = value => route.fulfill({json:value});
      if (req.method() === 'POST') { payload = req.postDataJSON(); return route.fulfill({status:400, body:'Fixture save stopped'}); }
      if (path === '/api/me') return json({id:1, role:'user', onboarding_completed:true, installed_apps:['leases']});
      if (path === '/api/ui-revision') return route.fulfill({body:'fixture'});
      if (path === '/api/leases') return json([]);
      if (path === '/api/lease-templates') return json(templates);
      if (path === '/api/properties') return json(houses);
      if (path === '/api/landlords') return json(owners);
      if (path === '/api/email/status') return json({configured:true});
      if (path === '/api/tours/admin-options') return json({today:'2030-01-01', now_time:'09:00', houses:[], last_date:'2030-02-01'});
      if (path === '/api/tours') return json([tour]);
      throw new Error(`Unexpected API: ${path}`);
    });
    const review = () => page.getByRole('button', {name:'Save draft', exact:true}).waitFor();
    const chooseGuest = () => page.getByRole('button', {name:/Alex Guest/}).click();
    await page.goto(`${base}/leases/new${scenario === 'tour' ? '?tour=71' : scenario === 'ordinary' ? '' : '?property=1&room=9001&start=2031-01-01'}`);
    if (scenario === 'incompatible') {
      await page.getByRole('button', {name:/Template 2/}).click();
      await page.getByText(/does not manage the selected property/).waitFor();
      await page.getByRole('button', {name:'Change property', exact:true}).click();
      await page.getByRole('button', {name:/House 1/}).click();
      await page.getByRole('button', {name:/Template 2/}).waitFor();
      await page.getByRole('button', {name:'Change property',exact:true}).click();
      await page.getByRole('button', {name:/House 2/}).click();
      await chooseGuest();
      await page.getByRole('button', {name:/Room 9003/}).click(); await review();
      await page.getByRole('button', {name:'Save draft',exact:true}).click();
      await page.getByText('Fixture save stopped',{exact:true}).waitFor();
      assert.equal(payload.property_id,2); assert.equal(payload.room_id,9003);
      assert.equal(payload.start_date,'2031-01-01');
      assert.deepEqual(errors, []); console.log(`PASS ${scenario}`); await page.close(); continue;
    }
    if (['multiple','ordinary'].includes(scenario)) await page.getByRole('button', {name:/Template 1/}).click();
    if (scenario === 'manual') {
      await page.getByText('Add a new guest', {exact:true}).click();
      for (const [name,value] of [['First name','Sam'],['Last name','Guest'],['Email address','sam@example.test']]) await page.getByRole('textbox',{name,exact:true}).fill(value);
      await page.getByRole('button',{name:'Use this guest',exact:true}).click();
    } else if (scenario !== 'tour') await chooseGuest();
    await review();
    const expectedProperty = ['tour','ordinary'].includes(scenario) ? 2 : 1;
    const expectedRoom = expectedProperty === 2 ? 9003 : 9001;
    const rows = page.locator('.lease-wizard-details');
    assert.match(await rows.filter({hasText:'Rental unit'}).innerText(), new RegExp(`Room ${expectedRoom}`));
    if (scenario === 'vacancy') {
      await page.getByRole('button',{name:'Change Template',exact:true}).click();
      await page.getByRole('button',{name:/Template 2/}).click();
      await page.getByText(/does not manage the selected property/).waitFor();
      await page.getByRole('button',{name:/Template 1/}).click();
      await review();
      await page.getByRole('button',{name:'Change Rental unit',exact:true}).click();
      await page.getByRole('button',{name:/Room 9002/}).click(); await review();
      await page.getByRole('button',{name:'Change Guest',exact:true}).click(); await chooseGuest(); await review();
      assert.match(await rows.filter({hasText:'Rental unit'}).innerText(), /Room 9002/);
      await page.getByRole('button',{name:'Change Rental unit',exact:true}).click();
      await page.getByRole('button',{name:/Room 9001/}).click(); await review();
      await page.screenshot({path:process.env.SCREENSHOT_PATH || '/tmp/bui6-review.png', fullPage:true});
      await page.setViewportSize({width:390,height:844});
      await page.waitForTimeout(250);
      await page.screenshot({path:'/tmp/bui6-review-mobile.png',fullPage:true});
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'Review should fit mobile width');
    }
    await page.getByRole('button',{name:'Save draft',exact:true}).click();
    await page.getByText('Fixture save stopped',{exact:true}).waitFor();
    assert.equal(payload.property_id, expectedProperty); assert.equal(payload.room_id, expectedRoom);
    const start = scenario === 'manual' ? payload.details.start_date : payload.start_date;
    assert.equal(start, expectedProperty === 2 ? '2032-05-01' : '2031-01-01');
    if (scenario === 'tour') { assert.equal(payload.months,7); assert.equal(payload.monthly_rent,1350); assert.equal(payload.tenant_email,'alex@example.test'); }
    assert.deepEqual(errors, []);
    console.log(`PASS ${scenario}`); await page.close();
  }
} finally { await browser.close(); await new Promise(r => server.close(r)); }
