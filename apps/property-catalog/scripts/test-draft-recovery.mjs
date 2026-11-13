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
  const file = resolve(dist, '.' + (!extname(path) ? '/index.html' : path));
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
const page = await browser.newPage();
page.setDefaultTimeout(10000);
const errors = [];
page.on('pageerror', e => errors.push(e.message));
let account = 1, allowed = true, fail = true, failAvailability = false, slots = ['10:00','11:00'], payload;
const task = { id: 91, title: 'Repair latch', description: 'Keep this notebook text', property_id: null, property_name: '', assignee: '', status: 'backlog', start_date: null, start_time: null, estimate_minutes: null, creator_id: 1, created_at: '', updated_at: '', can_edit: true };
const tourOptions = { today: '2026-11-14', last_date: '2026-12-14', now_time: '09:00', houses: [{ id: 1, name: 'House 1', rooms: [{ id: 9001, name: 'Room 9001', monthly_rent: 1000 }] }] };
const listings = [{ id:1, name:'House 1', area:'Waterloo', tours_available:true, rooms:[{id:9001,name:'Room 9001',kind:'Bedroom',bathroom_access:'',monthly_price:1000,price_is_estimate:false}] }];
await page.route('**/api/**', async route => {
  const req=route.request(), path=new URL(req.url()).pathname, json=value=>route.fulfill({json:value});
  if (path==='/api/me') return account === null ? route.fulfill({status:401}) : json({id:account,role:'user',onboarding_completed:true,installed_apps:['leases','tasks'],first_name:'Alex',last_name:'Guest',email:'alex@example.test',phone:'5195550100'});
  if (path==='/api/ui-revision') return route.fulfill({body:'fixture'});
  if (req.method()==='POST') {
    payload=req.postDataJSON();
    if(fail) return route.fulfill({status:500,body:'Recoverable fixture failure'});
    if(path==='/api/tasks') return json(task);
    if(path==='/api/leases'||path.startsWith('/api/prospects/')) return json({id:81,...payload,landlord_id:1,landlord_name:'Owner',template_id:1,tenant_id:null,status:'draft',can_edit:true,can_share:true,created_at:'',updated_at:''});
    if(path==='/api/tours') return json({token:'fixture',booking:{...tour,id:92,property_id:1,property_name:'House 1',room_id:9001,guest_email:payload.guest_email,tour_date:payload.tour_date,slot_start:payload.slot_start}});
  }
  if(path==='/api/my-signatures') return json([]);
  if(path==='/api/leases') return json([]);
  if(path==='/api/lease-templates') return json([template(1,1),template(2,2)]);
  if(path==='/api/properties'||path==='/api/tasks/properties') return json(allowed?houses:[]);
  if(path==='/api/landlords') return json(owners);
  if(path==='/api/email/status') return json({configured:true});
  if(path==='/api/tours/admin-options') return json({today:'2030-01-01',now_time:'09:00',houses:[],last_date:'2030-02-01'});
  if(path==='/api/tours/options') return json(tourOptions);
  if(path==='/api/guest-listings') return json(listings);
  if(path==='/api/tours/availability') return failAvailability ? route.fulfill({status:503,body:'Times unavailable'}) : json({available_slots:slots});
  if(path==='/api/tours') return json([tour]);
  if(path==='/api/tasks/91/bills') return json([]);
  if(path==='/api/tasks') return json([task]);
  if(path==='/api/tasks/users') return json([]);
  throw new Error(`Unexpected API: ${req.method()} ${path}`);
});
const stored = async scope => page.evaluate(scope => Object.keys(sessionStorage).filter(k=>k.startsWith('buildry.draft.v1:')&&k.includes(scope)).map(k=>({key:k,value:JSON.parse(sessionStorage[k])})),scope);
const waitStored = async (scope, field, value) => page.waitForFunction(({scope,field,value})=>Object.keys(sessionStorage).some(k=>k.startsWith('buildry.draft.v1:')&&k.includes(scope)&&JSON.parse(sessionStorage[k])[field]===value),{scope,field,value});
try {
  // Manual guest, wizard step and manual rent survive reload and browser Back.
  const leaseUrl=`${base}/leases/new?property=1&room=9001&start=2031-01-01`;
  await page.goto(leaseUrl);
  await page.getByText('Add a new guest',{exact:true}).click();
  for(const [name,value] of [['First name','Sam'],['Last name','Visitor'],['Email address','sam@example.test']]) await page.getByRole('textbox',{name,exact:true}).fill(value);
  await page.reload();
  await page.getByText('Add a new guest',{exact:true}).click();
  assert.equal(await page.getByRole('textbox',{name:'First name',exact:true}).inputValue(),'Sam');
  await page.getByRole('button',{name:'Use this guest',exact:true}).click();
  await page.getByRole('spinbutton',{name:'Monthly rent in Canadian dollars'}).fill('1475');
  await waitStored('lease-new','agreed_rent','1475');
  await page.reload();
  await page.getByRole('button',{name:'Save draft',exact:true}).waitFor();
  assert.equal(await page.getByRole('spinbutton',{name:'Monthly rent in Canadian dollars'}).inputValue(),'1475');
  await page.goto(`${base}/leases`); await page.goBack();
  await page.getByRole('button',{name:'Save draft',exact:true}).click();
  await page.getByText('Recoverable fixture failure',{exact:true}).last().waitFor();
  await page.reload();
  await page.getByRole('button',{name:'Save draft',exact:true}).waitFor();
  assert.equal(await page.getByRole('spinbutton',{name:'Monthly rent in Canadian dollars'}).inputValue(),'1475');
  account=2; await page.reload();
  await page.getByText('Add a new guest',{exact:true}).click();
  assert.equal(await page.getByRole('textbox',{name:'First name',exact:true}).inputValue(),'');
  account=1; allowed=false; await page.reload();
  await page.getByRole('button',{name:/House /}).count();
  await page.waitForFunction(()=>Object.keys(sessionStorage).some(k=>k.includes('user:1:lease-new')&&JSON.parse(sessionStorage[k]).choice_property===0));
  assert.equal((await stored('user:1:lease-new'))[0].value.choice_room,0);
  allowed=true;
  // A successful create clears recovery even if opening its returned body fails.
  await page.goto(`${base}/leases/new?tour=71`);
  await page.getByRole('button',{name:'Save draft',exact:true}).waitFor();
  fail=false; await page.getByRole('button',{name:'Save draft',exact:true}).click();
  await page.waitForFunction(()=>!Object.keys(sessionStorage).some(k=>k.includes('lease-new:?tour=71')));
  fail=true;
  console.log('PASS lease reload, Back, failure, account scope, revoked resource, success cleanup');

  await page.goto(`${base}/tasks/new`);
  await page.getByRole('textbox',{name:'Title',exact:true}).fill('Repair latch');
  await page.getByRole('button',{name:'Edit Description',exact:true}).click();
  await page.getByRole('textbox',{name:'Description',exact:true}).fill('Keep this notebook text');
  await page.getByRole('button',{name:'Save Description',exact:true}).click();
  await waitStored('task-new','description','Keep this notebook text');
  await page.reload();
  await page.getByRole('textbox',{name:'Title',exact:true}).waitFor();
  assert.equal(await page.getByRole('textbox',{name:'Title',exact:true}).inputValue(),'Repair latch');
  assert.match(await page.getByRole('button',{name:'Edit Description',exact:true}).innerText(),/Keep this notebook text/);
  await page.getByRole('button',{name:'Create task',exact:true}).click();
  await page.getByText('Recoverable fixture failure',{exact:true}).last().waitFor();
  await page.reload();
  assert.equal(await page.getByRole('textbox',{name:'Title',exact:true}).inputValue(),'Repair latch');
  fail=false; await page.getByRole('button',{name:'Create task',exact:true}).click();
  await page.waitForURL('**/tasks/91');
  assert.equal((await stored('user:1:task-new')).length,0);
  await page.goto(`${base}/tasks/new`);
  assert.equal(await page.getByRole('textbox',{name:'Title',exact:true}).inputValue(),'');
  fail=true;
  console.log('PASS task title, notebook, reload, failure, success cleanup');

  // Public booking keeps contact, date and an available time across rental dates.
  for (const owner of [1,null]) {
    account=owner;
    const bookUrl=`${base}/book?property_id=1&room_id=9001&year=2027&start=jan&months=4`;
    await page.goto(bookUrl);
    await page.locator('input[autocomplete=given-name]').fill('Booking draft');
    await page.locator('input[autocomplete=family-name]').fill('Visitor');
    await page.locator('input[autocomplete=tel]').fill('4165550100');
    await page.locator('input[autocomplete=email]').fill('draft@example.test');
    await page.getByRole('button',{name:/^Tour date:/}).click();
    await page.locator('.date-picker-day:not([disabled])').first().click();
    await page.getByRole('button',{name:/^Start time/}).click();
    await page.getByRole('option',{name:'10:00 AM',exact:true}).click();
    await waitStored('booking:1:9001','selected_slot','10:00');
    failAvailability=true; await page.reload();
    await page.getByText('Times unavailable',{exact:true}).waitFor();
    assert.equal((await stored(`${owner===null?'guest':'user:'+owner}:booking:1:9001`))[0].value.selected_slot,'10:00');
    failAvailability=false; await page.reload();
    await page.getByRole('button',{name:'Start time',exact:true}).filter({hasText:'10:00 AM'}).waitFor();
    await page.getByRole('link',{name:'Change dates',exact:true}).click();
    await page.goto(bookUrl.replace('start=jan','start=may'));
    await page.waitForFunction(()=>document.querySelector('input[autocomplete=given-name]')?.value==='Booking draft');
    await page.getByRole('button',{name:'Start time',exact:true}).filter({hasText:'10:00 AM'}).waitFor();
    // Explicit clearing also survives account prefill.
    await page.locator('input[autocomplete=given-name]').fill('');
    await waitStored('booking:1:9001','legal_first_name','');
    await page.reload();
    await page.getByRole('button',{name:'Start time',exact:true}).filter({hasText:'10:00 AM'}).waitFor();
    assert.equal(await page.locator('input[autocomplete=given-name]').inputValue(),'');
    await page.locator('input[autocomplete=given-name]').fill('Booking draft');
    await page.getByRole('button',{name:'Book tour',exact:true}).click();
    await page.getByText('Recoverable fixture failure',{exact:true}).last().waitFor();
    assert.equal(payload.rental_start,'2027-05-01');
    await page.reload();
    await page.getByRole('button',{name:'Start time',exact:true}).filter({hasText:'10:00 AM'}).waitFor();
    slots=['11:00']; await page.reload();
    await page.getByRole('button',{name:'Start time',exact:true}).filter({hasText:'Select a start time'}).waitFor();
    assert.equal(await page.getByRole('button',{name:'Book tour',exact:true}).isDisabled(),true);
    await page.getByRole('button',{name:'Start time',exact:true}).click();
    await page.getByRole('option',{name:'11:00 AM',exact:true}).click();
    fail=false; await page.getByRole('button',{name:'Book tour',exact:true}).click();
    await page.getByRole('heading',{name:'Tour booked',exact:true}).waitFor();
    assert.equal((await stored(`${owner===null?'guest':'user:'+owner}:booking:1:9001`)).length,0);
    fail=true; slots=['10:00','11:00'];
    console.log(`PASS booking ${owner??'anonymous'} date change, reload, cleared input, failure, success cleanup`);
  }
  // Corrupt or blocked browser storage cannot break normal form editing.
  account=1;
  await page.goto(`${base}/tasks/new`);
  await page.evaluate(()=>sessionStorage.setItem('buildry.draft.v1:user:1:task-new','{broken'));
  await page.reload();
  await page.getByRole('textbox',{name:'Title',exact:true}).fill('After corrupt draft');
  await waitStored('task-new','title','After corrupt draft');
  const blocked = await browser.newPage();
  const blockedErrors=[];
  blocked.on('pageerror', e=>blockedErrors.push(e.message));
  await blocked.route('**/api/**', route=> {
    const path=new URL(route.request().url()).pathname;
    if(path==='/api/me') return route.fulfill({status:401});
    if(path==='/api/ui-revision') return route.fulfill({body:'fixture'});
    if(path==='/api/tours/options') return route.fulfill({json:tourOptions});
    if(path==='/api/guest-listings') return route.fulfill({json:listings});
    throw new Error(`Unexpected blocked-storage request: ${path}`);
  });
  await blocked.addInitScript(()=>Object.defineProperty(window,'sessionStorage',{get(){throw new DOMException('Blocked','SecurityError');}}));
  await blocked.goto(`${base}/book?property_id=1&room_id=9001&year=2027&start=jan&months=4`);
  await blocked.locator('input[autocomplete=given-name]').fill('Storage blocked');
  await blocked.getByRole('button',{name:'House',exact:true}).waitFor({state:'visible'});
  assert.equal(await blocked.locator('input[autocomplete=given-name]').inputValue(),'Storage blocked');
  assert.deepEqual(blockedErrors,[]);
  await blocked.close();
  console.log('PASS corrupt and blocked storage');
  assert.deepEqual(errors,[]);
} finally { await browser.close(); await new Promise(r=>server.close(r)); }
