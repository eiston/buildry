import './form-interactions.mjs';

// Server-rendered forms use the same FormSelect shell and global keyboard
// behavior as Dioxus. Keep the native control for validation and form submission.
for (const select of document.querySelectorAll('select[data-form-select]')) {
  const label = select.getAttribute('aria-label') || select.labels?.[0]?.textContent.trim() || '';
  const group = document.createElement('div');
  group.className = 'form-select';
  const opener = document.createElement('button');
  opener.type = 'button';
  opener.className = 'form-select-button';
  opener.setAttribute('aria-label', label);
  opener.setAttribute('aria-haspopup', 'listbox');
  opener.setAttribute('aria-expanded', 'false');
  opener.setAttribute('aria-controls', `${select.id}-options`);
  const value = document.createElement('span');
  value.className = 'form-select-value';
  const chevron = document.createElement('span');
  chevron.className = 'form-select-chevron';
  chevron.setAttribute('aria-hidden', 'true');
  chevron.innerHTML = '<svg class="ui-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>';
  opener.append(value, chevron);
  const labels = [...select.labels];
  select.before(group);
  group.append(opener, select);
  select.hidden = true;
  let menu, dismiss;

  function refresh() {
    value.textContent = select.selectedOptions[0]?.textContent || '';
    group.classList.toggle('is-empty', !select.value);
    opener.disabled = select.disabled;
    if (select.validity.valid) opener.removeAttribute('aria-invalid');
  }
  function close(restoreFocus = false) {
    menu?.remove(); dismiss?.remove();
    menu = dismiss = null;
    opener.classList.remove('open');
    opener.setAttribute('aria-expanded', 'false');
    if (restoreFocus) opener.focus();
  }
  function open() {
    if (menu) { close(); return; }
    dismiss = document.createElement('button');
    dismiss.type = 'button';
    dismiss.className = 'form-select-dismiss';
    dismiss.setAttribute('aria-label', `Close ${label} options`);
    dismiss.addEventListener('click', () => close());
    menu = document.createElement('div');
    menu.id = `${select.id}-options`;
    menu.className = 'form-select-menu';
    menu.setAttribute('role', 'listbox');
    menu.setAttribute('aria-label', label);
    for (const option of select.options) {
      if (!option.value) continue;
      const choice = document.createElement('button');
      choice.type = 'button';
      choice.setAttribute('role', 'option');
      choice.setAttribute('aria-selected', String(option.selected));
      choice.classList.toggle('selected', option.selected);
      choice.disabled = option.disabled;
      choice.textContent = option.textContent;
      choice.addEventListener('click', () => {
        select.value = option.value;
        select.dispatchEvent(new Event('change', { bubbles: true }));
        close(true);
      });
      menu.append(choice);
    }
    group.append(dismiss, menu);
    opener.classList.add('open');
    opener.setAttribute('aria-expanded', 'true');
  }
  opener.addEventListener('click', open);
  select.addEventListener('change', refresh);
  select.addEventListener('invalid', event => {
    event.preventDefault();
    opener.setAttribute('aria-invalid', 'true');
    opener.focus();
  });
  for (const item of labels) item.addEventListener('click', event => { event.preventDefault(); opener.focus(); });
  refresh();
}
