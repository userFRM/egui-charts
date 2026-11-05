import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import test from 'node:test';

test('reduced motion keeps the poster until play is requested and reacts to preference changes', async () => {
  const page = readFileSync(new URL('./src/content/docs/index.mdx', import.meta.url), 'utf8');
  const script = page.match(/<script is:inline set:html=\{`([\s\S]*?)`\} \/>/)?.[1];
  assert.ok(script, 'the tour must initialize its motion preference');
  const sources = [{ dataset: { src: 'tour.webm' } }, { dataset: { src: 'tour.mp4' } }];
  for (const source of sources) source.removeAttribute = key => { delete source[key]; };
  const events = {};
  let plays = 0;
  let pauses = 0;
  const video = {
    autoplay: true,
    querySelectorAll: () => sources,
    load() {},
    pause() { pauses++; },
    play() { plays++; return Promise.resolve(); },
  };
  const button = { hidden: true, addEventListener: (name, fn) => { events[name] = fn; } };
  const motion = { matches: true, addEventListener: (name, fn) => { events[name] = fn; } };
  runInNewContext(script, {
    document: { getElementById: id => id === 'tour-video' ? video : button },
    window: { matchMedia: () => motion },
  });
  assert.equal(video.autoplay, false);
  assert.equal(button.hidden, false);
  assert.equal(video.controls, false, 'native controls cannot play without a source');
  assert.equal(plays, 0);
  assert.ok(sources.every(source => !source.src), 'reduced motion must not fetch video');
  events.click();
  await Promise.resolve();
  assert.equal(plays, 1);
  assert.equal(button.hidden, true);
  assert.equal(video.controls, true);
  assert.ok(sources.every(source => source.src === source.dataset.src));
  motion.matches = false;
  events.change();
  assert.equal(video.autoplay, true);
  assert.equal(plays, 2);
  motion.matches = true;
  events.change();
  assert.equal(video.autoplay, false);
  assert.equal(pauses, 2);
  assert.ok(sources.every(source => !source.src));
});
