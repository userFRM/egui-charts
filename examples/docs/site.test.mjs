import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';

const site = process.env.DOCS_SITE_DIR ?? new URL('../../_site/docs/', import.meta.url).pathname;
const decode = (html) => html.replace(/<[^>]*>/g, '').replace(/&#x([0-9a-f]+);/gi, (_, hex) => String.fromCodePoint(parseInt(hex, 16)))
  .replace(/&#(\d+);/g, (_, number) => String.fromCodePoint(Number(number)))
  .replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&quot;', '"').replaceAll('&amp;', '&');

const guideScenes = readFileSync(new URL('./scenes/extensions.rs', import.meta.url), 'utf8');
const section = (from, to) => guideScenes.slice(guideScenes.indexOf(from), to && guideScenes.indexOf(to)).trim();
const own = (file) => readFileSync(new URL(file, import.meta.url), 'utf8');

const runnable = [
  ['order-flow', 'order-flow'],
  ['study-data', 'order-flow'],
  ['replay', 'recording-replay'],
  ['strategy-report', 'host-report'],
  ['host-services', 'host-report'],
];

for (const [page, scenes, setup] of [
  ['first-chart', ['first-chart'], own('./first_chart.rs')],
  ['price-scale', ['price-scale'], own('./scenes/price_scale.rs')],
  ['time-scale', ['time-scale'], own('./scenes/time_scale.rs')],
  ['panes', ['panes'], own('./scenes/panes.rs')],
  ['theming', ['theme'], own('./scenes/theme.rs')],
  ['time-zones', ['theme'], own('./scenes/theme.rs')],
  ['horizontal-scales', ['volatility-smile', 'yield-curve'], section('/// Compare', '/// Shade')],
  ['extensions', ['primitives'], `${section('/// Shade', '/// Draw each')}\n\n${section('/// Shared fixed')}`],
  ['extensions/custom-series', ['custom-series'], section('/// Draw each')],
  ...runnable.map(([page, scene]) => [page, [scene], own(`./scenes/${scene.replaceAll('-', '_')}.rs`).split('#[cfg(test)]')[0]]),
]) {
  test(`the assembled ${page} embeds the shipped scene beside its exact compiled setup`, () => {
    const html = readFileSync(resolve(site, `${page}/index.html`), 'utf8');
    assert.match(html, /class="chart-example"/);
    for (const scene of scenes) {
      assert.match(html, new RegExp(`<iframe[^>]*data-src="/egui-charts/docs/examples/widget/\\?scene=${scene}"[^>]*title="[^"]+"[^>]*loading="lazy"`));
    }
    assert.match(html, /The interactive example needs JavaScript and WebAssembly/);
    const source = setup.trim().split('\n').filter((line) => line.trim());
    const displayed = [...html.matchAll(/<div class="ec-line"[^>]*>(.*?)<\/div>/gs)]
      .map((line) => decode(line[1])).filter((line) => line.trim());
    if (page === 'theming' || page === 'time-zones') {
      // These scenes follow other setup snippets on their guide pages.
      assert.ok(displayed.join('\n').includes(source.join('\n')), 'the scene source is displayed unchanged');
    } else {
      assert.deepEqual(displayed.slice(0, source.length), source);
    }
    if (runnable.some(([name]) => name === page)) {
      assert.match(html, /class="chart-example" data-run="manual"/);
      assert.match(html, /<summary>Compiled Rust source<\/summary>/);
      assert.doesNotMatch(html, /<iframe[^>]*\ssrc=/);
    }
    const widget = resolve(site, 'examples/widget');
    assert.match(readFileSync(resolve(widget, 'index.html'), 'utf8'), /data-example-state="loading"/);
    const wasm = readdirSync(widget).filter((file) => file.endsWith('.wasm'));
    assert.equal(wasm.length, 1);
    assert.ok(statSync(resolve(widget, wasm[0])).size > 0);
  });
}
