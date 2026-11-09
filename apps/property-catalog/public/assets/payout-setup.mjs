let sdk;
function stripeSdk() {
  sdk ??= new Promise((resolve, reject) => {
    if (window.Stripe) { resolve(window.Stripe); return; }
    const script = document.createElement('script');
    script.src = 'https://js.stripe.com/v3/';
    script.onload = () => window.Stripe ? resolve(window.Stripe) : reject(new Error('Stripe could not load.'));
    script.onerror = () => { script.remove(); reject(new Error('Stripe could not load. Try again.')); };
    document.head.append(script);
  }).catch(error => { sdk = undefined; throw error; });
  return sdk;
}

function setPath(target, path, value) {
  const parts = path.split('.');
  const last = parts.pop();
  for (const key of parts) target = target[key] ??= {};
  target[last] = value;
}

function tokenData(values, prefix) {
  const data = {};
  for (const [name, value] of values) {
    if (!name.startsWith(prefix) || !String(value).trim()) continue;
    const field = name.slice(prefix.length);
    if (field === 'dob') {
      const [year, month, day] = value.split('-').map(Number);
      if (!year || !month || !day) throw new Error('Choose a valid date of birth.');
      data.dob = { year, month, day };
    } else setPath(data, field, value === 'true' ? true : value === 'false' ? false : value);
  }
  return data;
}

async function request(body) {
  const response = await fetch('/api/stripe/connect/setup', {
    method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(await response.text() || 'Could not save payout details. Try again.');
  return response.json();
}

async function tokenize(stripe, type, data) {
  const result = await stripe.createToken(type, data);
  if (result.error) throw new Error(result.error.message || 'Check your payout details.');
  if (!result.token?.id) throw new Error('Could not secure your details. Try again.');
  return result.token.id;
}

async function saveTokens(root, form) {
  const values = new FormData(form);
  if (form.classList.contains('payout-bank-start-form')) {
    const kind=values.get('business_type');
    if (!['individual','company'].includes(kind)) throw new Error('Choose an account type.');
    await request({action:'start',business_type:kind});
    root.dataset.businessType=kind;
  }
  const defaults=JSON.parse(root.dataset.profileDefaults || '{}');
  const Stripe = await stripeSdk();
  const stripe = Stripe(root.dataset.stripeKey, { developerTools: { assistant: { enabled: false } } });
  for (const document of form.querySelectorAll('[data-payout-document]')) {
    const file = document.querySelector('input[type=file]').files[0];
    if (!file) throw new Error('Choose the required verification document.');
    if (file.size > 10 * 1024 * 1024 || !['image/jpeg','image/png','application/pdf'].includes(file.type)) {
      throw new Error('Choose a JPG, PNG or PDF document under 10 MB.');
    }
    const upload = new FormData();
    upload.append('file', file);
    upload.append('purpose', document.dataset.payoutDocument === 'registration_file' ? 'account_requirement' : 'identity_document');
    const response = await fetch('https://uploads.stripe.com/v1/files', {
      method: 'POST', headers: { Authorization: `Bearer ${root.dataset.stripeKey}` }, body: upload,
    });
    const result = await response.json();
    if (!response.ok || !result.id) throw new Error(result.error?.message || 'Could not upload the verification document.');
    values.set(document.dataset.payoutDocument, result.id);
  }
  const body = {
    action: 'save', account_token: null, bank_token: null, persons: [],
    registration_file: values.get('registration_file'), profile_url: values.get('profile_url') || defaults.profile_url, product_description: values.get('product_description') || defaults.product_description,
    support_phone: values.get('support_phone') || defaults.support_phone, mcc: values.get('mcc') || defaults.mcc, accept_terms: values.get('consent') === 'yes',
  };
  const account = tokenData(values, 'account.');
  if (values.has('consent')) {
    if (values.get('consent') !== 'yes') throw new Error('Agree to the Stripe Connected Account Agreement to continue.');
    account.tos_shown_and_accepted = true;
  }
  if (Object.keys(account).length) {
    account.business_type = root.dataset.businessType;
    account[account.business_type] ??= {};
    if (account.individual?.address) account.individual.address.country = 'CA';
    if (account.company?.address) account.company.address.country = 'CA';
    body.account_token = await tokenize(stripe, 'account', account);
  }
  const bank = tokenData(values, 'bank.');
  if (Object.keys(bank).length) {
    if (!/^\d{3}$/.test(bank.institution) || !/^\d{5}$/.test(bank.transit)) {
      throw new Error('Enter a 3-digit institution number and a 5-digit transit number.');
    }
    body.bank_token = await tokenize(stripe, 'bank_account', {
      country: 'CA', currency: 'cad', account_holder_type: root.dataset.businessType,
      account_holder_name: bank.account_holder_name, routing_number: `${bank.transit}-${bank.institution}`,
      account_number: bank.account_number,
    });
  }
  for (const person of form.querySelectorAll('[data-payout-person]')) {
    const key = person.dataset.payoutPerson;
    const data = tokenData(values, `person.${key}.`);
    if (data.address) data.address.country = 'CA';
    const role = person.dataset.personRole;
    if (role) setPath(data, `relationship.${role}`, true);
    data.metadata = { buildry_setup_key: key };
    body.persons.push({ key, id: key.startsWith('person_') ? key : null, token: await tokenize(stripe, 'person', data) });
  }
  return body;
}

const mounted = new WeakSet();
function mount(root) {
  if (mounted.has(root)) return;
  const form = root.querySelector('form');
  if (!form) return;
  mounted.add(root);
  const error = form.querySelector('.payout-form-error');
  let submit = form.querySelector('.payout-submit');
  let busy = false;
  let pending;
  const showError = message => { error.textContent = message; error.hidden = !message; error.scrollIntoView({ block: 'nearest' }); };
  form.addEventListener('click', event => {
    const button = event.target.closest('button');
    if (!busy && button && !button.classList.contains('payout-submit')) pending = undefined;
  });
  form.addEventListener('input', () => { if (!busy) pending = undefined; });
  form.addEventListener('change', () => { if (!busy) pending = undefined; });
  form.addEventListener('submit', async event => {
    event.preventDefault();
    if (busy) return;
    submit=form.querySelector('.payout-submit');
    if (!form.reportValidity()) return;
    // Shared pickers use hidden named values, so validate those explicitly.
    for (const input of form.querySelectorAll('input[type=hidden][name]')) {
      if (!input.value && (input.name === 'mcc' || input.name.endsWith('.dob') || input.name.endsWith('.address.state'))) {
        showError('Complete the required verification fields.'); return;
      }
    }
    busy = true;
    error.hidden = true;
    const controls = [...form.querySelectorAll('input,button')];
    const disabled = controls.map(control => control.disabled);
    submit?.setAttribute('aria-busy', 'true');
    // Capture values before disabling the fields so FormData includes every control.
    try {
      const tokenPromise = pending ? Promise.resolve(pending) : form.classList.contains('payout-start-form')
        ? Promise.resolve({ action: 'start', business_type: new FormData(form).get('business_type') })
        : saveTokens(root, form);
      controls.forEach(control => { control.disabled = true; });
      pending = await tokenPromise;
      await request(pending);
      pending = undefined;
      window.location.reload();
    } catch (reason) {
      controls.forEach((control,index) => { control.disabled = disabled[index]; });
      submit?.removeAttribute('aria-busy');
      showError(reason.message || 'Could not save payout details. Try again.');
      busy = false;
    }
  });
  const verify = form.querySelector('.payout-verification-button');
  verify?.addEventListener('click', async () => {
    if (busy) return;
    busy = true;
    verify.disabled = true;
    try {
      const response = await fetch('/api/stripe/connect', {
        method:'POST', credentials:'same-origin', headers:{'Content-Type':'application/json'}, body:JSON.stringify('onboard'),
      });
      if (!response.ok) throw new Error('Could not open the verification check. Try again.');
      const value = await response.json();
      window.location.assign(value.url);
    } catch (reason) { showError(reason.message); busy = false; verify.disabled = false; }
  });
}
const sync = () => document.querySelectorAll('[data-payout-setup]').forEach(mount);
new MutationObserver(sync).observe(document.body,{childList:true,subtree:true});
sync();
