// One layout and scroll-title policy for the shared page headers, including
// headers nested inside constrained forms or permission fieldsets.
let pending = false;
function updateHeaders() {
  pending = false;
  for (const header of document.querySelectorAll('.app-page-header')) {
    const sidebar = header.closest('.app-shell')?.querySelector('.sidebar');
    const left = matchMedia('(min-width: 681px)').matches ? sidebar?.getBoundingClientRect().right ?? 0 : 0;
    const width = document.documentElement.clientWidth - left;
    if (!header.getClientRects().length) continue;
    const rect = header.getBoundingClientRect();
    const margin = parseFloat(getComputedStyle(header).marginLeft) || 0;
    const offset = margin + left - rect.left;
    if (!header.closest('.notebook-screen')) {
      header.style.setProperty('--header-width', `${width}px`);
      header.style.setProperty('--header-offset', `${offset}px`);
    }
    const title = header.querySelector('.form-action-title');
    if (!title) continue;
    const scope = header.closest('main, .notebook-screen') ?? header.parentElement;
    const heading = [...scope.querySelectorAll('h1, .access-detail-head h2')].find(h => !h.closest('.app-page-header') && h.getClientRects().length);
    if (heading && title.hasAttribute('data-auto-title') && title.textContent !== heading.textContent) title.textContent = heading.textContent;
    header.classList.toggle('has-scrolled-title', heading ? heading.getBoundingClientRect().bottom <= header.getBoundingClientRect().bottom : !!title.textContent);
  }
}
function schedule() {
  if (!pending) { pending = true; requestAnimationFrame(updateHeaders); }
}
new MutationObserver(schedule).observe(document.body, { childList: true, subtree: true, characterData: true });
new ResizeObserver(schedule).observe(document.documentElement);
// Sidebar collapse changes the content boundary without resizing the window.
new MutationObserver(schedule).observe(document.body, { attributes: true, subtree: true, attributeFilter: ['class'] });
addEventListener('resize', schedule);
addEventListener('scroll', schedule, true);
schedule();
