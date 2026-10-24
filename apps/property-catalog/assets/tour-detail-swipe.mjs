let gesture = null;
let settling = false;

const mobile = () => window.matchMedia("(max-width: 680px)").matches;
const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function resetStyles(card, backdrop) {
  card.style.transform = "";
  card.style.transition = "";
  card.style.willChange = "";
  backdrop.style.backgroundColor = "";
  backdrop.style.transition = "";
}

function settle(card, backdrop, close) {
  settling = true;
  const duration = reducedMotion() ? 0 : 270;
  card.style.transition = `transform ${duration}ms cubic-bezier(.2,.8,.2,1)`;
  backdrop.style.transition = `background-color ${duration}ms ease-out`;
  card.style.transform = close ? `translate3d(0, ${window.innerHeight}px, 0)` : "translate3d(0, 0, 0)";
  backdrop.style.backgroundColor = close ? "transparent" : "";

  window.setTimeout(() => {
    if (close) card.querySelector('[aria-label="Close tour details"]')?.click();
    resetStyles(card, backdrop);
    settling = false;
  }, duration + 20);
}

document.addEventListener("touchstart", (event) => {
  if (!mobile() || settling || event.touches.length !== 1) return;
  const card = event.target.closest(".tour-detail-card");
  if (!card || card.scrollTop > 0) return;
  const touch = event.touches[0];
  gesture = {
    card,
    backdrop: card.closest(".tour-detail-backdrop"),
    identifier: touch.identifier,
    x: touch.clientX,
    y: touch.clientY,
    startedAt: performance.now(),
    dragging: false,
  };
}, { passive: true });

document.addEventListener("touchmove", (event) => {
  if (!gesture) return;
  const touch = Array.from(event.touches).find((item) => item.identifier === gesture.identifier);
  if (!touch) return;
  const distanceX = touch.clientX - gesture.x;
  const distanceY = touch.clientY - gesture.y;
  if (!gesture.dragging && (distanceY <= 0 || Math.abs(distanceX) >= distanceY || gesture.card.scrollTop > 0)) return;

  if (event.cancelable) event.preventDefault();
  gesture.dragging = true;
  const offset = Math.max(0, distanceY);
  const progress = Math.min(offset / Math.max(gesture.card.clientHeight, 1), 1);
  gesture.card.style.transition = "none";
  gesture.card.style.willChange = "transform";
  gesture.card.style.transform = `translate3d(0, ${offset}px, 0)`;
  gesture.backdrop.style.transition = "none";
  gesture.backdrop.style.backgroundColor = `rgba(13, 41, 41, ${0.64 * (1 - progress * 0.75)})`;
}, { passive: false });

document.addEventListener("touchend", (event) => {
  if (!gesture) return;
  const touch = Array.from(event.changedTouches).find((item) => item.identifier === gesture.identifier);
  if (!touch) return;
  const current = gesture;
  gesture = null;
  if (!current.dragging) return;
  const distanceY = Math.max(0, touch.clientY - current.y);
  const velocity = distanceY / Math.max(performance.now() - current.startedAt, 1);
  const close = distanceY > Math.min(160, current.card.clientHeight * 0.25) || (distanceY > 45 && velocity > 0.65);
  settle(current.card, current.backdrop, close);
}, { passive: true });

document.addEventListener("touchcancel", () => {
  if (gesture?.dragging) settle(gesture.card, gesture.backdrop, false);
  gesture = null;
}, { passive: true });
