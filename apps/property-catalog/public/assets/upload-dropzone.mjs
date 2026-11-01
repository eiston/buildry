// Shared gallery, template and identity upload surface. The native input remains
// the form control, so selection works with Dioxus callbacks and multipart forms.
const selector = '[data-upload-dropzone]';
const initialized = new WeakSet();

function refresh(zone) {
  const input = zone.querySelector('input[type="file"]');
  if (!input) return;
  const selected = zone.hasAttribute('data-show-selection') && input.files?.length;
  zone.querySelector('[data-upload-title]').textContent = input.disabled
    ? 'Uploading…' : zone.dataset.uploadLabel || 'Drop photos or browse';
  zone.querySelector('[data-upload-hint]').textContent = selected
    ? [...input.files].map(file => file.name).join(', ') : zone.dataset.uploadHint || 'JPG, PNG, WebP · 5 MB each';
}

function initialize(zone) {
  if (initialized.has(zone)) return;
  const input = zone.querySelector('input[type="file"]');
  if (!input) return;
  initialized.add(zone);
  zone.classList.add('upload-dropzone');
  const content = document.createElement('span');
  content.className = 'upload-dropzone-content';
  content.innerHTML = '<svg class="ui-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg><strong data-upload-title></strong><small data-upload-hint></small>';
  zone.prepend(content);
  let depth = 0;
  zone.addEventListener('dragenter', event => {
    event.preventDefault();
    if (!zone.querySelector('input[type="file"]')?.disabled) { depth++; zone.classList.add('dragging'); }
  });
  zone.addEventListener('dragover', event => { event.preventDefault(); });
  zone.addEventListener('dragleave', () => {
    depth = Math.max(0, depth - 1);
    if (!depth) zone.classList.remove('dragging');
  });
  zone.addEventListener('drop', event => {
    event.preventDefault();
    depth = 0;
    zone.classList.remove('dragging');
    const input = zone.querySelector('input[type="file"]');
    if (!input || input.disabled || !event.dataTransfer?.files.length) return;
    if (!input.multiple && event.dataTransfer.files.length !== 1) {
      input.setCustomValidity('Choose one file.');
      input.reportValidity();
      return;
    }
    input.files = event.dataTransfer.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  });
  // Resolve the current input because Dioxus replaces it after each selection.
  zone.addEventListener('change', event => {
    if (!event.target.matches('input[type="file"]')) return;
    event.target.setCustomValidity('');
    refresh(zone);
  });
  refresh(zone);
}

function scan() {
  document.querySelectorAll(selector).forEach(zone => { initialize(zone); refresh(zone); });
}
scan();
new MutationObserver(records => {
  if (records.some(record => record.type === 'attributes' || [...record.addedNodes].some(node => node.nodeType === 1 && (node.matches?.(selector + ',input[type="file"]') || node.querySelector?.(selector))))) scan();
}).observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['disabled', 'data-upload-label', 'data-upload-hint'] });
