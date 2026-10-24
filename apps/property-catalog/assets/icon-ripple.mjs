function playRipple(control, point) {
  if (!control) return;
  if (getComputedStyle(control).position === "static") control.classList.add("icon-ripple-host");

  const bounds = control.getBoundingClientRect();
  const diameter = Math.max(bounds.width, bounds.height) * 2;
  if (control.closest(".tour-agenda-week")) {
    document.querySelectorAll(".tour-agenda-week .icon-tap-ripple").forEach((active) => active.remove());
  }
  control.querySelector(".icon-tap-ripple")?.remove();
  const ripple = document.createElement("span");
  ripple.className = "icon-tap-ripple";
  ripple.setAttribute("aria-hidden", "true");
  ripple.style.width = `${diameter}px`;
  ripple.style.height = `${diameter}px`;
  ripple.style.left = `${(point ? point.x - bounds.left : bounds.width / 2) - diameter / 2}px`;
  ripple.style.top = `${(point ? point.y - bounds.top : bounds.height / 2) - diameter / 2}px`;
  ripple.addEventListener("animationend", () => ripple.remove(), { once: true });
  setTimeout(() => ripple.remove(), 700);
  control.append(ripple);
}

document.addEventListener("pointerdown", (event) => {
  if (event.pointerType !== "touch" && event.pointerType !== "pen") return;

  const dateButton = event.target.closest(".tour-agenda-week button");
  const navButton = matchMedia("(max-width: 680px)").matches
    ? event.target.closest(".sidebar .nav-item")
    : null;
  const inputRow = event.target.closest(".inline-field, label:has(>.shared-form-input), .form-field:has(>.shared-form-input)");
  const control = dateButton?.querySelector("strong") ?? navButton ?? event.target.closest(":is(button,a)[aria-label]:has(.ui-icon)") ?? inputRow;
  const action = dateButton ?? navButton ?? control;
  if (!control || action.disabled || action.getAttribute("aria-disabled") === "true") return;
  playRipple(control, dateButton ? null : { x: event.clientX, y: event.clientY });
});

document.addEventListener("calendar-date-auto-selected", (event) => {
  const dateButton = event.target.closest(".tour-agenda-week button.selected");
  if (dateButton) playRipple(dateButton.querySelector("strong"));
}, true);
