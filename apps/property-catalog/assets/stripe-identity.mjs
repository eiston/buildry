// Identity documents go directly to Stripe through its official browser SDK.
const card = document.querySelector('[data-stripe-identity]');
if (card) {
  const endpoint = card.dataset.endpoint;
  const form = card.querySelector('[data-identity-form]');
  const button = form.querySelector('button');
  const refresh = card.querySelector('[data-identity-refresh]');
  const status = card.querySelector('[data-identity-status]');
  let busy = false;
  let sdk;
  const messages = {
    not_started: '', requires_input: 'Verification needs your input.',
    processing: 'Stripe is checking your ID.', verified: 'ID verified by Stripe. Awaiting Buildry approval.',
    canceled: 'Verification canceled. Contact Buildry.', redacted: 'Verification removed. Contact Buildry.'
  };
  function render(state) {
    status.textContent = state.approved ? 'ID approved.' : (messages[state.status] ?? 'Verification status unavailable.');
    form.hidden = !['not_started', 'requires_input'].includes(state.status);
  }
  async function request(options) {
    const response = await fetch(endpoint, { credentials: 'same-origin', cache: 'no-store', ...options });
    if (response.redirected) throw new Error('Sign in again to continue verification.');
    if (!response.ok) throw new Error(await response.text() || 'Could not check identity verification.');
    return response.json();
  }
  function stripeSdk() {
    if (window.Stripe) return Promise.resolve(window.Stripe);
    if (!sdk) sdk = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      const timer = setTimeout(() => { script.remove(); reject(new Error('Could not load Stripe. Try again.')); }, 15000);
      script.src = 'https://js.stripe.com/endive/stripe.js';
      script.onload = () => { clearTimeout(timer); if (window.Stripe) resolve(window.Stripe); else { script.remove(); sdk = undefined; reject(new Error('Could not load Stripe. Try again.')); } };
      script.onerror = () => { clearTimeout(timer); script.remove(); sdk = undefined; reject(new Error('Could not load Stripe. Try again.')); };
      document.head.append(script);
    });
    return sdk.catch(error => { sdk = undefined; throw error; });
  }
  async function check() { render(await request()); }
  form.addEventListener('submit', async event => {
    event.preventDefault();
    if (busy || !form.reportValidity()) return;
    busy = true; button.disabled = refresh.disabled = true; status.textContent = 'Opening Stripe…';
    try {
      const Stripe = await stripeSdk();
      const session = await request({method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({consent:form.elements.consent.checked})});
      if (session.client_secret) {
        const result = await Stripe(session.publishable_key).verifyIdentity(session.client_secret);
        if (result.error) throw new Error(result.error.message || 'Verification was not completed. Try again.');
      }
      // A successful modal close is not a verified result. Always ask our server.
      await check();
    } catch (error) { status.textContent = error.message || 'Could not start verification. Try again.'; }
    finally { busy = false; button.disabled = refresh.disabled = false; }
  });
  refresh.addEventListener('click', async () => {
    if (busy) return;
    busy = true; refresh.disabled = true;
    try { await check(); } catch (error) { status.textContent = error.message; }
    finally { busy = false; refresh.disabled = false; }
  });
  check().catch(error => {status.textContent = error.message;});
}
