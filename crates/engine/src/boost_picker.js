(send) => {
  let target = null;
  const marker = document.createElement('div');
  Object.assign(marker.style, {position:'fixed',pointerEvents:'none',zIndex:'2147483647',border:'2px solid #7368ff',background:'rgba(115,104,255,.12)',boxSizing:'border-box',display:'none',borderRadius:'3px'});
  document.documentElement.appendChild(marker);
  const selector = element => {
    const path = [];
    for (let node = element; node && node.nodeType === 1; node = node.parentElement) {
      if (node.id && document.querySelectorAll('#' + CSS.escape(node.id)).length === 1) { path.unshift('#' + CSS.escape(node.id)); break; }
      const siblings = node.parentElement ? [...node.parentElement.children].filter(sibling => sibling.localName === node.localName) : [node];
      path.unshift(CSS.escape(node.localName) + (siblings.length > 1 ? ':nth-of-type(' + (siblings.indexOf(node)+1) + ')' : ''));
      if (node === document.body) break;
    }
    return path.join(' > ');
  };
  const move = event => {
    if (!event.isTrusted || event.target === marker) return;
    target = event.target;
    const bounds = target.getBoundingClientRect();
    Object.assign(marker.style,{display:'block',left:bounds.left+'px',top:bounds.top+'px',width:bounds.width+'px',height:bounds.height+'px'});
  };
  const click = event => {
    if (!event.isTrusted || event.button !== 0 || !target) return;
    event.preventDefault(); event.stopImmediatePropagation();
    const value = selector(target);
    if (value && value.length <= 4096) send(value);
    marker.style.display = 'none';
  };
  const key = event => {
    if (!event.isTrusted) return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'z') { event.preventDefault(); event.stopImmediatePropagation(); send(''); }
    if (event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); cleanup(); send('\u001b'); }
  };
  const cleanup = () => { document.removeEventListener('pointermove',move,true); document.removeEventListener('click',click,true); document.removeEventListener('keydown',key,true); marker.remove(); };
  document.addEventListener('pointermove',move,true); document.addEventListener('click',click,true); document.addEventListener('keydown',key,true);
  return cleanup;
}
