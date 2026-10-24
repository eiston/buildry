// Keep touch events separate from pointer events: mobile browsers can cancel a
// pointer gesture when they decide the page might scroll after the long press.
const HOLD_MS = 700;
const MOVE_TOLERANCE = 9;
let pending = null;
let active = null;

function tiles(grid) {
  return [...grid.querySelectorAll(':scope > .gallery-photo:not(.gallery-drag-ghost)')];
}

function positions(items) {
  return new Map(items.map(item => [item, item.getBoundingClientRect()]));
}

function slotAt(state, x, y) {
  // offset positions describe the settled grid, even while a FLIP animation
  // moves a tile visually through a neighbouring slot.
  const grid = state.grid.getBoundingClientRect();
  const localX = x - grid.left;
  const localY = y - grid.top;
  const inset = 8;
  return state.order.find(tile =>
    localX >= tile.offsetLeft + inset &&
    localX <= tile.offsetLeft + tile.offsetWidth - inset &&
    localY >= tile.offsetTop + inset &&
    localY <= tile.offsetTop + tile.offsetHeight - inset
  );
}

function animateGrid(grid, before) {
  for (const item of tiles(grid)) {
    if (item.classList.contains('is-dragged')) continue;
    const old = before.get(item);
    if (!old) continue;
    const next = item.getBoundingClientRect();
    const dx = old.left - next.left;
    const dy = old.top - next.top;
    const sx = old.width / next.width;
    const sy = old.height / next.height;
    if (Math.abs(dx) + Math.abs(dy) + Math.abs(sx - 1) + Math.abs(sy - 1) < 0.02) continue;
    item.getAnimations().forEach(animation => animation.cancel());
    item.animate([
      { transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})` },
      { transform: 'translate(0, 0) scale(1, 1)' }
    ], { duration: 300, easing: 'cubic-bezier(.2,.8,.2,1)' });
  }
}

function placeGhost(state, x, y) {
  const grid = state.grid.getBoundingClientRect();
  state.ghost.style.left = `${x - grid.left - state.offsetX}px`;
  state.ghost.style.top = `${y - grid.top - state.offsetY}px`;
}

function startDrag(state) {
  if (pending !== state) return;
  pending = null;
  active = state;
  const rect = state.tile.getBoundingClientRect();
  state.anchorX = (state.x - rect.left) / rect.width;
  state.anchorY = (state.y - rect.top) / rect.height;
  state.offsetX = state.x - rect.left;
  state.offsetY = state.y - rect.top;
  const ghost = state.tile.cloneNode(true);
  ghost.classList.add('gallery-drag-ghost');
  ghost.classList.remove('is-cover');
  ghost.style.width = `${rect.width}px`;
  ghost.style.height = `${rect.height}px`;
  ghost.querySelector('.gallery-remove')?.remove();
  ghost.querySelector('.gallery-cover')?.remove();
  state.ghost = ghost;
  state.grid.append(ghost);
  state.tile.classList.add('is-dragged');
  state.grid.classList.add('gallery-sorting');
  if (state.kind === 'pointer') state.button.setPointerCapture(state.pointerId);
  placeGhost(state, state.x, state.y);
  if (navigator.vibrate) navigator.vibrate(15);
}

function preview(state, target) {
  const from = state.order.indexOf(state.tile);
  const to = state.order.indexOf(target);
  if (to < 0 || from === to) return;
  const before = positions(state.order);
  state.order.splice(from, 1);
  state.order.splice(to, 0, state.tile);
  state.order.forEach((tile, index) => {
    tile.style.order = index;
    tile.classList.toggle('is-cover', index === 0);
    const label = tile.querySelector('.gallery-cover');
    if (label) label.hidden = index !== 0;
  });
  animateGrid(state.grid, before);
  const rect = state.tile.getBoundingClientRect();
  state.ghost.style.width = `${rect.width}px`;
  state.ghost.style.height = `${rect.height}px`;
  state.offsetX = rect.width * state.anchorX;
  state.offsetY = rect.height * state.anchorY;
  placeGhost(state, state.x, state.y);
}

function finish(cancelled = false) {
  if (pending) {
    clearTimeout(pending.timer);
    pending = null;
  }
  if (!active) return;
  const state = active;
  active = null;
  const to = state.order.indexOf(state.tile);
  if (cancelled) {
    const before = positions(state.order);
    state.order.splice(to, 1);
    state.order.splice(state.from, 0, state.tile);
    state.order.forEach((tile, index) => {
      tile.style.order = index;
      tile.classList.toggle('is-cover', index === 0);
    });
    animateGrid(state.grid, before);
  }
  state.ghost.remove();
  if (state.kind === 'pointer' && state.button.hasPointerCapture(state.pointerId)) {
    state.button.releasePointerCapture(state.pointerId);
  }
  state.tile.classList.remove('is-dragged');
  state.grid.classList.remove('gallery-sorting');
  if (!cancelled && to !== state.from) {
    const result = state.grid.querySelector('.gallery-sort-result');
    result.value = `${state.from},${to}`;
    result.dispatchEvent(new Event('input', { bubbles: true }));
  }
  // Dioxus owns the grid markup. Its render restores classes and order on commit.
  requestAnimationFrame(() => {
    state.order.forEach(tile => {
      tile.style.order = '';
      tile.querySelector('.gallery-cover')?.removeAttribute('hidden');
    });
  });
  lastDragAt = Date.now();
}

function arm(target, x, y, kind, id) {
  if (pending || active) return;
  const button = target.closest?.('.gallery-photo .gallery-open');
  if (!button) return;
  const tile = button.closest('.gallery-photo');
  const grid = tile.closest('.gallery-grid');
  const state = {
    kind, pointerId: kind === 'pointer' ? id : undefined,
    touchId: kind === 'touch' ? id : undefined,
    tile, grid, button,
    from: tiles(grid).indexOf(tile), order: tiles(grid),
    x, y, startX: x, startY: y
  };
  state.timer = setTimeout(() => startDrag(state), HOLD_MS);
  pending = state;
}

function move(state, x, y) {
  state.x = x;
  state.y = y;
  if (pending) {
    if (Math.hypot(x - state.startX, y - state.startY) > MOVE_TOLERANCE) finish(true);
    return;
  }
  placeGhost(state, x, y);
  const target = slotAt(state, x, y);
  if (target) preview(state, target);
}

document.addEventListener('pointerdown', event => {
  if (event.pointerType === 'touch' || event.button !== 0) return;
  arm(event.target, event.clientX, event.clientY, 'pointer', event.pointerId);
});

document.addEventListener('pointermove', event => {
  const state = active || pending;
  if (!state || state.kind !== 'pointer' || event.pointerId !== state.pointerId) return;
  if (active) event.preventDefault();
  move(state, event.clientX, event.clientY);
}, { passive: false });

document.addEventListener('pointerup', event => {
  const state = active || pending;
  if (state?.kind === 'pointer' && event.pointerId === state.pointerId) finish(false);
});
document.addEventListener('pointercancel', event => {
  const state = active || pending;
  if (state?.kind === 'pointer' && event.pointerId === state.pointerId) finish(true);
});
document.addEventListener('touchstart', event => {
  if (event.touches.length !== 1) return;
  const touch = event.changedTouches[0];
  arm(event.target, touch.clientX, touch.clientY, 'touch', touch.identifier);
}, { passive: true });
document.addEventListener('touchmove', event => {
  const state = active || pending;
  if (state?.kind !== 'touch') return;
  const touch = [...event.touches].find(item => item.identifier === state.touchId);
  if (!touch) return;
  if (active) event.preventDefault();
  move(state, touch.clientX, touch.clientY);
}, { passive: false });
document.addEventListener('touchend', event => {
  const state = active || pending;
  if (state?.kind === 'touch' && [...event.changedTouches].some(item => item.identifier === state.touchId)) finish(false);
});
document.addEventListener('touchcancel', event => {
  const state = active || pending;
  if (state?.kind === 'touch' && [...event.changedTouches].some(item => item.identifier === state.touchId)) finish(true);
});
document.addEventListener('dragstart', event => {
  if (event.target.closest('.gallery-photo')) event.preventDefault();
});
document.addEventListener('contextmenu', event => {
  if (event.target.closest('.gallery-photo')) event.preventDefault();
});
document.addEventListener('click', event => {
  if (event.target.closest('.gallery-open') && lastDragAt > Date.now() - 500) {
    event.preventDefault();
    event.stopPropagation();
  }
}, true);
let lastDragAt = 0;
