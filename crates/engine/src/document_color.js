(report => {
  let last = '';
  // Arc takes the top color once the page has loaded and keeps it while the
  // page scrolls; a Boost change takes it again.
  let frozenTop = null;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d', {willReadFrequently: true});
  const sample = color => {
    context.fillStyle = '#000';
    context.fillStyle = color;
    context.fillRect(0, 0, 1, 1);
    return Array.from(context.getImageData(0, 0, 1, 1).data).slice(0, 3).map(value => value / 255);
  };
  // A Boost color recolors the page with two blend layers on the root, picked
  // by the page's own lightness; chrome takes the recolored page colors.
  const boosted = (rgb, top) => {
    const root = document.documentElement;
    if (!document[Symbol.for('echo.local.boost.stylesheet')]) {
      root.removeAttribute('data-echo-boost-dark');
      return [rgb, top];
    }
    const dark = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2] < 110;
    if (dark !== root.hasAttribute('data-echo-boost-dark')) root.toggleAttribute('data-echo-boost-dark', dark);
    const scale = getComputedStyle(root, '::before');
    const lift = getComputedStyle(root, '::after');
    if (scale.mixBlendMode !== 'multiply' || lift.mixBlendMode !== 'screen') return [rgb, top];
    const [m, l] = [sample(scale.backgroundColor), sample(lift.backgroundColor)];
    const map = color => color.slice(0, 3).map((value, index) => Math.round(255 * (1 - (1 - value / 255 * m[index]) * (1 - l[index]))));
    return [map(rgb), map(top)];
  };
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
    const [page, edge] = boosted(rgb, top);
    const key = `${location.href}:${page.join(',')}:${edge.join(',')}`;
    if (key !== last) { last = key; report(location.href, page[0], page[1], page[2], edge[0], edge[1], edge[2]); }
  };
  addEventListener('DOMContentLoaded', update);
  addEventListener('load', () => { frozenTop = null; update(); });
  document[Symbol.for('echo.document.color.refresh')] = () => { frozenTop = null; update(); };
  setInterval(update, 1000);
  update();
})
