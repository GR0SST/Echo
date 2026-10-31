(report => {
  let last = '';
  // Arc takes the top color once the page has loaded and keeps it while the
  // page scrolls; a Boost change takes it again.
  let frozenTop = null;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d', {willReadFrequently: true});
  const update = () => {
    if (!document.documentElement || !context) return;
    const root = getComputedStyle(document.documentElement);
    const scheme = root.colorScheme;
    context.fillStyle = (scheme === 'dark' || (scheme.includes('dark') && matchMedia('(prefers-color-scheme: dark)').matches)) ? '#121212' : '#ffffff';
    context.fillRect(0, 0, 1, 1);
    for (const node of [document.documentElement, document.body]) {
      if (!node) continue;
      context.fillStyle = getComputedStyle(node).backgroundColor;
      context.fillRect(0, 0, 1, 1);
    }
    const rgb = context.getImageData(0, 0, 1, 1).data;
    // The page's top edge (often a header bar) over the canvas color, for chrome
    // drawn directly above it.
    let top = frozenTop;
    if (!top) {
      const stack = document.elementsFromPoint ? document.elementsFromPoint(innerWidth / 2, 0) : [];
      for (const node of stack.reverse()) {
        if (node === document.documentElement || node === document.body) continue;
        context.fillStyle = getComputedStyle(node).backgroundColor;
        context.fillRect(0, 0, 1, 1);
      }
      top = Array.from(context.getImageData(0, 0, 1, 1).data);
      if (document.readyState === 'complete') frozenTop = top;
    }
    const key = `${location.href}:${rgb[0]},${rgb[1]},${rgb[2]}:${top[0]},${top[1]},${top[2]}`;
    if (key !== last) { last = key; report(location.href, rgb[0], rgb[1], rgb[2], top[0], top[1], top[2]); }
  };
  addEventListener('DOMContentLoaded', update);
  addEventListener('load', () => { frozenTop = null; update(); });
  document[Symbol.for('echo.document.color.refresh')] = () => { frozenTop = null; update(); };
  setInterval(update, 1000);
  update();
})
