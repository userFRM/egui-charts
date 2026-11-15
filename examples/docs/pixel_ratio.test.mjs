// An idle guide example is drawn again after a device pixel ratio change.
// It runs in a browser, not under cargo. Build and serve the release example, then run it:
//   trunk --config examples/docs/Trunk.toml build --release --locked --public-url / --dist <dist>
//   python3 -m http.server 8123 --directory <dist>
//   DOCS_EXAMPLE_URL=http://127.0.0.1:8123/ PLAYWRIGHT=<path of the playwright package> \
//     node --test examples/docs/pixel_ratio.test.mjs
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT ?? 'playwright');
const base = process.env.DOCS_EXAMPLE_URL ?? 'http://127.0.0.1:8123/';

test('idle_example_redraws_after_a_pixel_ratio_change', async () => {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({ viewport: { width: 800, height: 500 }, deviceScaleFactor: 1 });
    const page = await context.newPage();
    await page.addInitScript(() => {
      window.counts = { draws: 0, resizes: 0 };
      for (const gl of [WebGLRenderingContext, WebGL2RenderingContext]) {
        for (const name of ['drawArrays', 'drawElements']) {
          const draw = gl.prototype[name];
          gl.prototype[name] = function (...args) {
            window.counts.draws += 1;
            return draw.apply(this, args);
          };
        }
      }
      const Observer = window.ResizeObserver;
      window.ResizeObserver = class extends Observer {
        constructor(callback) {
          super((entries, observer) => {
            window.counts.resizes += 1;
            callback(entries, observer);
          });
        }
      };
    });
    const counts = () => page.evaluate(() => ({ ...window.counts }));
    const backing = () =>
      page.evaluate(() => {
        const canvas = document.getElementById('example_canvas');
        return [canvas.width, canvas.height];
      });
    await page.goto(`${base}?scene=first-chart&theme=light`);
    await page.waitForFunction(() => document.documentElement.dataset.exampleState === 'ready');
    await page.waitForTimeout(1000);
    let settled = await counts();
    await page.waitForTimeout(1000);
    assert.equal((await counts()).draws, settled.draws, 'the example is idle before the change');
    assert.deepEqual(await backing(), [800, 500], 'the backing starts at the client size');

    // Same CSS size, another ratio: what a display with another scale factor does. Going back
    // to 1 checks that the wake is armed again at each new ratio.
    const cdp = await context.newCDPSession(page);
    for (const ratio of [2, 1]) {
      await cdp.send('Emulation.setDeviceMetricsOverride', {
        width: 800,
        height: 500,
        deviceScaleFactor: ratio,
        mobile: false,
      });
      await page.waitForFunction((ratio) => window.devicePixelRatio === ratio, ratio);
      await page.waitForTimeout(1000);
      const after = await counts();
      assert.equal(after.resizes, settled.resizes, `only the pixel ratio changed to ${ratio}, not the CSS size`);
      assert.deepEqual(await backing(), [800 * ratio, 500 * ratio], `the backing follows the ratio ${ratio}`);
      assert.ok(after.draws > settled.draws, `the example is drawn again at the ratio ${ratio}`);
      await page.waitForTimeout(1000);
      settled = await counts();
      assert.equal(settled.draws, after.draws, `the example is idle again at the ratio ${ratio}`);
    }
  } finally {
    await browser.close();
  }
});
