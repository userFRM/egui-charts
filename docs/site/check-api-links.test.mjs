import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { checkLinks } from './check-api-links.mjs';

test('resolve guide and API members as browser links and reject missing pages or anchors', () => {
  const root = mkdtempSync(join(tmpdir(), 'guide-links-'));
  try {
    const files = {
      'chapter/index.html': '<h2 id="save &amp; load">Save</h2><a name="legacy"></a>',
      'api/type.html': '<h2 id="method.update">Update</h2>',
      'api/alias.html': '<meta http-equiv="refresh" content="0;URL=type.html">',
      'api/cycle.html': '<meta http-equiv="refresh" content="0;URL=cycle.html">',
    };
    for (const [name, html] of Object.entries(files)) {
      mkdirSync(dirname(join(root, name)), { recursive: true });
      writeFileSync(join(root, name), html);
    }
    for (const [href, valid] of [
      ['chapter/?mode=read#save%20%26%20load', true],
      ['/egui-charts/docs/chapter/#legacy', true],
      ['https://userfrm.github.io/egui-charts/docs/api/type.html#method.update', true],
      ['//userfrm.github.io/egui-charts/docs/api/alias.html#method.update', true],
      ['#top', true],
      ['/egui-charts/docs/examples/widget/?scene=first-chart', true],
      ['https://docs.rs/egui-charts/', true],
      ['/egui-charts/demo/', true],
      ['chapter/missing/', false],
      ['chapter/#missing', false],
      ['api/missing.html', false],
      ['api/alias.html#method.missing', false],
      ['api/cycle.html', false],
      ['chapter/#%broken', false],
    ]) {
      writeFileSync(join(root, 'index.html'), `<a href="${href}">Link</a>`);
      const { errors } = checkLinks(root);
      assert.equal(errors.length, valid ? 0 : 1, `${href}: ${errors.join('\n')}`);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('report a code span naming an API member only when its page has no link to that member', () => {
  const root = mkdtempSync(join(tmpdir(), 'guide-members-'));
  try {
    const chart = 'api/egui_charts/widget/struct.Chart.html';
    mkdirSync(dirname(join(root, chart)), { recursive: true });
    writeFileSync(join(root, chart), '<h2 id="method.bars">Bars</h2><span id="structfield.name"></span>');
    const link = `<a href="/egui-charts/docs/${chart}#method.bars">bars</a>`;
    for (const [page, html, errors] of [
      ['guide/index.html', '<code>Chart::bars</code>', 1],
      ['guide/index.html', `<code>Chart::bars</code> ${link}`, 0],
      ['guide/index.html', `<a href="/egui-charts/docs/${chart}">Chart</a> <code>Chart::bars</code>`, 1],
      ['guide/index.html', '<code>widget::Chart::bars(&amp;self)</code> <code>Chart::name</code>', 2],
      ['guide/index.html', '<pre><code>Chart::bars</code></pre>', 0],
      ['guide/index.html', '<code>Chart::missing</code> <code>Color32::PLACEHOLDER</code> <code>Chart</code>', 0],
      ['release-notes/index.html', '<code>Chart::bars</code>', 0],
    ]) {
      mkdirSync(dirname(join(root, page)), { recursive: true });
      writeFileSync(join(root, page), html);
      const result = checkLinks(root);
      assert.equal(result.errors.length, errors, `${page}: ${html}: ${result.errors.join('\n')}`);
      rmSync(join(root, page));
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
