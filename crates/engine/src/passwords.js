// This capability is captured by native V8 and never installed on window.
(report) => {
  'use strict';
  const Input = HTMLInputElement;
  const value = Object.getOwnPropertyDescriptor(Input.prototype, 'value');
  const fields = Object.getOwnPropertyDescriptor(HTMLFormElement.prototype, 'elements').get;
  const serialize = JSON.stringify.bind(JSON), URLType = URL, InputEventType = Event;
  const origin = location.origin;
  const edited = new WeakSet(), filled = new WeakSet(), lastPassword = new WeakMap();
  const filledUser = new WeakMap();
  let reporting = false, filling = false, previous = null;
  const read = input => value.get.call(input);
  const write = (input, text) => {
    value.set.call(input, text);
    input.dispatchEvent(new InputEventType('input', {bubbles:true}));
    input.dispatchEvent(new InputEventType('change', {bubbles:true}));
  };
  const visible = input => {
    if (input.disabled || input.readOnly || !input.isConnected) return false;
    if (input.checkVisibility && !input.checkVisibility({checkOpacity:true,checkVisibilityCSS:true})) return false;
    const r = input.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && r.right > 0 && r.bottom > 0 && r.left < innerWidth && r.top < innerHeight;
  };
  const controls = form => [...fields.call(form)].filter(e => e instanceof Input);
  const passwords = form => controls(form).filter(e => e.type === 'password' && visible(e) && e.autocomplete !== 'one-time-code');
  const action = form => { try { return new URLType(form.action, location.href).origin; } catch { return ''; } };
  const username = (form, password) => {
    const inputs = controls(form), before = inputs.slice(0, inputs.indexOf(password));
    const usable = e => ['text','email'].includes(e.type) && visible(e) && e.autocomplete !== 'one-time-code';
    return inputs.find(e => usable(e) && e.autocomplete.split(/\s+/).includes('username')) ||
      before.find(e => usable(e) && e.type === 'email') || before.filter(usable).at(-1) || null;
  };
  const loginPasswords = form => action(form) === origin ? passwords(form).filter(e => !e.autocomplete.split(/\s+/).includes('new-password')) : [];
  const requestFill = only => {
    for (const form of (only ? [only] : [...document.forms])) {
      const password = loginPasswords(form).find(e => !read(e));
      if (!password) continue;
      const user = username(form, password), hint = user ? read(user) : '';
      if (hint.length <= 65536) report(serialize({kind:'fill',username_hint:hint}));
      return;
    }
  };
  const documentState = () => {
    reporting = false;
    try {
      const has = [...document.forms].some(form => controls(form).some(e => e.type === 'password' && e.isConnected && (!e.checkVisibility || e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true}))));
      if (has !== previous) {
        previous = has;
        report(serialize({kind:'document',has_password_form:has}));
        if (has) requestFill();
      }
    } catch {}
  };
  document.addEventListener('input', event => {
    try {
      const input = event.target;
      if (!(input instanceof Input) || !input.form || filling) return;
      if (input.type === 'password') {
        if (event.isTrusted) { edited.add(input);filled.delete(input);lastPassword.set(input.form,input); }
      } else if (filledUser.has(input.form) && read(input) !== filledUser.get(input.form)) {
        for (const password of passwords(input.form)) if (filled.has(password)) {
          write(password, '');filled.delete(password);
        }
        filledUser.delete(input.form);
      }
    } catch {}
  }, true);
  document.addEventListener('focusin', event => {
    try { if (event.target instanceof Input && event.target.form) requestFill(event.target.form); } catch {}
  }, true);
  document.addEventListener('submit', event => {
    try {
      const form = event.target;
      if (!event.isTrusted || !(form instanceof HTMLFormElement) || action(form) !== origin) return;
      const password = lastPassword.get(form);
      if (!password || password.form !== form || !edited.has(password) || !visible(password)) return;
      const text = read(password), user = username(form,password), name = user ? read(user) : '';
      if (!text || text.length > 262144 || name.length > 65536) return;
      // A registration/change form's confirmation must agree with its new password.
      if (password.autocomplete.split(/\s+/).includes('new-password') &&
          passwords(form).some(e => e.autocomplete.split(/\s+/).includes('new-password') && read(e) !== text)) return;
      edited.delete(password);
      report(serialize({kind:'submitted',username:name,password:text,action_origin:action(form)}));
    } catch {}
  }, true);
  const start = () => {
    documentState();
    new MutationObserver(() => {
      if (!reporting) { reporting = true;setTimeout(documentState, 50); }
    }).observe(document.documentElement, {subtree:true,childList:true,attributes:true,attributeFilter:['type','style','class','hidden','disabled','readonly']});
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, {once:true}); else start();
  return (user, password) => {
    if (location.origin !== origin) return false;
    filling = true;
    let applied = false;
    try {
      for (const form of document.forms) {
        const candidates = loginPasswords(form), target = candidates.find(e => !read(e));
        if (!target) continue;
        const input = username(form,target);
        if (input && read(input) && read(input) !== user) continue;
        if (input && !read(input)) write(input,user);
        filledUser.set(form,user);
        write(target,password);filled.add(target);applied = true;
      }
    } finally { filling = false; }
    return applied;
  };
}
