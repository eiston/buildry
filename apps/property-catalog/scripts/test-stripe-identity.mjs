import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const source = await readFile(new URL('../assets/stripe-identity.mjs', import.meta.url), 'utf8');
function fixture({ state = 'processing', modalError, failStart = false, failSdk = false } = {}) {
  const calls = [];
  const button = { disabled: false };
  const refresh = { disabled: false, addEventListener: (_, fn) => { refresh.click = fn; } };
  const status = { textContent: '' };
  const form = {
    hidden: false, valid: true, elements: { consent: { checked: true } },
    reportValidity: () => form.valid, querySelector: () => button,
    addEventListener: (_, fn) => { form.submit = fn; }
  };
  const card = { dataset: { endpoint: '/lease/synthetic/identity' }, querySelector: key => ({
    '[data-identity-form]': form, '[data-identity-refresh]': refresh, '[data-identity-status]': status
  })[key] };
  const window = {};
  const document = {
    querySelector: () => card, createElement: () => ({ remove() {} }),
    head: { append(script) {
      assert.equal(script.src, 'https://js.stripe.com/endive/stripe.js');
      if (failSdk) { script.onerror(); return; }
      window.Stripe = key => {
        assert.equal(key, 'pk_test_fixture');
        return { async verifyIdentity(secret) {
          calls.push('sdk'); assert.equal(secret, 'synthetic-secret');
          return modalError ? { error: { message: modalError } } : {};
        } };
      };
      script.onload();
    } }
  };
  const fetch = async (url, options) => {
    assert.equal(url, '/lease/synthetic/identity');
    assert.equal(options.cache, 'no-store');
    calls.push(options.method || 'GET');
    if (options.method === 'POST') {
      assert.deepEqual(JSON.parse(options.body), { consent: true });
      if (failStart) return { ok: false, text: async () => 'Stripe Identity unavailable' };
      return { ok: true, json: async () => ({ status: 'requires_input', publishable_key: 'pk_test_fixture', client_secret: 'synthetic-secret' }) };
    }
    return { ok: true, json: async () => ({ status: calls.includes('POST') ? state : 'not_started', approved: false }) };
  };
  vm.runInNewContext(source, { document, window, fetch, Error, JSON, setTimeout, clearTimeout });
  return { form, button, refresh, status, calls };
}
const tick = () => new Promise(resolve => setImmediate(resolve));

test('uses Stripe SDK then trusts the server status, not modal completion', async () => {
  const f = fixture(); await tick();
  await f.form.submit({ preventDefault() {} });
  assert.deepEqual(f.calls, ['GET', 'POST', 'sdk', 'GET']);
  assert.equal(f.status.textContent, 'Stripe is checking your ID.');
  assert.equal(f.form.hidden, true);
  assert.equal(f.button.disabled, false);
});
test('verified result still awaits human approval', async () => {
  const f = fixture({ state: 'verified' }); await tick();
  await f.form.submit({ preventDefault() {} });
  assert.equal(f.status.textContent, 'ID verified by Stripe. Awaiting Buildry approval.');
});
test('consent validation prevents starting verification', async () => {
  const f = fixture(); await tick(); f.form.valid = false;
  await f.form.submit({ preventDefault() {} });
  assert.deepEqual(f.calls, ['GET']);
});
for (const [label, options, expected] of [
  ['modal cancellation', { modalError: 'Verification canceled' }, 'Verification canceled'],
  ['provider unavailable', { failStart: true }, 'Stripe Identity unavailable'],
  ['SDK load failure', { failSdk: true }, 'Could not load Stripe. Try again.']
]) test(`${label} keeps the verification action available for retry`, async () => {
  const f = fixture(options); await tick();
  await f.form.submit({ preventDefault() {} });
  assert.equal(f.status.textContent, expected);
  assert.equal(f.form.hidden, false);
  assert.equal(f.button.disabled, false);
  assert.equal(f.refresh.disabled, false);
});
