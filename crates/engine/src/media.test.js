import { expect, test } from 'bun:test';

const factory = (0, eval)(await Bun.file(new URL('./media.js', import.meta.url)).text());

function fixture(elements, run) {
  const descriptors = new Map(['navigator', 'document', 'setInterval'].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  const events = new Map(), reports = [], nativeHandlers = new Map();
  const session = {
    metadata: { title: 'Session metadata' }, playbackState: 'paused',
    setActionHandler(name, handler) { nativeHandlers.set(name, handler); },
  };
  try {
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { mediaSession: session } });
    Object.defineProperty(globalThis, 'document', { configurable: true, value: {
      title: 'Document title', querySelectorAll: () => elements,
      addEventListener: (name, handler) => events.set(name, handler),
    } });
    Object.defineProperty(globalThis, 'setInterval', { configurable: true, value: () => 0 });
    const command = factory(json => reports.push(JSON.parse(json)));
    run({ session, command, reports, events, nativeHandlers });
  } finally {
    for (const [key, descriptor] of descriptors) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  }
}

test('session-only media resumes through its play handler, then pauses', () => {
  fixture([], ({ session, command, reports }) => {
    const calls = [];
    session.setActionHandler('play', () => { calls.push('play'); session.playbackState = 'playing'; });
    session.setActionHandler('pause', () => { calls.push('pause'); session.playbackState = 'paused'; });
    command('toggle');
    expect(calls).toEqual(['play']);
    expect(reports.at(-1)).toMatchObject({ active: true, playing: true });
    command('toggle');
    expect(calls).toEqual(['play', 'pause']);
    expect(reports.at(-1)).toMatchObject({ active: true, playing: false });
  });
});

test('DOM transport uses current playback state and removes stale track handlers', () => {
  const calls = [];
  const media = { paused: false, ended: false, tagName: 'AUDIO', readyState: 4, currentTime: 1, duration: 30, volume: 1,
    play() { calls.push('play'); this.paused = false; return Promise.resolve(); },
    pause() { calls.push('pause'); this.paused = true; },
  };
  fixture([media], ({ session, command, reports, nativeHandlers }) => {
    session.setActionHandler('nexttrack', () => calls.push('old track'));
    session.setActionHandler('nexttrack', () => calls.push('next track'));
    command('nexttrack');
    expect(calls).toEqual(['next track']);
    expect(reports.at(-1).next).toBe(true);
    session.setActionHandler('nexttrack', null);
    command('nexttrack');
    expect(nativeHandlers.get('nexttrack')).toBeNull();
    expect(reports.at(-1).next).toBe(false);
    expect(calls).toEqual(['next track']);
    command('toggle'); command('toggle');
    expect(calls).toEqual(['next track', 'pause', 'play']);
    expect(reports.at(-1)).toMatchObject({ active: true, playing: true });
  });
});

test('native video PiP entry and exit report document state without changing playback', () => {
  const video = {paused:false,ended:false,tagName:'VIDEO',readyState:4,videoWidth:640,currentTime:3,duration:30,volume:1};
  fixture([video], ({reports,events}) => {
    expect(reports.at(-1).picture_in_picture).toBe(false);
    document.pictureInPictureElement=video;
    events.get('enterpictureinpicture')();
    expect(reports.at(-1)).toMatchObject({video:true,playing:true,picture_in_picture:true});
    document.pictureInPictureElement=null;
    events.get('leavepictureinpicture')();
    expect(reports.at(-1)).toMatchObject({playing:true,picture_in_picture:false});
    expect(video.currentTime).toBe(3);
  });
});
