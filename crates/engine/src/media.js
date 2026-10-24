// The callback and command closure stay in the renderer's native context map;
// no privileged object or browser IPC entry point is installed on the page.
(report) => {
  const handlers = new Map();
  const session = navigator.mediaSession;
  if (session) {
    const set = session.setActionHandler.bind(session);
    session.setActionHandler = function(action, handler) {
      set(action, handler);
      if (handler) handlers.set(action, handler); else handlers.delete(action);
    };
  }
  let current = null, previous = '';
  const started = new WeakSet();
  const choose = () => {
    const elements = [...document.querySelectorAll('video,audio')];
    return elements.find(e => !e.paused && !e.ended) || (elements.includes(current) ? current : elements.find(e => e.currentTime > 0)) || elements.find(e => e.readyState >= 1) || null;
  };
  const update = () => {
    current = choose();
    if (current && !current.paused && !current.ended) started.add(current);
    const metadata = session?.metadata;
    const data = {
      active: current ? started.has(current) && !current.ended : (session?.playbackState !== 'none' && !!metadata),
      playing: current ? !current.paused && !current.ended : session?.playbackState === 'playing',
      video: current?.tagName === 'VIDEO' && current.videoWidth > 0,
      title: String(metadata?.title || document.title || '').slice(0, 512),
      artist: String(metadata?.artist || '').slice(0, 512),
      artwork: String(metadata?.artwork?.[0]?.src || '').slice(0, 2048),
      previous: handlers.has('previoustrack'), next: handlers.has('nexttrack'),
      position: Math.floor(current?.currentTime || 0),
      duration: Number.isFinite(current?.duration) ? current.duration : 0,
      volume: current?.volume ?? 1
    };
    const json = JSON.stringify(data);
    if (json !== previous) { previous = json; report(json); }
  };
  for (const event of ['play','pause','ended','emptied','loadedmetadata','volumechange']) document.addEventListener(event, update, true);
  setInterval(update, 500);
  update();
  return (action, volume) => {
    current = choose();
    const name = action === 'toggle' ? ((current ? current.paused : session?.playbackState !== 'playing') ? 'play' : 'pause') : action;
    const handler = handlers.get(name);
    if (handler) handler({ action: name });
    else if (current) {
      if (name === 'play') current.play().catch(() => {});
      if (name === 'pause') current.pause();
      if (name === 'volume') current.volume = Math.min(1, Math.max(0, volume));
      if (name === 'seekbackward') current.currentTime = Math.max(0, current.currentTime - 15);
      if (name === 'seekforward') current.currentTime = Math.min(current.duration || Infinity, current.currentTime + 15);
    }
    update();
  };
}
