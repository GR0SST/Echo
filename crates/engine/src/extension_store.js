// The Store's post-M142 presentation check uses a server browser-brand flag.
// Advertise this embedder's native Store support without changing its install
// buttons, permissions, policy decisions, or verified CRX installation path.
(() => {
  const key = '_F_getIjData';
  if (Object.getOwnPropertyDescriptor(window, key)?.configurable === false) return;
  let factory = window[key];
  let wrapped;
  const compatible = original => typeof original !== 'function' ? original : function (...args) {
    const data = Reflect.apply(original, this, args);
    if (data && typeof data.Kk === 'boolean' &&
        typeof window.chrome?.webstorePrivate?.beginInstallWithManifest3 === 'function' &&
        window.chrome?.management) {
      data.Kk = true;
    }
    return data;
  };
  wrapped = compatible(factory);
  Object.defineProperty(window, key, {
    configurable: true,
    enumerable: true,
    get: () => wrapped,
    set: value => { factory = value; wrapped = compatible(value); },
  });
})();
