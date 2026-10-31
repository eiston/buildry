// Install the same compatibility shim inside the separate PDF worker realm.
Promise.withResolvers ||= function () {
  let resolve, reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
};
await import("./pdf.worker.mjs");
