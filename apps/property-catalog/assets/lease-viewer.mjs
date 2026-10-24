// PDF.js needs this API, which some embedded browsers do not provide.
Promise.withResolvers ||= function () {
  let resolve, reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
};

const initialized = new WeakSet();
let libraryPromise;
function library() {
  libraryPromise ||= import("/assets/pdfjs/pdf.mjs").then((pdfjs) => {
    pdfjs.GlobalWorkerOptions.workerSrc = "/assets/pdfjs/pdf.worker-compat.mjs";
    return pdfjs;
  });
  return libraryPromise;
}

function initializeViewer(viewer) {
  if (initialized.has(viewer)) return;
  initialized.add(viewer);
  // Both the staff view and the signing page use this single reader shell.
  if (!viewer.querySelector(".lease-pdf-stage")) {
    const progress = document.createElement("div");
    progress.className = "lease-pdf-progress";
    progress.setAttribute("role", "progressbar");
    progress.setAttribute("aria-label", "Loading lease PDF");
    progress.setAttribute("aria-valuemin", "0");
    progress.setAttribute("aria-valuemax", "100");
    const fill = document.createElement("span");
    fill.dataset.progressFill = "";
    progress.append(fill);
    const stage = document.createElement("div");
    stage.className = "lease-pdf-stage";
    stage.setAttribute("aria-label", "Lease PDF pages");
    const message = document.createElement("p");
    message.className = "lease-pdf-message";
    message.setAttribute("role", "status");
    message.textContent = "Loading the lease PDF…";
    stage.append(message);
    viewer.append(progress, stage);
  }
  const url = viewer.dataset.document || viewer.querySelector(".lease-pdf-open")?.href;
  const stage = viewer.querySelector(".lease-pdf-stage");
  const message = viewer.querySelector(".lease-pdf-message");
  const progress = viewer.querySelector(".lease-pdf-progress");
  const progressFill = viewer.querySelector("[data-progress-fill]");
  const count = viewer.querySelector("[data-page-count], .lease-pdf-page-count");
  const previous = viewer.querySelector('[data-page="previous"], .lease-pdf-previous');
  const next = viewer.querySelector('[data-page="next"], .lease-pdf-next');
  const signed = viewer.dataset.signed === "true" || viewer.classList.contains("signed");
  const signatureKind = viewer.dataset.signatureKind || "tenant";
  const pages = [];
  let pdf, previewSignature = "", currentPage = signed ? 7 : 1;
  let progressFrame = 0;

  function errorState() {
    if (!message) return;
    if (progress) progress.hidden = true;
    message.hidden = false;
    const link = document.createElement("a");
    link.href = url;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    link.textContent = "Open the PDF in a new tab";
    message.replaceChildren("The PDF preview could not load. ", link, ".");
    if (count) count.textContent = "Unavailable";
    if (previous) previous.disabled = true;
    if (next) next.disabled = true;
  }

  function markSignature(canvas, number) {
    if (number !== 7 || (!signed && !previewSignature)) return;
    const context = canvas.getContext("2d");
    const y = signatureKind === "landlord" ? 164 : 272;
    context.save();
    context.setTransform(canvas.width / 765, 0, 0, canvas.height / 990, 0, 0);
    context.fillStyle = "rgba(27, 95, 180, .10)";
    context.strokeStyle = "#1b5fb4";
    context.lineWidth = 2;
    context.fillRect(313, y, 272, 48);
    context.strokeRect(313, y, 272, 48);
    if (previewSignature && !signed) {
      context.fillStyle = "#1452a6";
      context.font = '37px "Buildry Signature", cursive';
      context.fillText(previewSignature, 318, y + 35, 255);
    }
    context.restore();
  }

  async function renderPage(number, force = false) {
    const slot = pages[number - 1];
    if (!slot || (slot.rendered && !force)) return;
    if (slot.rendering) return slot.rendering;
    slot.rendering = (async () => {
      const page = await pdf.getPage(number);
      const base = page.getViewport({ scale: 1 });
      const width = Math.min(base.width * 1.3, Math.max(280, stage.clientWidth - 36));
      const viewport = page.getViewport({ scale: width / base.width });
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      const canvas = slot.canvas;
      canvas.width = Math.ceil(viewport.width * ratio);
      canvas.height = Math.ceil(viewport.height * ratio);
      canvas.style.width = `${Math.ceil(viewport.width)}px`;
      canvas.style.height = `${Math.ceil(viewport.height)}px`;
      await page.render({
        canvasContext: canvas.getContext("2d", { alpha: false }),
        viewport,
        transform: [ratio, 0, 0, ratio, 0, 0],
      }).promise;
      markSignature(canvas, number);
      slot.element.classList.add("loaded");
      slot.rendered = true;
    })();
    try { await slot.rendering; } finally { slot.rendering = null; }
  }

  function goToPage(number) {
    currentPage = Math.max(1, Math.min(pdf.numPages, number));
    const page = pages[currentPage - 1].element;
    const pageRect = page.getBoundingClientRect();
    const stageRect = stage.getBoundingClientRect();
    const horizontal = window.matchMedia("(max-width: 760px)").matches && viewer.classList.contains("lease-pdf-scroll-viewer");
    if (horizontal) {
      stage.scrollTo({ left: stage.scrollLeft + pageRect.left - stageRect.left - (stage.clientWidth - pageRect.width) / 2, behavior: "smooth" });
    } else {
      stage.scrollTo({ top: stage.scrollTop + pageRect.top - stageRect.top - 18, behavior: "smooth" });
    }
    if (count) count.textContent = `Page ${currentPage} of ${pdf.numPages}`;
    if (previous) previous.disabled = currentPage === 1;
    if (next) next.disabled = currentPage === pdf.numPages;
  }

  function updateProgress() {
    if (!pdf || !pages.length) return;
    const horizontal = window.matchMedia("(max-width: 760px)").matches && viewer.classList.contains("lease-pdf-scroll-viewer");
    const stageRect = stage.getBoundingClientRect();
    const center = horizontal ? stageRect.left + stage.clientWidth / 2 : stageRect.top + stage.clientHeight / 2;
    let distance = Infinity;
    for (let index = 0; index < pages.length; index++) {
      const rect = pages[index].element.getBoundingClientRect();
      const pageCenter = horizontal ? rect.left + rect.width / 2 : rect.top + rect.height / 2;
      if (Math.abs(pageCenter - center) < distance) {
        distance = Math.abs(pageCenter - center);
        currentPage = index + 1;
      }
    }
    if (count) count.textContent = `Page ${currentPage} of ${pdf.numPages}`;
    if (previous) previous.disabled = currentPage === 1;
    if (next) next.disabled = currentPage === pdf.numPages;
    if (!progress) return;
    const maximum = horizontal ? stage.scrollWidth - stage.clientWidth : stage.scrollHeight - stage.clientHeight;
    const position = horizontal ? stage.scrollLeft : stage.scrollTop;
    const percent = maximum > 0
      ? (100 / pdf.numPages) + (Math.max(0, position) / maximum) * (100 - 100 / pdf.numPages)
      : 100;
    const value = Math.max(0, Math.min(100, percent));
    progress.setAttribute("aria-valuenow", String(Math.round(value)));
    progress.setAttribute("aria-valuetext", `Page ${currentPage} of ${pdf.numPages}`);
    progressFill.style.width = `${value}%`;
  }

  stage?.addEventListener("scroll", () => {
    if (progressFrame) cancelAnimationFrame(progressFrame);
    progressFrame = requestAnimationFrame(() => {
      progressFrame = 0;
      updateProgress();
    });
  }, { passive: true });
  window.addEventListener("resize", updateProgress);

  previous?.addEventListener("click", () => pdf && goToPage(currentPage - 1));
  next?.addEventListener("click", () => pdf && goToPage(currentPage + 1));
  window.addEventListener("lease:preview-signature", async (event) => {
    if (event.detail?.kind !== signatureKind) return;
    previewSignature = event.detail.name || "";
    if (!pdf) return;
    try { await document.fonts.load('37px "Buildry Signature"'); } catch (_) { /* fallback font */ }
    await renderPage(7, true);
    goToPage(7);
  });

  (async () => {
    try {
      if (!url || !stage || !message) throw new Error("PDF viewer is missing its document or stage");
      const pdfjs = await library();
      pdf = await pdfjs.getDocument({ url }).promise;
      viewer.querySelector(".lease-pdf-canvas")?.remove();
      const track = document.createElement("div");
      track.className = "lease-pdf-pages";
      for (let number = 1; number <= pdf.numPages; number++) {
        const element = document.createElement("div");
        element.className = "lease-pdf-page";
        element.setAttribute("aria-label", `PDF page ${number} of ${pdf.numPages}`);
        const canvas = document.createElement("canvas");
        canvas.className = "lease-pdf-canvas";
        element.append(canvas);
        track.append(element);
        pages.push({ element, canvas, rendered: false, rendering: null });
      }
      stage.append(track);
      await renderPage(currentPage);
      message.hidden = true;
      if (progress) {
        progress.classList.add("ready");
        progress.setAttribute("aria-label", "Lease PDF reading progress");
      }
      updateProgress();
      if (count) count.textContent = `Page ${currentPage} of ${pdf.numPages}`;
      if (previous) previous.disabled = currentPage === 1;
      if (next) next.disabled = currentPage === pdf.numPages;
      if (signed) goToPage(Math.min(7, pdf.numPages));
      if ("IntersectionObserver" in window) {
        const observer = new IntersectionObserver((entries) => {
          for (const entry of entries) {
            if (!entry.isIntersecting) continue;
            const number = pages.findIndex((slot) => slot.element === entry.target) + 1;
            observer.unobserve(entry.target);
            if (number) renderPage(number).catch((error) => {
              console.error("Lease PDF page failed", error);
              errorState();
            });
          }
        }, { root: stage, rootMargin: "400px 200px" });
        pages.forEach((slot) => observer.observe(slot.element));
      } else {
        for (let number = 1; number <= pdf.numPages; number++) {
          if (!pages[number - 1].rendered) await renderPage(number);
        }
      }
    } catch (error) {
      console.error("Lease PDF preview failed", error);
      errorState();
    }
  })();
}

function initializeViewers() {
  document.querySelectorAll(".lease-pdf-viewer").forEach(initializeViewer);
}
if (document.body) {
  initializeViewers();
  new MutationObserver(initializeViewers).observe(document.body, { childList: true, subtree: true });
} else {
  document.addEventListener("DOMContentLoaded", initializeViewers, { once: true });
}
