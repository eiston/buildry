// Keep the admin shell mounted when following links within the admin app.
if (!window.__buildryAdminNavigation) {
  window.__buildryAdminNavigation = true;
  history.scrollRestoration = 'manual';
  addEventListener('pageshow', () => {
    scrollTo(0, 0);
    requestAnimationFrame(() => scrollTo(0, 0));
  });

  const isAdminPath = (path) =>
    path === '/' ||
    ['/pricing', '/calendar', '/tours', '/vacancy'].includes(path) ||
    ['/properties/', '/leases', '/access'].some((prefix) => path.startsWith(prefix));

  document.addEventListener('click', (event) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const link = event.target instanceof Element ? event.target.closest('.app-shell a[href]') : null;
    if (!link || link.closest('.sidebar') || link.hasAttribute('download')) return;
    if (link.target && link.target !== '_self') return;

    const destination = new URL(link.href, location.href);
    if (destination.origin !== location.origin || !isAdminPath(destination.pathname)) return;
    if (destination.pathname === location.pathname && destination.search === location.search && destination.hash) return;

    event.preventDefault();
    event.stopPropagation();
    if (destination.href === location.href) return;
    history.pushState({}, '', destination.href);
    dispatchEvent(new PopStateEvent('popstate'));
    scrollTo(0, 0);
  }, true);
}
