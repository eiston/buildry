let sdk;
function stripeSdk() {
  sdk ??= new Promise((resolve,reject) => {
    if (window.Stripe) { resolve(window.Stripe); return; }
    const script = document.createElement('script');
    script.src = 'https://js.stripe.com/endive/stripe.js';
    script.onload = () => window.Stripe ? resolve(window.Stripe) : reject(new Error('Bank payments could not load.'));
    script.onerror = () => { script.remove(); reject(new Error('Bank payments could not load. Try again.')); };
    document.head.append(script);
  }).catch(error => { sdk = undefined; throw error; });
  return sdk;
}
const mounted = new WeakSet();
function mount(root) {
  const form = root.querySelector('form');
  if (!form || mounted.has(root)) return;
  mounted.add(root);
  let busy = false;
  form.addEventListener('submit',async event => {
    event.preventDefault();
    if (busy || !form.reportValidity()) return;
    const error = form.querySelector('[role=alert]');
    const values = new FormData(form);
    const verify = root.dataset.verify === 'true';
    if (!verify && (!/^\d{3}$/.test(values.get('institution')) || !/^\d{5}$/.test(values.get('transit')) || !/^\d{5,12}$/.test(values.get('account')))) {
      error.hidden = false;
      error.textContent = 'Check your institution, transit, and account numbers.';
      return;
    }
    busy = true;
    error.hidden = true;
    const controls = [...form.querySelectorAll('input,button')];
    controls.forEach(control => { control.disabled = true; });
    const submit = form.querySelector('[type=submit]');
    submit.setAttribute('aria-busy','true');
    try {
      const Stripe = await stripeSdk();
      if (!root.isConnected) return;
      const stripe = Stripe(root.dataset.stripeKey, { ...(root.dataset.stripeAccount ? { stripeAccount:root.dataset.stripeAccount } : {}), developerTools: { assistant: { enabled: false } } });
      const result = verify
        ? await stripe.verifyMicrodepositsForSetup(root.dataset.rentPad,{amounts:[Number(values.get('first')),Number(values.get('second'))]})
        : await stripe.confirmAcssDebitSetup(root.dataset.rentPad,{
          payment_method: {
            billing_details:{name:values.get('name'),email:values.get('email')},
            acss_debit:{institution_number:values.get('institution'),transit_number:values.get('transit'),account_number:values.get('account')},
          },
        });
      if (result.error) {
        // A response can be lost after Stripe has already accepted the authorization or verification.
        const recovered = await stripe.retrieveSetupIntent(root.dataset.rentPad).catch(() => null);
        if (recovered?.setupIntent && (recovered.setupIntent.status === 'succeeded' || (!verify && recovered.setupIntent.status === 'requires_action'))) {
          window.location.reload();
          return;
        }
        throw new Error(result.error.message || 'Could not set up bank debit. Try again.');
      }
      window.location.reload();
    } catch(reason) {
      error.hidden = false;
      error.textContent = reason.message || 'Could not set up bank debit. Try again.';
      controls.forEach(control => { control.disabled = false; });
      submit.removeAttribute('aria-busy');
      busy = false;
    }
  });
}
const sync = () => document.querySelectorAll('[data-rent-pad]').forEach(mount);
new MutationObserver(sync).observe(document.body,{childList:true,subtree:true});
sync();
