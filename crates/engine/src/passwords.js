// This capability is captured by native V8 and never installed on window.
(report) => {
  'use strict';
  const Input = HTMLInputElement;
  const value = Object.getOwnPropertyDescriptor(Input.prototype, 'value');
  const fields = Object.getOwnPropertyDescriptor(HTMLFormElement.prototype, 'elements').get;
  const serialize = JSON.stringify.bind(JSON), URLType = URL, InputEventType = Event;
  const origin = window.origin;
  // A sandboxed opaque frame has a URL but no authority for that URL's logins.
  if (origin === 'null' || origin !== location.origin) return () => false;
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
  // Geometry only crosses frame boundaries. No capability, origin, account or
  // secret is sent through postMessage; native CEF authenticates those separately.
  const geometryRequests = new Map();
  const anchorTag = 'echo-password-field-geometry-v1';
  const sendParent = window.postMessage.bind(parent);
  let focusedInput = null, focusedForm = null, focusedPassword = null, focusedField = 0;
  const fieldAnchor = input => new Promise(resolve => {
    const r = input.getBoundingClientRect(), rect = [r.x,r.y,r.width,r.height];
    if (parent === window) { resolve(rect);return; }
    const nonce = crypto.getRandomValues(new Uint32Array(4)).join('-');
    geometryRequests.set(nonce, {resolve});
    setTimeout(() => { const p=geometryRequests.get(nonce);geometryRequests.delete(nonce);p?.resolve?.(null); }, 300);
    sendParent({tag:anchorTag,nonce,rect}, '*');
  });
  window.addEventListener('message', event => {
    const m = event.data;
    if (!m || m.tag !== anchorTag || typeof m.nonce !== 'string' || m.nonce.length > 64) return;
    if (!Array.isArray(m.rect) || m.rect.length !== 4 || !m.rect.every(Number.isFinite)) return;
    if (m.reply) {
      if (event.source !== parent) return;
      const p = geometryRequests.get(m.nonce);geometryRequests.delete(m.nonce);
      if (p?.resolve) p.resolve(m.rect);
      else if (p?.child) p.child.postMessage({...m,reply:true}, '*');
      return;
    }
    const frame = [...document.querySelectorAll('iframe,frame')].find(f => f.contentWindow === event.source);
    if (!frame) return;
    const r = frame.getBoundingClientRect();
    const sx = frame.offsetWidth ? r.width/frame.offsetWidth : 1, sy = frame.offsetHeight ? r.height/frame.offsetHeight : 1;
    const rect = [r.x+(frame.clientLeft+m.rect[0])*sx,r.y+(frame.clientTop+m.rect[1])*sy,m.rect[2]*sx,m.rect[3]*sy];
    if (parent === window) event.source.postMessage({tag:anchorTag,nonce:m.nonce,rect,reply:true}, '*');
    else if (geometryRequests.size < 256) {
      geometryRequests.set(m.nonce,{child:event.source});
      setTimeout(() => geometryRequests.delete(m.nonce),300);
      sendParent({tag:anchorTag,nonce:m.nonce,rect}, '*');
    }
  });
  const requestFill = async (only, focus = null) => {
    for (const form of (only ? [only] : [...document.forms])) {
      const password = loginPasswords(form).find(e => !read(e) || filled.has(e));
      if (!password) continue;
      const user = username(form, password);
      const hint = user && !(focus && filledUser.get(form) === read(user)) ? read(user) : '';
      if (hint.length > 65536) return;
      const anchor = focus && user === focus ? await fieldAnchor(user) : null;
      if (focus && (document.activeElement !== focus || !visible(focus))) return;
      if (anchor && (focusedInput !== focus || focusedForm !== form || focusedPassword !== password)) {
        focusedField = (focusedField + 1) >>> 0;
        focusedInput = focus;focusedForm = form;focusedPassword = password;
      }
      report(serialize({kind:'fill',username_hint:hint,anchor,field:anchor ? focusedField : 0}));
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
      if (event.isTrusted && ['text','email'].includes(input.type)) requestFill(input.form,input);
    } catch {}
  }, true);
  const dismissField = () => {
    focusedInput = null;focusedForm = null;focusedPassword = null;
    focusedField = (focusedField+1) >>> 0;report(serialize({kind:'dismiss'}));
  };
  document.addEventListener('focusout', event => {
    // A native account window may temporarily take platform focus. Preserve the
    // exact field capability across that blur; a DOM focus move still revokes it.
    if (event.target === focusedInput && (event.relatedTarget || document.hasFocus())) dismissField();
  }, true);
  window.addEventListener('scroll', () => report(serialize({kind:'dismiss'})), true);
  document.addEventListener('keydown', event => {
    if (event.isTrusted && event.key === 'ArrowDown' && !event.ctrlKey && !event.metaKey && !event.altKey && event.target instanceof Input && event.target.form)
      requestFill(event.target.form,event.target);
  },true);
  document.addEventListener('mousedown', event => {
    const input=event.target;
    if (event.isTrusted && input instanceof Input && input.form)
      setTimeout(() => { if (document.activeElement === input) requestFill(input.form,input); },0);
  },true);
  document.addEventListener('focusin', event => {
    if (focusedInput && event.target !== focusedInput) dismissField();
    try { if (event.isTrusted && navigator.userActivation.isActive && event.target instanceof Input && event.target.form) requestFill(event.target.form,event.target); } catch {}
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
  return (user, password, selected, field) => {
    if (location.origin !== origin || (selected && field !== focusedField)) return false;
    filling = true;
    let applied = false;
    try {
      for (const form of document.forms) {
        if (selected && (!focusedInput || focusedInput.form !== form || focusedForm !== form || !visible(focusedInput))) continue;
        const candidates = loginPasswords(form), target = candidates.find(e => !read(e) || (selected && filled.has(e)));
        if (!target || (selected && target !== focusedPassword)) continue;
        const input = username(form,target);
        if (selected && input !== focusedInput) continue;
        if (input && read(input) && read(input) !== user && !selected) continue;
        if (input && (!read(input) || selected)) write(input,user);
        filledUser.set(form,user);
        write(target,password);filled.add(target);applied = true;
      }
    } finally { filling = false; }
    return applied;
  };
}
