function appearanceFor(root) {
  const styles = getComputedStyle(root);
  const token = (name) => styles.getPropertyValue(name).trim();
  return {
    theme: "stripe",
    variables: {
      colorPrimary: token("--focus-ring"),
      colorText: "#234b33",
      colorTextSecondary: token("--text-secondary"),
      colorTextPlaceholder: "#899b90",
      colorBackground: token("--field-background"),
      colorDanger: token("--danger"),
      borderRadius: token("--control-radius"),
      fontFamily: "Manrope, DM Sans, sans-serif",
      fontSizeBase: "16px",
      spacingUnit: "4px",
    },
    rules: {
      ".Input": {
        backgroundColor: "transparent",
        border: "0",
        boxShadow: "0 1px 0 #e3ebe6",
        padding: "10px 12px",
        minHeight: token("--control-height"),
        fontWeight: "500",
      },
      ".Input:focus": {
        backgroundColor: "#eaf4ed",
        boxShadow: "inset 0 -2px 0 " + token("--focus-ring"),
      },
      ".Input--invalid": { boxShadow: "inset 0 -2px 0 " + token("--danger") },
      ".Label": { color: "#55705c", fontSize: "12px", fontWeight: "700", marginLeft: "12px" },
    },
  };
}

let sdk;
function stripeSdk() {
  sdk ??= new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = "https://js.stripe.com/endive/stripe.js";
    script.onload = () => window.Stripe ? resolve(window.Stripe) : reject();
    script.onerror = () => {
      script.remove();
      reject();
    };
    document.head.append(script);
  }).catch((error) => {
    sdk = undefined;
    throw error;
  });
  return sdk;
}

const mounted = new Map();
async function mount(root, state) {
  const error = root.querySelector(".rent-checkout-error");
  const loading = root.querySelector(".rent-checkout-loading");
  const due = root.parentElement.querySelector(".rent-checkout-due");
  const submit = root.querySelector(".rent-checkout-submit");
  const back = root.querySelector(".rent-checkout-back");
  const showError = (message) => {
    error.textContent = message;
    error.hidden = !message;
  };
  let confirming = false;
  let ready = false;
  let canConfirm = false;
  const update = () => { submit.disabled = confirming || !ready || !canConfirm; };
  try {
    const Stripe = await stripeSdk();
    if (!root.isConnected) return;
    const stripeAccount = root.dataset.stripeAccount;
    const checkout = Stripe(root.dataset.stripeKey, { ...(stripeAccount ? { stripeAccount } : {}), developerTools: { assistant: { enabled: false } } }).initCheckoutElementsSdk({
      clientSecret: root.dataset.rentCheckout,
      elementsOptions: {
        appearance: appearanceFor(root),
        fonts: [{ cssSrc: "https://fonts.googleapis.com/css2?family=Manrope:wght@500;600;700&display=swap" }],
      },
    });
    const payment = checkout.createPaymentElement({ layout: "tabs", wallets: { applePay: "never", googlePay: "never", link: "never" } });
    state.payment = payment;
    payment.on("ready", () => {
      loading.hidden = true;
      ready = true;
      update();
    });
    payment.on("loaderror", () => {
      loading.hidden = true;
      showError("Card fields couldn’t load. Go back and try again.");
    });
    payment.mount(root.querySelector(".rent-stripe-fields"));
    const syncSession = (session) => {
      const amount = session.total.total.minorUnitsAmount;
      const currency = session.currency.toLowerCase();
      const validTotal = amount === 0 && currency === "cad";
      due.textContent = new Intl.NumberFormat("en-CA", { style: "currency", currency }).format(amount / session.minorUnitsAmountDivisor);
      canConfirm = validTotal && session.canConfirm;
      if (!validTotal) showError("Autopay setup requires $0.00 due today.");
      update();
    };
    checkout.on("change", syncSession);
    const result = await checkout.loadActions();
    if (!root.isConnected) return;
    if (result.type !== "success") throw new Error();
    syncSession(result.actions.getSession());
    submit.addEventListener("click", async () => {
      if (confirming || !canConfirm) return;
      confirming = true;
      back.disabled = true;
      submit.setAttribute("aria-busy", "true");
      update();
      showError("");
      try {
        const returnUrl = new URL("/payments/settings/autopay", window.location.origin);
        returnUrl.searchParams.set("lease", root.dataset.lease);
        const confirmation = await result.actions.confirm({ redirect: "if_required" });
        if (confirmation.type !== "error") {
          window.location.assign(returnUrl.href);
          return;
        }
        showError(confirmation.error.message || "Couldn’t turn on autopay. Try again.");
      } catch {
        showError("Couldn’t turn on autopay. Try again.");
      }
      confirming = false;
      back.disabled = false;
      submit.removeAttribute("aria-busy");
      update();
    });
  } catch {
    loading.hidden = true;
    showError("Card fields couldn’t load. Go back and try again.");
  }
}

function sync() {
  for (const [root, state] of mounted) {
    if (!root.isConnected) {
      state.payment?.destroy();
      mounted.delete(root);
    }
  }
  for (const root of document.querySelectorAll("[data-rent-checkout]")) {
    if (mounted.has(root)) continue;
    const state = {};
    mounted.set(root, state);
    mount(root, state);
  }
}

new MutationObserver(sync).observe(document.body, { childList: true, subtree: true });
sync();
