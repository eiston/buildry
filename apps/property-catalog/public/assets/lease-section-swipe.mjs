// Both registers remain mounted while the track follows the finger.
if (!window.__buildryLeaseSectionSwipe) {
  window.__buildryLeaseSectionSwipe = true;

  const sections = ['/leases', '/leases/templates'];
  const mobile = matchMedia('(max-width: 680px)');
  const root = document.documentElement;
  let touchStart = null;
  let suppressClickUntil = 0;

  const currentSection = () => sections.indexOf(location.pathname);
  const setProgress = (value) => {
    const progress = Math.max(0, Math.min(1, value));
    root.style.setProperty('--lease-tab-progress', String(progress));
    root.style.setProperty('--lease-track-offset', `${-progress * 50}%`);
  };

  function settle() {
    root.classList.remove('lease-dragging');
    setProgress(Math.max(0, currentSection()));
  }

  function goToSection(path) {
    if (!sections.includes(path) || path === location.pathname) return;
    history.pushState({}, '', path);
    dispatchEvent(new PopStateEvent('popstate'));
    settle();
  }

  setProgress(Math.max(0, currentSection()));
  addEventListener('popstate', settle);

  document.addEventListener('click', (event) => {
    if (Date.now() < suppressClickUntil) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    const link = event.target.closest?.('.lease-register-page .lease-section-nav a[href]');
    if (!link || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    goToSection(new URL(link.href).pathname);
  }, true);

  document.addEventListener('touchstart', (event) => {
    touchStart = null;
    if (!mobile.matches || currentSection() < 0 || event.touches.length !== 1 || !event.target.closest?.('.lease-register-page')) return;
    if (event.target.closest?.('input, textarea, select, button, dialog, [role="dialog"], .lease-section-nav, [data-no-tab-swipe]')) return;
    const touch = event.touches[0];
    if (touch.clientX < 24 || touch.clientX > innerWidth - 24) return;
    touchStart = { x: touch.clientX, y: touch.clientY, section: currentSection(), dragging: false };
  }, { passive: true });

  document.addEventListener('touchmove', (event) => {
    if (!touchStart || event.touches.length !== 1) return;
    const touch = event.touches[0];
    const dx = touch.clientX - touchStart.x;
    const dy = touch.clientY - touchStart.y;
    if (!touchStart.dragging && (Math.abs(dx) < 10 || Math.abs(dx) < Math.abs(dy) * 1.3)) return;
    if (!sections[touchStart.section + (dx < 0 ? 1 : -1)]) return;
    event.preventDefault();
    touchStart.dragging = true;
    root.classList.add('lease-dragging');
    setProgress(touchStart.section - dx / innerWidth);
  }, { passive: false });

  document.addEventListener('touchend', (event) => {
    if (!touchStart || event.changedTouches.length !== 1) return;
    const { x, y, section, dragging } = touchStart;
    touchStart = null;
    const touch = event.changedTouches[0];
    const dx = touch.clientX - x;
    const dy = touch.clientY - y;
    if (dragging && Math.abs(dx) >= 65 && Math.abs(dx) >= Math.abs(dy) * 1.3 && sections[section + (dx < 0 ? 1 : -1)]) {
      suppressClickUntil = Date.now() + 450;
      goToSection(sections[section + (dx < 0 ? 1 : -1)]);
    } else settle();
  }, { passive: true });

  document.addEventListener('touchcancel', () => { touchStart = null; settle(); }, { passive: true });
}
