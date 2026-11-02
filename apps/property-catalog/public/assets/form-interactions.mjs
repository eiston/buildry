// Keyboard behavior for all shared popovers; their Dioxus owners still own state.
const groups = '.form-select, .form-multiselect, .date-picker';
const trigger = '.form-select-button, .form-multiselect-trigger, .date-picker-trigger';
const dismiss = '.form-select-dismiss, .form-multiselect-dismiss, .date-picker-backdrop';
const visible = el => !el.disabled && el.getClientRects().length > 0;
let typed = '', typedAt = 0;
const closerFor = group => [...group.querySelectorAll(dismiss)].find(visible);
function focusChoice(item) {
  if (!item) return;
  item.focus({preventScroll:true});
  const menu = item.closest('.form-select-menu, .form-multiselect-menu, .date-picker-panel');
  if (!menu) return;
  const row = item.getBoundingClientRect(), bounds = menu.getBoundingClientRect();
  if (row.top < bounds.top) menu.scrollTop += row.top - bounds.top;
  else if (row.bottom > bounds.bottom) menu.scrollTop += row.bottom - bounds.bottom;
}
function focusOnOpen(group) {
  const observer = new MutationObserver(ready);
  let timer;
  function ready() {
    const items = choices(group);
    if (!closerFor(group) || !items.length) return;
    observer.disconnect(); clearTimeout(timer);
    focusChoice(items.find(el => el.getAttribute('aria-selected') === 'true') || items[0]);
  }
  observer.observe(group, {childList:true, subtree:true, attributes:true, attributeFilter:['hidden']});
  timer = setTimeout(() => observer.disconnect(), 1000);
  ready();
}

function choices(group) {
  return [...group.querySelectorAll('[role="option"], .date-picker-day')].filter(visible);
}
document.addEventListener('keydown', event => {
  const group = event.target.closest(groups);
  if (!group) return;
  const opener = group.querySelector(trigger);
  const closer = closerFor(group);
  if (event.key === 'Escape' && closer) {
    event.preventDefault(); event.stopPropagation(); closer.click(); opener?.focus({preventScroll:true}); return;
  }
  const items = choices(group);
  const current = items.indexOf(document.activeElement);
  if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key) && (current >= 0 || event.target === opener || event.target.type === 'search')) {
    event.preventDefault();
    if (!closer && event.target === opener) {
      opener.click();
      focusOnOpen(group);
      return;
    }
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : current < 0 ? 0 : (current + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
    focusChoice(items[next]);
  } else if (current >= 0 && event.key.length === 1 && event.key !== ' ' && !event.ctrlKey && !event.metaKey) {
    typed = Date.now() - typedAt > 700 ? event.key : typed + event.key;
    typedAt = Date.now();
    const found = items.find(item => item.textContent.trim().toLowerCase().startsWith(typed.toLowerCase()));
    if (found) { event.preventDefault(); focusChoice(found); }
  }
});
document.addEventListener('focusout', event => {
  const group = event.target.closest(groups);
  if (group && event.relatedTarget && !group.contains(event.relatedTarget)) closerFor(group)?.click();
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
  if (!dialog || event.defaultPrevented) return;
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
