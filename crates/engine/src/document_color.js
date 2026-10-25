(report => {
  let last = '';
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
    const key = `${location.href}:${rgb[0]},${rgb[1]},${rgb[2]}`;
    if (key !== last) { last = key; report(location.href, rgb[0], rgb[1], rgb[2]); }
  };
  addEventListener('DOMContentLoaded', update);
  addEventListener('load', update);
  setInterval(update, 1000);
  update();
})
