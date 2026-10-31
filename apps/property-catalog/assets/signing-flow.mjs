for (const form of document.querySelectorAll("[data-signature-form]")) {
  const name = form.querySelector('input[name="signature"]');
  const preview = form.querySelector("[data-preview-signature]");
  const display = form.querySelector("[data-sign-preview]");
  const submit = form.querySelector("[data-submit-signature]");

  const reset = () => {
    preview.hidden = false;
    display.hidden = true;
    submit.hidden = true;
  };

  name.addEventListener("input", reset);
  form.querySelector('input[name="consent"]').addEventListener("change", reset);

  preview.addEventListener("click", () => {
    if (!form.reportValidity()) return;
    const signature = name.value.trim();
    if (!signature) return;
    display.querySelector("strong").textContent = signature;
    display.hidden = false;
    preview.hidden = true;
    submit.hidden = false;
    window.dispatchEvent(new CustomEvent("lease:preview-signature", {
      detail: { kind: form.dataset.kind, name: signature },
    }));
    submit.focus();
  });

  form.addEventListener("submit", (event) => {
    if (submit.hidden) event.preventDefault();
  });
}
