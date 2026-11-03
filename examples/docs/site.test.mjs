import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';

const site = process.env.DOCS_SITE_DIR ?? new URL('../../_site/docs/', import.meta.url).pathname;
const decode = (html) => html.replace(/<[^>]*>/g, '').replace(/&#x([0-9a-f]+);/gi, (_, hex) => String.fromCodePoint(parseInt(hex, 16)))
  .replace(/&#(\d+);/g, (_, number) => String.fromCodePoint(Number(number)))
  .replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&quot;', '"').replaceAll('&amp;', '&');

for (const [scene, file] of [
  ['first-chart', './first_chart.rs'],
  ['price-scale', './scenes/price_scale.rs'],
  ['time-scale', './scenes/time_scale.rs'],
  ['panes', './scenes/panes.rs'],
]) {
  test(`the assembled ${scene} embeds the shipped scene beside its exact compiled setup`, () => {
    const page = readFileSync(resolve(site, `${scene}/index.html`), 'utf8');
    assert.match(page, /class="chart-example"/);
    assert.match(page, new RegExp(`<iframe[^>]*data-src="/egui-charts/docs/examples/widget/\\?scene=${scene}"[^>]*title="[^"]+"[^>]*loading="lazy"`));
    assert.match(page, /The interactive example needs JavaScript and WebAssembly/);
    const source = readFileSync(new URL(file, import.meta.url), 'utf8')
      .trim().split('\n').filter((line) => line.trim());
    const displayed = [...page.matchAll(/<div class="ec-line"[^>]*>(.*?)<\/div>/gs)]
      .map((line) => decode(line[1])).filter((line) => line.trim());
    assert.deepEqual(displayed.slice(0, source.length), source);
    const widget = resolve(site, 'examples/widget');
    assert.match(readFileSync(resolve(widget, 'index.html'), 'utf8'), /data-example-state="loading"/);
    const wasm = readdirSync(widget).filter((file) => file.endsWith('.wasm'));
    assert.equal(wasm.length, 1);
    assert.ok(statSync(resolve(widget, wasm[0])).size > 0);
  });
}
