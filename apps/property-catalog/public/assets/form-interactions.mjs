// Keyboard behavior for all shared popovers; their Dioxus owners still own state.
const groups = '.form-select, .form-multiselect, .date-picker';
const trigger = '.form-select-button, .form-multiselect-trigger, .date-picker-trigger';
const dismiss = '.form-select-dismiss, .form-multiselect-dismiss, .date-picker-backdrop';
const visible = el => !el.disabled && el.getClientRects().length > 0;
let typed = '', typedAt = 0;
function choices(group) {
  return [...group.querySelectorAll('[role="option"], .date-picker-day')].filter(visible);
}
document.addEventListener('keydown', event => {
  const group = event.target.closest(groups);
  if (!group) return;
  const opener = group.querySelector(trigger);
  const closer = group.querySelector(dismiss);
  if (event.key === 'Escape' && closer) {
    event.preventDefault(); event.stopPropagation(); closer.click(); opener?.focus(); return;
  }
  const items = choices(group);
  const current = items.indexOf(document.activeElement);
  if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key) && (current >= 0 || event.target === opener || event.target.type === 'search')) {
    event.preventDefault();
    if (!closer && event.target === opener) {
      opener.click();
      requestAnimationFrame(() => requestAnimationFrame(() => {
        const ready = choices(group);
        (ready.find(el => el.getAttribute('aria-selected') === 'true') || ready[0])?.focus();
      }));
      return;
    }
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : current < 0 ? 0 : (current + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
    items[next]?.focus(); items[next]?.scrollIntoView({block:'nearest'});
  } else if (current >= 0 && event.key.length === 1 && event.key !== ' ' && !event.ctrlKey && !event.metaKey) {
    typed = Date.now() - typedAt > 700 ? event.key : typed + event.key;
    typedAt = Date.now();
    const found = items.find(item => item.textContent.trim().toLowerCase().startsWith(typed.toLowerCase()));
    if (found) { event.preventDefault(); found.focus(); found.scrollIntoView({block:'nearest'}); }
  }
});
document.addEventListener('focusout', event => {
  const group = event.target.closest(groups);
  if (group && event.relatedTarget && !group.contains(event.relatedTarget)) group.querySelector(dismiss)?.click();
});

// Full-screen editors and galleries share modal focus containment and restoration.
let dialog = null, returnFocus = null;
const focusables = root => [...root.querySelectorAll('button, a[href], input, select, textarea, [tabindex="0"]')].filter(el => visible(el) && el.tabIndex >= 0);
new MutationObserver(() => {
  const next = [...document.querySelectorAll('[aria-modal="true"]:is([role="dialog"],[role="alertdialog"])')].filter(visible).at(-1) || null;
  if (next === dialog) return;
  if (dialog && !next) { if (returnFocus?.isConnected) returnFocus.focus(); returnFocus = null; }
  if (!dialog && next) returnFocus = document.activeElement;
  dialog = next;
  if (dialog && !dialog.contains(document.activeElement)) (dialog.querySelector('input[autofocus], textarea[autofocus]') || focusables(dialog)[0])?.focus();
}).observe(document.documentElement, {childList:true, subtree:true});
document.addEventListener('keydown', event => {
  if (!dialog) return;
  if (event.key === 'Escape') {
    const close = dialog.querySelector('.gallery-back, .stay-modal-close, .notebook-screen .back-icon-button, [aria-label^="Close"]');
    if (close) { event.preventDefault(); close.click(); }
  }
  if (event.key !== 'Tab') return;
  const items = focusables(dialog);
  if (!items.length) return;
  if (event.shiftKey && (document.activeElement === items[0] || !dialog.contains(document.activeElement))) { event.preventDefault(); items.at(-1).focus(); }
  else if (!event.shiftKey && document.activeElement === items.at(-1)) { event.preventDefault(); items[0].focus(); }
});
